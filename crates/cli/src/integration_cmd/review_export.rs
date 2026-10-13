//! Copy only hash-bound bytes from the selected profile's managed evidence tree.
use super::*;
use std::collections::BTreeSet;
use std::io::Read;

const LIMIT: u64 = 64 * 1024 * 1024;
const REFERENCES: usize = 20_000;

pub(super) struct Collector {
    root: PathBuf,
    destination: PathBuf,
    copied: BTreeSet<String>,
    expanded: BTreeSet<String>,
    pub attachments: Vec<Value>,
    pub missing: Vec<Value>,
    pub total: u64,
}

impl Collector {
    pub fn new(root: &Path, destination: &Path) -> Result<Self> {
        Ok(Self {
            root: root.canonicalize()?,
            destination: destination.to_owned(),
            copied: BTreeSet::new(),
            expanded: BTreeSet::new(),
            attachments: vec![],
            missing: vec![],
            total: 0,
        })
    }

    fn managed_path(&self, path: &Path) -> Result<PathBuf> {
        anyhow::ensure!(
            path.is_absolute(),
            "Native attachment must have an absolute managed path"
        );
        let relative = path
            .strip_prefix(&self.root)
            .context("Attachment is outside this product-owned evidence store")?;
        anyhow::ensure!(
            !relative.as_os_str().is_empty(),
            "Attachment is a directory"
        );
        let mut cursor = self.root.clone();
        for part in relative.components() {
            anyhow::ensure!(
                matches!(part, std::path::Component::Normal(_)),
                "Attachment path traversal rejected"
            );
            cursor.push(part);
            anyhow::ensure!(
                !fs::symlink_metadata(&cursor)?.file_type().is_symlink(),
                "Attachment symlinks are not permitted"
            );
        }
        anyhow::ensure!(
            cursor.canonicalize()? == cursor,
            "Attachment path is not canonical"
        );
        Ok(cursor)
    }

    fn retain(
        &mut self,
        bytes: &[u8],
        expected: &str,
        source: Value,
        receipt: &str,
        role: &str,
    ) -> Result<()> {
        anyhow::ensure!(
            expected.len() == 64 && expected.bytes().all(|b| b.is_ascii_hexdigit()),
            "Attachment digest is missing or invalid"
        );
        let digest = sha256_hex(bytes);
        anyhow::ensure!(digest == expected, "Attachment hash mismatch");
        anyhow::ensure!(
            self.attachments.len() < REFERENCES,
            "Export exceeds 20000 attachment references"
        );
        if !self.copied.contains(&digest) {
            anyhow::ensure!(
                self.total + bytes.len() as u64 <= LIMIT,
                "Export exceeds 64 MiB; attachment remains an explicit gap"
            );
            fs::write(self.destination.join("attachments").join(&digest), bytes)?;
            self.copied.insert(digest.clone());
            self.total += bytes.len() as u64;
        }
        self.attachments.push(
            json!({"source":source,"path":format!("attachments/{digest}"),
            "sha256":digest,"bytes":bytes.len(),"receipt_artifact":receipt,"role":role}),
        );
        Ok(())
    }

    fn copy_file(
        &mut self,
        path: &Path,
        expected: &str,
        receipt: &str,
        role: &str,
        direct: bool,
    ) -> Result<Vec<u8>> {
        let canonical = self.managed_path(path)?;
        if direct {
            anyhow::ensure!(
                canonical.parent() == Some(self.root.as_path()),
                "Receipt attachment is outside the content-addressed store"
            );
        }
        #[cfg(unix)]
        let file = {
            use rustix::fs::{open, Mode, OFlags};
            fs::File::from(open(
                &canonical,
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?)
        };
        #[cfg(not(unix))]
        let file = fs::File::open(&canonical)?;
        let meta = file.metadata()?;
        anyhow::ensure!(
            meta.is_file() && meta.len() <= LIMIT,
            "Attachment is not a bounded regular file"
        );
        let mut bytes = vec![];
        file.take(LIMIT + 1).read_to_end(&mut bytes)?;
        anyhow::ensure!(bytes.len() as u64 <= LIMIT, "Attachment exceeds 64 MiB");
        self.retain(&bytes, expected, json!(canonical), receipt, role)?;
        Ok(bytes)
    }

    fn reference(&mut self, path: &Path, hash: Option<&str>, receipt: &str, role: &str) {
        if let Err(error) = self.copy_file(path, hash.unwrap_or(""), receipt, role, false) {
            self.missing.push(json!({"path":path,"receipt_artifact":receipt,"role":role,"reason":error.to_string()}));
        }
    }

    fn manifest(
        &mut self,
        value: &Value,
        key: &str,
        folder: Option<&Path>,
        receipt: &str,
    ) -> Result<()> {
        let rows = value[key]
            .as_array()
            .context("Native attachment manifest is missing")?;
        anyhow::ensure!(
            rows.len() <= REFERENCES,
            "Native manifest exceeds 20000 entries"
        );
        for row in rows {
            let Some(name) = row["path"].as_str() else {
                self.missing.push(json!({"receipt_artifact":receipt,"role":key,"reason":"Native attachment path missing"}));
                continue;
            };
            let path = Path::new(name);
            let source = if let Some(folder) = folder {
                if path.is_absolute()
                    || path
                        .components()
                        .any(|c| !matches!(c, std::path::Component::Normal(_)))
                {
                    self.missing.push(json!({"path":path,"receipt_artifact":receipt,"reason":"Relative native manifest path escaped its folder"}));
                    continue;
                }
                folder.join(path)
            } else {
                path.to_owned()
            };
            self.reference(&source, row["sha256"].as_str(), receipt, key);
        }
        Ok(())
    }

    fn envelope(&mut self, value: &Value, receipt: &str) -> Result<()> {
        if value["plan"]["pack"] == "cargo-mutants"
            && value["admission"].is_object()
            && value["report"]["facts"]["native_artifact_closure"]["schema"]
                != "thruline.cargo-mutants-artifacts.v1"
        {
            // Older captures committed to outcomes.json without committing
            // to its referenced log/diff children. Preserve the historical
            // bytes; mutable staging paths cannot repair that provenance.
            self.missing.push(json!({"receipt_artifact":receipt,"role":"mutation-native-closure",
                "reason":"This mutation capture has no sealed native log/diff closure. Raw receipt and outcomes bytes are retained; referenced staging files are not retrospectively trusted."}));
        }
        match value["schema"].as_str().unwrap_or("") {
            "thruline.integration-plan.v1" if value["pack"] == "semgrep" => {
                if let Some(reference) =
                    value["environment"]["THRULINE_SEMGREP_PREPARATION"].as_str()
                {
                    let reference = engine::parse_json(reference.as_bytes())?;
                    self.reference(
                        Path::new(
                            reference["path"]
                                .as_str()
                                .context("snapshot evidence path missing")?,
                        ),
                        reference["sha256"].as_str(),
                        receipt,
                        "semgrep-source-snapshot",
                    );
                }
            }
            "thruline.semgrep-snapshot-preparation.v1" => {
                for stream in ["stdout", "stderr"] {
                    let data = value[stream]["text"]
                        .as_str()
                        .context("snapshot native stream missing")?;
                    anyhow::ensure!(
                        data.len() <= 1024 * 1024 && value[stream]["size"] == data.len(),
                        "snapshot stream exceeds bounded shape"
                    );
                    self.retain(data.as_bytes(),value[stream]["sha256"].as_str().context("snapshot stream digest missing")?,json!({"receipt":receipt,"json_pointer":format!("/{stream}/text"),"name":format!("semgrep-snapshot-{stream}.txt")}),receipt,"semgrep-snapshot-native-stream")?;
                }
                if value["complete"] != true {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"semgrep-source-snapshot","reason":"Snapshot preparation incomplete"}));
                }
            }
            "thruline.semgrep-mode-capture.v1" => {
                let runs = value["runs"]
                    .as_array()
                    .context("Semgrep mode native runs missing")?;
                anyhow::ensure!(
                    runs.len() <= 16,
                    "Semgrep native run count exceeds protocol budget"
                );
                for (index, run) in runs.iter().enumerate() {
                    for stream in ["stdout", "stderr"] {
                        let data = run[stream]["text"]
                            .as_str()
                            .context("Semgrep inline native stream missing")?;
                        anyhow::ensure!(
                            data.len() <= 262145 && run[stream]["size"] == data.len(),
                            "Semgrep stream size differs"
                        );
                        self.retain(data.as_bytes(),run[stream]["sha256"].as_str().context("Semgrep stream digest missing")?,
                            json!({"receipt":receipt,"json_pointer":format!("/runs/{index}/{stream}/text"),"name":format!("semgrep-{}-{stream}.txt",run["name"].as_str().unwrap_or("native"))}),receipt,"semgrep-mode-native-stream")?;
                    }
                }
                if value["capture_complete"] != true {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"semgrep-mode-native-capture","reason":"Native mode capture was incomplete; all captured partial streams remain preserved","details":value["capture_error"]}));
                }
            }
            "thruline.schemathesis-extensions-capture.v1" => {
                use base64::Engine;
                let artifacts = value["artifacts"]
                    .as_array()
                    .context("Schemathesis native artifacts missing")?;
                anyhow::ensure!(
                    artifacts.len() <= 32,
                    "Schemathesis artifact count exceeds bound"
                );
                let mut names = std::collections::BTreeSet::new();
                for artifact in artifacts {
                    let name = artifact["relative_path"]
                        .as_str()
                        .context("Schemathesis artifact name missing")?;
                    anyhow::ensure!(
                        !name.is_empty()
                            && !name.contains(['/', '\\'])
                            && name != "."
                            && name != ".."
                            && names.insert(name),
                        "Schemathesis artifact names are not a bounded unique inventory"
                    );
                    let encoded = artifact["base64"]
                        .as_str()
                        .context("Schemathesis native bytes missing")?;
                    anyhow::ensure!(
                        encoded.len() <= 349528,
                        "Schemathesis inline artifact exceeds bound"
                    );
                    let data = base64::engine::general_purpose::STANDARD
                        .decode(encoded)
                        .context("Invalid native artifact encoding")?;
                    anyhow::ensure!(
                        data.len() <= 262144
                            && artifact["bytes"].as_u64() == Some(data.len() as u64),
                        "Schemathesis native byte count differs"
                    );
                    if name == "native.json" && artifact["complete"] == true {
                        anyhow::ensure!(
                            value["native_raw_json"].as_str().map(str::as_bytes)
                                == Some(data.as_slice()),
                            "Schemathesis native JSON bytes differ from original report"
                        );
                    }
                    self.retain(&data, artifact["sha256"].as_str().context("Native artifact digest missing")?, json!({"receipt":receipt,"name":name,"original_bytes":artifact["original_bytes"],"original_sha256":artifact["original_sha256"]}), receipt, "schemathesis-extension-native")?;
                    if artifact["complete"] != true {
                        self.missing.push(json!({"receipt_artifact":receipt,"role":"schemathesis-extension-native","reason":"Native artifact exceeded bounded retention","name":name,"original_bytes":artifact["original_bytes"]}));
                    }
                }
                if value["capture_complete"] != true {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"schemathesis-extension-native-closure","reason":"The native attempt did not produce a complete artifact closure","details":value["capture_gaps"]}));
                }
            }
            "thruline.megalinter-capture.v1" | "thruline.megalinter-capture.v2" => {
                if value["schema"] == "thruline.megalinter-capture.v2"
                    && (value["artifact_closure"]["complete"] != true
                        || !value["artifact_closure"]["gaps"]
                            .as_array()
                            .is_some_and(Vec::is_empty))
                {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"megalinter-native-closure",
                        "reason":"The native MegaLinter attempt did not retain a complete artifact closure.",
                        "native_gaps":value["artifact_closure"]["gaps"]}));
                }
                let folder = Path::new(
                    value["artifact_directory"]
                        .as_str()
                        .context("MegaLinter native folder missing")?,
                );
                if value["schema"] == "thruline.megalinter-capture.v2" {
                    self.manifest(value, "native_artifacts", None, receipt)?;
                } else {
                    self.manifest(value, "native_artifacts", Some(folder), receipt)?;
                }
            }
            "thruline.joern-frontend-capture.v1" => {
                self.manifest(value, "artifacts", None, receipt)?;
                if value["artifact_retention"]["complete"] != true {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"joern-frontend-native-closure","reason":"Native frontend artifact retention was incomplete","details":value["artifact_retention"]["omitted"]}));
                }
            }
            "thruline.clippy-capture.v1"
            | "thruline.clippy-capture.v2"
            | "thruline.semgrep-repair-capture.v1"
            | "thruline.opengrep-repair-capture.v1"
            | "thruline.osv-guided-repair-capture.v1"
            | "thruline.osv-mode-capture.v1"
            | "thruline.repair-oracle-capture.v1"
            | "thruline.infer-mode-capture.v1"
            | "thruline.infer-capture.v1"
            | "thruline.joern-capture.v1"
            | "thruline.joern-predefined-capture.v1"
            | "thruline.openrewrite-capture.v1"
            | "thruline.openrewrite-module-capture.v1"
            | "thruline.sonar-capture.v1" => {
                self.manifest(value, "artifacts", None, receipt)?;
            }
            "thruline.native-repair-attempt-artifacts.v1" => {
                self.manifest(value, "artifacts", None, receipt)?;
                if value["complete"] != true
                    || value["gaps"].as_array().is_none_or(|g| !g.is_empty())
                {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"failed-repair-native-closure",
                        "reason":"Attempt artifact capture was incomplete", "details":value["gaps"]}));
                }
            }
            "thruline.local-agent-capture.v1" => {
                self.manifest(value, "native_artifacts", None, receipt)?;
                let proposal = &value["proposal"];
                self.reference(
                    Path::new(proposal["path"].as_str().unwrap_or("")),
                    proposal["sha256"].as_str(),
                    receipt,
                    "proposal",
                );
                for phase in ["before", "after"] {
                    let oracle = &value["oracle"][phase];
                    if oracle.is_null() {
                        continue;
                    }
                    for stream in ["stdout", "stderr"] {
                        self.reference(
                            Path::new(oracle[format!("{stream}_path")].as_str().unwrap_or("")),
                            oracle[format!("{stream}_sha256")].as_str(),
                            receipt,
                            &format!("oracle-{phase}-{stream}"),
                        );
                    }
                }
            }
            "thruline.cargo-mutants-artifacts.v1" => {
                self.manifest(value, "native_artifacts", None, receipt)?;
                if value["complete"] != true {
                    self.missing.push(json!({"receipt_artifact":receipt,"role":"mutation-native-closure",
                        "reason":"Native mutation log/diff closure was incomplete at capture","details":value["errors"]}));
                }
            }
            "thruline.ruff-repair.v1" | "thruline.native-repair.v1" => {
                if value["schema"] == "thruline.native-repair.v1" {
                    let failed = value["state"].as_str().is_some_and(|state| {
                        !matches!(
                            state,
                            "previewed" | "verified_for_named_checks" | "applied_verified_patch"
                        )
                    });
                    if failed
                        && value["attempt_artifacts"]["schema"]
                            != "thruline.native-repair-attempt-artifacts.v1"
                    {
                        self.missing.push(json!({"receipt_artifact":receipt,"role":"failed-repair-native-closure",
                            "reason":"Failed repair has no sealed attempt artifact manifest; mutable attempt directories are not retrospectively trusted"}));
                    }
                    if value["native_attachment_seal"]["complete"] == false {
                        self.missing.push(json!({"receipt_artifact":receipt,"role":"repair-attachment-seal",
                            "reason":"Native attachment sealing was incomplete", "details":value["native_attachment_seal"], "attempt_log_gaps":value["attempt_log_gaps"]}));
                    }
                }
                // Repair logs already have their own EVB artifacts. The exact
                // patch bytes are embedded in the hash-bound repair receipt;
                // mutable sibling repair directories are never trusted here.
                if let Some(diff) = value["diff"].as_str() {
                    self.retain(
                        diff.as_bytes(),
                        value["patch_sha256"].as_str().unwrap_or(""),
                        json!({"receipt":receipt,"json_pointer":"/diff","name":"change.patch"}),
                        receipt,
                        "repair-patch",
                    )?;
                }
            }
            "thruline.agent-repair.v1" => {
                // Only fields defined by the repair protocol authorize reads.
                // Snapshot paths and mutable sibling metadata are not manifests.
                for (pointer, role) in [
                    ("/capture", "source-agent-capture"),
                    ("/proposal", "agent-repair-proposal"),
                    ("/native_suggestion/response", "native-suggestion-response"),
                    ("/resume/invocation", "resume-invocation"),
                    ("/resume/events", "resume-events"),
                    (
                        "/verification/before/command/stdout",
                        "oracle-before-stdout",
                    ),
                    (
                        "/verification/before/command/stderr",
                        "oracle-before-stderr",
                    ),
                    ("/verification/after/command/stdout", "oracle-after-stdout"),
                    ("/verification/after/command/stderr", "oracle-after-stderr"),
                    (
                        "/verification/before/command/sandbox_profile",
                        "oracle-before-sandbox",
                    ),
                    (
                        "/verification/after/command/sandbox_profile",
                        "oracle-after-sandbox",
                    ),
                ] {
                    if let Some(reference) = value.pointer(pointer).filter(|v| !v.is_null()) {
                        self.reference(
                            Path::new(reference["path"].as_str().unwrap_or("")),
                            reference["sha256"].as_str(),
                            receipt,
                            role,
                        );
                    }
                }
                if !value["action_artifacts"].is_null() {
                    self.manifest(value, "action_artifacts", None, receipt)?;
                }
                if let Some(resume) = value.get("resume") {
                    for key in ["persistence_sources", "native_artifacts"] {
                        if !resume[key].is_null() {
                            self.manifest(resume, key, None, receipt)?;
                        }
                    }
                }
                if let Some(diff) = value["diff"].as_str() {
                    self.retain(
                        diff.as_bytes(),
                        value["proposal"]["sha256"].as_str().unwrap_or(""),
                        json!({"receipt":receipt,"json_pointer":"/diff","name":"proposal.diff"}),
                        receipt,
                        "repair-patch",
                    )?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn expand(
        &mut self,
        value: &Value,
        receipt: &str,
        depth: usize,
        nodes: &mut usize,
    ) -> Result<()> {
        *nodes += 1;
        anyhow::ensure!(
            *nodes <= 100_000 && depth <= 48,
            "Nested evidence exceeds bounded manifest traversal"
        );
        if value.is_object() {
            self.envelope(value, receipt)?;
        }
        // Recognize envelopes inside actual EVB record/report containers as
        // well as raw wrapper stdout, without treating generic paths as reads.
        match value {
            Value::Object(map) => {
                for child in map.values() {
                    self.expand(child, receipt, depth + 1, nodes)?;
                }
            }
            Value::Array(rows) => {
                for child in rows {
                    self.expand(child, receipt, depth + 1, nodes)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn receipt(&mut self, artifact: &ArtifactDigest) {
        self.receipt_with_expansion(artifact, true);
    }

    pub fn receipt_opaque(&mut self, artifact: &ArtifactDigest) {
        self.receipt_with_expansion(artifact, false);
    }

    fn receipt_with_expansion(&mut self, artifact: &ArtifactDigest, expand: bool) {
        let path = Path::new(&artifact.path);
        let attempt = (|| -> Result<()> {
            let bytes = self.copy_file(
                path,
                artifact.sha256.as_deref().unwrap_or(""),
                &artifact.path,
                "receipt",
                true,
            )?;
            if expand && self.expanded.insert(sha256_hex(&bytes)) && bytes.len() <= 8 * 1024 * 1024
            {
                if let Ok(value) = engine::parse_json(&bytes) {
                    self.expand(&value, &artifact.path, 0, &mut 0)?;
                }
            }
            Ok(())
        })();
        if let Err(error) = attempt {
            self.missing
                .push(json!({"path":path,"reason":error.to_string()}));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, Collector) {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap().join("evidence");
        let out = temp.path().join("export");
        fs::create_dir_all(root.join("native")).unwrap();
        fs::create_dir_all(out.join("attachments")).unwrap();
        let collector = Collector::new(&root, &out).unwrap();
        (temp, collector)
    }

    fn artifact(collector: &mut Collector, value: Value) {
        let bytes = serde_json::to_vec(&value).unwrap();
        let digest = sha256_hex(&bytes);
        let path = collector.root.join(&digest);
        fs::write(&path, bytes).unwrap();
        collector.receipt(&ArtifactDigest {
            path: path.to_string_lossy().into_owned(),
            sha256: Some(digest),
        });
    }

    #[test]
    fn semgrep_modes_export_exact_inline_streams_and_refuse_replaced_bytes() {
        let (_temp, mut collector) = setup();
        let raw =
            include_bytes!("../../../verify/tests/fixtures/semgrep_modes/baseline-fixed.json");
        let value: Value = serde_json::from_slice(raw).unwrap();
        artifact(&mut collector, value.clone());
        assert!(collector.missing.is_empty(), "{:?}", collector.missing);
        for run in value["runs"].as_array().unwrap() {
            for stream in ["stdout", "stderr"] {
                let data = run[stream]["text"].as_str().unwrap().as_bytes();
                let hash = sha256_hex(data);
                assert_eq!(
                    fs::read(collector.destination.join("attachments").join(hash)).unwrap(),
                    data
                );
            }
        }
        let mut changed = value;
        changed["runs"][0]["stdout"]["text"] = json!("different version bytes\n");
        changed["runs"][0]["stdout"]["size"] = json!("different version bytes\n".len());
        artifact(&mut collector, changed);
        assert!(!collector.missing.is_empty());
    }

    #[test]
    fn schemathesis_extension_bytes_are_exported_and_remain_after_source_replacement() {
        use base64::Engine;
        let (_temp, mut collector) = setup();
        let raw = "{\"complete\":true}\n";
        let hash = sha256_hex(raw.as_bytes());
        artifact(
            &mut collector,
            json!({"schema":"thruline.schemathesis-extensions-capture.v1","native_raw_json":raw,"capture_complete":true,
            "artifacts":[{"relative_path":"native.json","sha256":hash,"bytes":raw.len(),"original_bytes":raw.len(),"original_sha256":hash,"complete":true,"base64":base64::engine::general_purpose::STANDARD.encode(raw)}]}),
        );
        assert!(collector.missing.is_empty(), "{:?}", collector.missing);
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            raw.as_bytes()
        );
        fs::write(
            collector.root.join("native/other"),
            b"unrelated replacement",
        )
        .unwrap();
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            raw.as_bytes()
        );
    }

    #[test]
    fn schemathesis_failed_and_truncated_bytes_keep_an_explicit_gap() {
        use base64::Engine;
        let (_temp, mut collector) = setup();
        let raw = b"owned calculation stopped\n";
        let hash = sha256_hex(raw);
        artifact(
            &mut collector,
            json!({"schema":"thruline.schemathesis-extensions-capture.v1","native_raw_json":null,"capture_complete":false,"capture_gaps":["native.json was not emitted"],
            "artifacts":[{"relative_path":"native.stderr","sha256":hash,"bytes":raw.len(),"original_bytes":raw.len()+1,"complete":false,"base64":base64::engine::general_purpose::STANDARD.encode(raw)}]}),
        );
        assert_eq!(collector.missing.len(), 2);
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            raw
        );
    }

    #[test]
    fn schemathesis_inline_hash_and_original_json_mismatch_are_refused() {
        use base64::Engine;
        for (raw, digest) in [
            ("{}", sha256_hex(b"another value")),
            ("{\"n\":1}", sha256_hex(b"{\"n\":1}")),
        ] {
            let (_temp, mut collector) = setup();
            artifact(
                &mut collector,
                json!({"schema":"thruline.schemathesis-extensions-capture.v1","native_raw_json":"{}","capture_complete":true,
                "artifacts":[{"relative_path":"native.json","sha256":digest,"bytes":raw.len(),"complete":true,"base64":base64::engine::general_purpose::STANDARD.encode(raw)}]}),
            );
            assert!(!collector.missing.is_empty());
        }
    }

    #[test]
    fn known_absolute_manifests_and_nested_envelopes_export_actual_bytes() {
        let (_temp, mut collector) = setup();
        let source = collector.root.join("native/report.txt");
        let bytes = b"preserved native report bytes\n";
        fs::write(&source, bytes).unwrap();
        let hash = sha256_hex(bytes);
        let schemas = [
            "thruline.clippy-capture.v1",
            "thruline.semgrep-repair-capture.v1",
            "thruline.opengrep-repair-capture.v1",
            "thruline.osv-guided-repair-capture.v1",
            "thruline.osv-mode-capture.v1",
            "thruline.repair-oracle-capture.v1",
            "thruline.infer-capture.v1",
            "thruline.infer-mode-capture.v1",
            "thruline.joern-capture.v1",
            "thruline.joern-predefined-capture.v1",
            "thruline.openrewrite-capture.v1",
            "thruline.openrewrite-module-capture.v1",
            "thruline.sonar-capture.v1",
        ];
        for schema in schemas {
            artifact(
                &mut collector,
                json!({"record":{"report":{"capture":{"schema":schema,"artifacts":[{"path":source,"sha256":hash}]}}}}),
            );
        }
        assert!(collector.missing.is_empty(), "{:?}", collector.missing);
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            bytes
        );
        assert_eq!(
            collector
                .attachments
                .iter()
                .filter(|a| a["role"] == "artifacts")
                .count(),
            schemas.len()
        );
    }

    #[test]
    fn joern_frontend_retention_gap_preserves_available_bytes_and_missing_scope() {
        let (_temp, mut collector) = setup();
        let source = collector.root.join("native/frontend.log");
        let bytes = b"owned numeric frontend report\n";
        fs::write(&source, bytes).unwrap();
        let hash = sha256_hex(bytes);
        artifact(
            &mut collector,
            json!({"schema":"thruline.joern-frontend-capture.v1",
            "artifacts":[{"path":source,"sha256":hash}],
            "artifact_retention":{"complete":false,"omitted":[{"relative_path":"generated/table.dat","bytes":5,"reason":"bounded retention"}]}}),
        );
        assert_eq!(collector.missing.len(), 1);
        assert_eq!(
            collector.missing[0]["role"],
            "joern-frontend-native-closure"
        );
        assert_eq!(
            collector.missing[0]["details"][0]["relative_path"],
            "generated/table.dat"
        );
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            bytes
        );
        fs::write(&source, b"later unrelated bytes").unwrap();
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            bytes
        );
    }

    #[test]
    fn failed_attempt_manifest_exports_arbitrary_raw_native_bytes() {
        let (_temp, mut collector) = setup();
        let source = collector.root.join("native/failed.stdout");
        let bytes = b"native failure\n\xff\x00retained exactly";
        fs::write(&source, bytes).unwrap();
        let hash = sha256_hex(bytes);
        artifact(
            &mut collector,
            json!({"schema":"thruline.native-repair.v1", "state":"failed",
            "attempt_artifacts":{"schema":"thruline.native-repair-attempt-artifacts.v1",
            "complete":true,"gaps":[],"roots":[],"artifacts":[{"path":source,"sha256":hash,"bytes":bytes.len()}]}}),
        );
        assert!(collector.missing.is_empty(), "{:?}", collector.missing);
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(hash)).unwrap(),
            bytes
        );
    }

    #[test]
    fn failed_attempt_gaps_and_historical_missing_closure_are_not_complete() {
        let (_temp, mut collector) = setup();
        artifact(
            &mut collector,
            json!({"schema":"thruline.native-repair.v1", "state":"failed"}),
        );
        artifact(
            &mut collector,
            json!({"schema":"thruline.native-repair.v1", "state":"failed",
            "attempt_artifacts":{"schema":"thruline.native-repair-attempt-artifacts.v1",
            "complete":false,"gaps":[{"reason":"symlink refused"}],"roots":[],"artifacts":[]}}),
        );
        assert_eq!(collector.missing.len(), 2);
        assert!(collector
            .missing
            .iter()
            .all(|row| row["role"] == "failed-repair-native-closure"));
    }

    #[test]
    fn pre_execution_refusal_has_truthful_empty_native_closure() {
        let (_temp, mut collector) = setup();
        artifact(
            &mut collector,
            json!({"schema":"thruline.native-repair.v1", "state":"failed",
            "error":"Configuration refused before native execution", "attempt_artifacts":{
            "schema":"thruline.native-repair-attempt-artifacts.v1", "complete":true,
            "roots":[],"gaps":[],"artifacts":[]}}),
        );
        assert!(collector.missing.is_empty());
        assert_eq!(collector.attachments.len(), 1);
    }

    #[test]
    fn agent_proposal_and_both_oracle_traces_are_copied() {
        let (_temp, mut collector) = setup();
        let path = collector.root.join("native/trace");
        fs::write(&path, b"oracle and native bytes").unwrap();
        let hash = hash_file(&path).unwrap();
        let oracle = json!({"stdout_path":path,"stdout_sha256":hash,"stderr_path":path,"stderr_sha256":hash});
        artifact(
            &mut collector,
            json!({"schema":"thruline.local-agent-capture.v1",
            "native_artifacts":[{"path":path,"sha256":hash}],"proposal":{"path":path,"sha256":hash},
            "oracle":{"before":oracle,"after":oracle}}),
        );
        assert!(collector.missing.is_empty());
        assert_eq!(collector.attachments.len(), 7);
        assert_eq!(collector.copied.len(), 2);
    }

    #[test]
    fn historical_mutation_record_retains_bytes_but_discloses_unsealed_children() {
        let (_temp, mut collector) = setup();
        artifact(
            &mut collector,
            json!({"plan":{"pack":"cargo-mutants"},
            "admission":{"captured":true},"report":{"complete":true,"facts":{"caught":5}}}),
        );
        artifact(
            &mut collector,
            json!({"outcomes":[{"scenario":"Baseline",
            "log_path":"log/baseline.log","diff_path":null}],"caught":5}),
        );
        assert_eq!(collector.attachments.len(), 2);
        assert_eq!(collector.missing.len(), 1);
        assert_eq!(collector.missing[0]["role"], "mutation-native-closure");
        // The old relative path is not interpreted as authorization to read
        // an arbitrary sibling folder while exporting historical evidence.
        assert_eq!(collector.copied.len(), 2);
    }

    #[test]
    fn sealed_mutation_capture_exports_its_exact_children_without_historical_gap() {
        let (_temp, mut collector) = setup();
        let path = collector.root.join("native/mutation.log");
        fs::write(&path, b"real captured native failure bytes\n").unwrap();
        let hash = hash_file(&path).unwrap();
        artifact(
            &mut collector,
            json!({"plan":{"pack":"cargo-mutants"},
            "admission":{"captured":true},"report":{"facts":{"native_artifact_closure":{
                "schema":"thruline.cargo-mutants-artifacts.v1","complete":true,"errors":[],
                "native_artifacts":[{"path":path,"sha256":hash}]}}}}),
        );
        assert!(collector.missing.is_empty());
        assert_eq!(collector.attachments.len(), 2);
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            b"real captured native failure bytes\n"
        );
    }

    #[test]
    fn controlled_megalinter_retains_failed_bytes_and_explicit_capture_gaps() {
        let (_temp, mut collector) = setup();
        let folder = collector.root.join("native");
        let path = folder.join("command.stderr");
        let bytes = b"native command failed\n\xff";
        fs::write(&path, bytes).unwrap();
        let hash = hash_file(&path).unwrap();
        artifact(
            &mut collector,
            json!({"schema":"thruline.megalinter-capture.v2",
            "artifact_directory":folder,"complete":false,
            "artifact_closure":{"complete":false,"gaps":[{"role":"native-capture","error":"interrupted"}]},
            "native_artifacts":[{"relative_path":"command.stderr","path":path,"sha256":hash,"size":bytes.len()}]}),
        );
        assert_eq!(collector.missing.len(), 1);
        assert_eq!(collector.missing[0]["role"], "megalinter-native-closure");
        fs::write(&path, b"later mutation").unwrap();
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(&hash)).unwrap(),
            bytes
        );
    }

    #[test]
    fn outside_paths_traversal_and_bad_hashes_remain_explicit_gaps() {
        let (temp, mut collector) = setup();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"outside").unwrap();
        let native = collector.root.join("native/actual");
        fs::write(&native, b"actual").unwrap();
        artifact(
            &mut collector,
            json!({"schema":"thruline.infer-capture.v1","artifacts":[
            {"path":outside,"sha256":hash_file(&outside).unwrap()},
            {"path":native,"sha256":"0".repeat(64)}]}),
        );
        let folder = collector.root.join("native");
        artifact(
            &mut collector,
            json!({"schema":"thruline.megalinter-capture.v1","artifact_directory":folder,
            "native_artifacts":[{"path":"../native/actual","sha256":hash_file(&native).unwrap()}]}),
        );
        assert_eq!(collector.missing.len(), 3);
        assert_eq!(collector.attachments.len(), 2);
    }

    #[test]
    #[cfg(unix)]
    fn internal_and_external_symlinks_are_rejected() {
        let (temp, mut collector) = setup();
        let target = collector.root.join("native/target");
        fs::write(&target, b"actual").unwrap();
        let outside = temp.path().join("outside");
        fs::write(&outside, b"actual").unwrap();
        let internal = collector.root.join("native/internal");
        let external = collector.root.join("native/external");
        std::os::unix::fs::symlink(&target, &internal).unwrap();
        std::os::unix::fs::symlink(&outside, &external).unwrap();
        artifact(
            &mut collector,
            json!({"schema":"thruline.joern-capture.v1","artifacts":[
            {"path":internal,"sha256":hash_file(&target).unwrap()},
            {"path":external,"sha256":hash_file(&target).unwrap()}]}),
        );
        assert_eq!(collector.missing.len(), 2);
        assert!(collector
            .missing
            .iter()
            .all(|gap| gap["reason"].as_str().unwrap().contains("symlinks")));
    }

    #[test]
    fn raw_repair_logs_remain_exact_opaque_bytes() {
        let (_temp, mut collector) = setup();
        let bytes = br#"{"schema":"thruline.openrewrite-capture.v1","artifacts":[{"path":"/untrusted/original-log-path","sha256":"invalid"}]}"#;
        let hash = sha256_hex(bytes);
        let path = collector.root.join(&hash);
        fs::write(&path, bytes).unwrap();
        collector.receipt_opaque(&ArtifactDigest {
            path: path.to_string_lossy().into_owned(),
            sha256: Some(hash.clone()),
        });
        assert!(collector.missing.is_empty());
        assert_eq!(
            fs::read(collector.destination.join("attachments").join(hash)).unwrap(),
            bytes
        );
        assert_eq!(collector.attachments.len(), 1);
    }

    #[test]
    fn agent_repair_exports_explicit_refs_and_bound_diff() {
        let (_temp, mut collector) = setup();
        let path = collector.root.join("native/proposal.diff");
        let diff = "--- a/a.py\n+++ b/a.py\n@@ -1 +1 @@\n-bad\n+fixed\n";
        fs::write(&path, diff).unwrap();
        let reference = json!({"path":path,"sha256":hash_file(&path).unwrap()});
        artifact(
            &mut collector,
            json!({"schema":"thruline.agent-repair.v1",
            "diff":diff,"proposal":reference,"capture":reference,
            "native_suggestion":{"response":reference},
            "resume":{"invocation":reference,"events":reference,
                "persistence_sources":[reference],"native_artifacts":[reference]},
            "verification":{"before":{"command":{"stdout":reference,"stderr":reference}},
                "after":{"command":{"stdout":reference,"stderr":reference}}},
            "history":[{"attempt":"/untrusted/sibling"}]}),
        );
        assert!(collector.missing.is_empty(), "{:?}", collector.missing);
        for role in [
            "repair-patch",
            "source-agent-capture",
            "native-suggestion-response",
            "resume-invocation",
            "resume-events",
            "persistence_sources",
            "native_artifacts",
            "oracle-before-stdout",
            "oracle-before-stderr",
            "oracle-after-stdout",
            "oracle-after-stderr",
        ] {
            assert!(
                collector.attachments.iter().any(|a| a["role"] == role),
                "{role}"
            );
        }
        let forged =
            json!({"schema":"thruline.agent-repair.v1", "diff":"forged", "proposal":reference});
        artifact(&mut collector, forged);
        assert!(!collector.missing.is_empty());
    }

    #[test]
    fn repair_patch_is_bound_to_receipt_and_never_reads_sibling_store() {
        let (_temp, mut collector) = setup();
        let diff = "--- a/test.py\n+++ b/test.py\n@@ -1 +0,0 @@\n-import math\n";
        for schema in ["thruline.ruff-repair.v1", "thruline.native-repair.v1"] {
            artifact(
                &mut collector,
                json!({"schema":schema, "artifacts":"/untrusted/repair",
                "diff":diff,"patch_sha256":sha256_hex(diff.as_bytes())}),
            );
        }
        assert!(collector.missing.is_empty());
        let patch = collector
            .attachments
            .iter()
            .find(|a| a["role"] == "repair-patch")
            .unwrap();
        assert_eq!(
            fs::read(collector.destination.join(patch["path"].as_str().unwrap())).unwrap(),
            diff.as_bytes()
        );
    }
}
