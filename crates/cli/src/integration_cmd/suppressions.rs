//! Captured-source completion audit. No subprocess and no alternate evidence
//! store. The same inventory digest is part of before/after/current Inputs.
use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Serialize)]
pub(super) struct Source {
    sha256: String,
    unavailable: Option<String>,
}

#[derive(Clone)]
pub(super) struct Snapshot {
    pub identity: String,
    files: BTreeMap<String, Source>,
    problem: Option<String>,
    disabled: bool,
}

impl Snapshot {
    pub fn capture(plan: &Plan) -> Result<Self> {
        let disabled = plan.pack == "semgrep" && engine::semgrep_suppression_disabled(&plan.argv);
        let mut files = BTreeMap::new();
        let problem = if ["ast-grep", "semgrep"].contains(&plan.pack.as_str()) && !disabled {
            collect(plan, &mut files).err().map(|e| e.to_string())
        } else {
            None
        };
        let identity = hash_json(&json!({"schema":"thruline.source-suppression-audit.v1",
            "files":files, "problem":problem, "suppression_disabled":disabled}))?;
        Ok(Self {
            identity,
            files,
            problem,
            disabled,
        })
    }
}

#[cfg(unix)]
fn collect(plan: &Plan, files: &mut BTreeMap<String, Source>) -> Result<()> {
    use rustix::fs::{open, openat, Mode, OFlags};
    use sha2::{Digest, Sha256};
    use std::io::Read;
    use std::path::Component;
    let root_fd = open(
        &plan.root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let mut count = 0usize;
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(&plan.root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| e.depth() == 0 || e.file_name() != ".git")
    {
        let entry = entry?;
        if entry.depth() == 0 || entry.file_type().is_dir() {
            continue;
        }
        count += 1;
        anyhow::ensure!(
            count <= 100_000,
            "source inventory exceeds the 100,000-file input limit"
        );
        let relative = entry.path().strip_prefix(&plan.root)?;
        let key = relative
            .to_str()
            .context("source inventory path is not UTF-8")?
            .to_owned();
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            let kind = if entry.file_type().is_symlink() {
                "symlink"
            } else {
                "non-regular file"
            };
            files.insert(
                key,
                Source {
                    sha256: hash_json(&(kind, fs::read_link(entry.path()).ok()))?,
                    unavailable: Some(format!("Selected source is a {kind}; it was not followed.")),
                },
            );
            continue;
        }
        // Open every component relative to an already-open project directory,
        // refusing symlinks at each hop. A canonicalize-then-open check alone
        // would permit a concurrent replacement to escape the source boundary.
        let parts: Vec<_> = relative.components().collect();
        let mut parent = rustix::io::dup(&root_fd)?;
        for part in &parts[..parts.len() - 1] {
            let Component::Normal(name) = part else {
                bail!("non-relative source path")
            };
            parent = openat(
                &parent,
                *name,
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
                Mode::empty(),
            )?;
        }
        let Some(Component::Normal(name)) = parts.last() else {
            bail!("invalid source filename")
        };
        let fd = openat(
            &parent,
            *name,
            OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
            Mode::empty(),
        )?;
        let mut file = fs::File::from(fd);
        anyhow::ensure!(
            file.metadata()?.is_file(),
            "source changed to a non-regular file"
        );
        let mut hasher = Sha256::new();
        let mut read = 0u64;
        let mut buffer = [0u8; 65536];
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            read += n as u64;
            total += n as u64;
            anyhow::ensure!(
                read <= 1024 * 1024 * 1024,
                "bound source exceeds the 1 GiB input limit"
            );
            anyhow::ensure!(
                total <= 2 * 1024 * 1024 * 1024,
                "source inventory exceeds the 2 GiB input limit"
            );
            hasher.update(&buffer[..n]);
        }
        let sha256 = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        files.insert(
            key,
            Source {
                sha256,
                unavailable: None,
            },
        );
    }
    Ok(())
}

#[cfg(not(unix))]
fn collect(_plan: &Plan, _files: &mut BTreeMap<String, Source>) -> Result<()> {
    bail!("This host has no qualified directory-relative source reader")
}

fn relative(plan: &Plan, path: &str) -> Option<String> {
    use std::path::Component;
    let path = Path::new(path);
    let path = if path.is_absolute() {
        if plan.container.is_some() {
            path.strip_prefix("/input").ok()?
        } else {
            path.strip_prefix(&plan.root).ok()?
        }
    } else {
        path
    };
    let mut normalized = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) => normalized.push(name),
            Component::CurDir => {}
            _ => return None,
        }
    }
    if normalized.as_os_str().is_empty() {
        return None;
    }
    normalized.to_str().map(String::from)
}

fn explicit_ast_files(plan: &Plan, report: &Report, source: &Snapshot) -> Option<BTreeSet<String>> {
    if plan.argv.get(1).map(String::as_str) != Some("scan") {
        return None;
    }
    let mut keys = BTreeSet::new();
    let mut i = 2;
    let mut positional_only = false;
    while i < plan.argv.len() {
        let arg = &plan.argv[i];
        if !positional_only {
            match arg.as_str() {
                "--" => {
                    positional_only = true;
                    i += 1;
                    continue;
                }
                "--config" | "-c" | "--rule" | "-r" | "--filter" => {
                    i += 2;
                    if i > plan.argv.len() {
                        return None;
                    }
                    continue;
                }
                "--json"
                | "--json=compact"
                | "--json=pretty"
                | "--inspect=summary"
                | "--hint=unused-suppression"
                | "--no-ignore" => {
                    i += 1;
                    continue;
                }
                _ if arg.starts_with('-') => return None,
                _ => {}
            }
        }
        let path = relative(plan, arg)?;
        // A directory, unbound file, symlink or repeated selection cannot
        // establish this legacy file-only producer contract.
        if source.files.get(&path)?.unavailable.is_some() || !keys.insert(path) {
            return None;
        }
        i += 1;
    }
    let scanned = report
        .facts
        .get("scan_coverage")?
        .get("scanned_files")?
        .as_u64()?;
    if keys.is_empty() || keys.len() as u64 != scanned {
        return None;
    }
    Some(keys)
}

#[cfg(unix)]
fn selected_bytes(plan: &Plan, path: &str, remaining: usize) -> Result<Vec<u8>> {
    use rustix::fs::{open, openat, Mode, OFlags};
    use std::{io::Read, path::Component};
    let mut parent = open(
        &plan.root,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
        Mode::empty(),
    )?;
    let parts: Vec<_> = Path::new(path).components().collect();
    anyhow::ensure!(!parts.is_empty(), "empty source path");
    for part in &parts[..parts.len() - 1] {
        let Component::Normal(name) = part else {
            bail!("non-relative source path")
        };
        parent = openat(
            &parent,
            *name,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW,
            Mode::empty(),
        )?;
    }
    let Some(Component::Normal(name)) = parts.last() else {
        bail!("invalid source filename")
    };
    let fd = openat(
        &parent,
        *name,
        OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK,
        Mode::empty(),
    )?;
    let file = fs::File::from(fd);
    let metadata = file.metadata()?;
    anyhow::ensure!(metadata.is_file(), "selected source is not a regular file");
    let ceiling = remaining.min(8 * 1024 * 1024);
    anyhow::ensure!(metadata.len() <= ceiling as u64,
        "selected source exceeds the audit completeness ceiling (8 MiB/file or 64 MiB total); scanner budgets are unchanged");
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(ceiling as u64 + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() <= ceiling,
        "selected source grew past its audit completeness ceiling"
    );
    Ok(bytes)
}

#[cfg(not(unix))]
fn selected_bytes(_plan: &Plan, _path: &str, _remaining: usize) -> Result<Vec<u8>> {
    bail!("This host has no qualified directory-relative source reader")
}

fn limit(report: &mut Report, reason: &str) {
    report.complete = false;
    if !report.limitations.iter().any(|s| s == reason) {
        report.limitations.insert(0, reason.into());
    }
}

pub(super) fn apply(plan: &Plan, report: &mut Report, source: &Snapshot) {
    if semgrep_mode::is_plan(plan) {
        if let Err(error) = semgrep_mode::annotate_preparation(plan, report) {
            limit(
                report,
                &format!("Bounded source-snapshot preparation evidence is unavailable: {error}"),
            );
        }
        // Native phase commands and source paths have their own strict decoder.
        // The complete source audit stays in before/after/current identity.
        if let Some(problem) = &source.problem {
            limit(report, problem);
        }
        if plan.argv.len() == 7
            && report
                .facts
                .get("semgrep_mode")
                .and_then(|v| v.pointer("/git_identity/accepted_object_closure"))
                .and_then(Value::as_str)
                != Some(plan.argv[6].as_str())
        {
            limit(
                report,
                "The captured ancestor object closure differs from the accepted plan.",
            );
        }
        if let Some(paths) = report.facts.get("scanned_paths").and_then(Value::as_array) {
            let paths = paths.clone();
            for name in paths {
                if !name
                    .as_str()
                    .is_some_and(|p| source.files.get(p).is_some_and(|f| f.unavailable.is_none()))
                {
                    limit(
                        report,
                        "A native mode source path has no complete bound pre-run source bytes.",
                    );
                }
            }
        }
        report.facts.insert("source_suppression_audit".into(),json!({"basis":"explicit trusted mode protocol and native assertion/target inventory","identity":source.identity,"source_problem":source.problem}));
        return;
    }
    if !["ast-grep", "semgrep"].contains(&plan.pack.as_str()) {
        return;
    }
    let paths = if plan.pack == "semgrep" {
        report
            .facts
            .get("scanned_paths")
            .and_then(Value::as_array)
            .cloned()
    } else {
        report
            .facts
            .get("scan_entities")
            .and_then(|v| v.get("files"))
            .and_then(Value::as_array)
            .cloned()
    };
    // A summary-only legacy scan is usable only for unambiguous, explicit
    // regular-file targets whose count agrees with the producer. Directory
    // selection requires entity output; never audit a whole-project substitute.
    let basis = if paths.is_some() {
        "captured scanned paths"
    } else {
        "explicit legacy file selection and matching scan count"
    };
    let keys: BTreeSet<String> = match paths {
        Some(paths) => paths.iter().filter_map(|v| {
            let result = v.as_str().and_then(|p| relative(plan, p));
            if result.is_none() { limit(report, "Captured source path is invalid or outside the project; it was not opened."); }
            result
        }).collect(),
        None if plan.pack == "ast-grep" => match explicit_ast_files(plan, report, source) {
            Some(keys) => keys,
            None => { limit(report, "Legacy structural output has no exact scanned-file inventory. Review a capture with --inspect=entity; whole-project substitution is not used."); BTreeSet::new() }
        },
        None => { limit(report, "Semgrep did not retain a valid scanned-source inventory."); BTreeSet::new() },
    };
    if keys.is_empty() {
        limit(
            report,
            "No identified source bytes were available for the suppression audit.",
        );
    }
    // Disabling suppression changes filtering, not whether the scanner examined
    // any accepted source. Validate its inventory before skipping directive work.
    if source.disabled {
        report.facts.insert(
            "source_suppression_audit".into(),
            json!({
            "basis":"captured reviewed --disable-nosem option; findings remain visible",
            "identity":source.identity,"suppression_disabled":true,
            "scanned_paths":keys}),
        );
        return;
    }
    let selected_rules: Option<BTreeSet<String>> = report
        .facts
        .get("scan_entities")
        .and_then(|v| v.get("rules"))
        .and_then(Value::as_array)
        .map(|ids| {
            ids.iter()
                .filter_map(Value::as_str)
                .map(String::from)
                .collect()
        });
    let unused = report
        .facts
        .get("unused_suppression_ranges")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut evidence = Vec::new();
    let mut retained_bytes = 0usize;
    let mut retained_directives = 0usize;
    if keys.len() > 4096 {
        limit(report, "Selected source audit exceeds its 4096-file completeness ceiling; the scanner budget is unchanged.");
    }
    for path in keys.into_iter().take(4096) {
        let Some(file) = source.files.get(&path) else {
            limit(report, "A captured source path has no bound pre-run bytes (missing, excluded metadata, or symlink traversal); suppression completeness is unavailable.");
            continue;
        };
        if let Some(reason) = &file.unavailable {
            limit(report, &format!("{path}: {reason}"));
            evidence.push(json!({"path":path,"sha256":file.sha256,"unavailable":reason}));
            continue;
        }
        let audit = match selected_bytes(plan, &path, 64 * 1024 * 1024 - retained_bytes) {
            Ok(bytes) => {
                retained_bytes += bytes.len();
                if sha256_hex(&bytes) != file.sha256 {
                    limit(report, "Selected source changed after its pre-run identity; suppression audit cannot describe the captured execution.");
                    evidence.push(json!({"path":path,"before_sha256":file.sha256,"audit_sha256":sha256_hex(&bytes),"unavailable":"source changed"}));
                    continue;
                }
                engine::source_suppressions(&plan.format, &path, &bytes)
                // Source bytes are dropped here, before the next file opens.
            }
            Err(error) => {
                let reason = error.to_string();
                limit(report, &format!("{path}: {reason}"));
                evidence.push(json!({"path":path,"sha256":file.sha256,"unavailable":reason}));
                continue;
            }
        };
        if let Some(reason) = &audit.unavailable {
            limit(report, &format!("{path}: {reason}"));
        }
        let mut directives = Vec::new();
        let directive_budget = 4096 - retained_directives;
        if audit.directives.len() > directive_budget {
            limit(report, "Selected source contains more than 4096 directive records; the bounded audit is incomplete.");
        }
        for directive in audit.directives.iter().take(directive_budget) {
            retained_directives += 1;
            let unused_by_tool = unused.iter().any(|v| {
                v.get("file")
                    .and_then(Value::as_str)
                    .and_then(|p| relative(plan, p))
                    .as_deref()
                    == Some(path.as_str())
                    && v.get("start").and_then(Value::as_u64) == Some(directive.byte_start as u64)
                    && v.get("end").and_then(Value::as_u64) == Some(directive.byte_end as u64)
            });
            let unmatched = directive.rules.as_ref().is_some_and(|ids| {
                ids.iter().all(|id| id.is_empty())
                    || selected_rules
                        .as_ref()
                        .is_some_and(|selected| ids.iter().all(|id| !selected.contains(id)))
            });
            let resolution = if unused_by_tool {
                "native unused-suppression observation at the exact byte range"
            } else if unmatched {
                "named directive does not select any captured effective rule"
            } else {
                limit(report, "Source contains a suppression directive/filter whose effect is absent from the retained result. Full selected-rule coverage is incomplete; no hidden finding is inferred.");
                "omission effect unavailable; incomplete coverage"
            };
            directives.push(json!({"directive":directive,"resolution":resolution}));
        }
        evidence.push(json!({"path":path,"sha256":file.sha256,"directives":directives,"unavailable":audit.unavailable}));
    }
    report.facts.insert("source_suppression_audit".into(), json!({
        "schema":"thruline.source-suppression-audit.v1","identity":source.identity,
        "basis":basis,"sources":evidence,"audited_bytes":retained_bytes,"inventory_problem":source.problem}));
}

#[cfg(test)]
mod tests {
    use super::*;
    fn plan(root: &Path) -> Plan {
        serde_json::from_value(json!({"schema":"thruline.integration-plan.v1","pack":"ast-grep","format":"ast-grep-json-v1","root":root,"argv":["ast-grep","scan","--inspect=entity","."],"criterion":{"id":"audit","revision":"1","requirement":"bounded structural scan","success":"no observations","accepted_by":"test","condition":{"kind":"no_observations"}},"rules":[],"runtime":[],"dependencies":[],"dataset":[],"environment":{},"input_closure_complete":true,"timeout_seconds":5,"output_limit_bytes":8192,"cpu_seconds":5,"memory_mb":256,"report":{"kind":"stdout"}})).unwrap()
    }
    fn report(files: &[&str], rules: &[&str]) -> Report {
        let mut r = engine::decode("ast-grep-json-v1", b"[]").unwrap();
        r.facts
            .insert("scan_entities".into(), json!({"files":files,"rules":rules}));
        r
    }
    #[test]
    fn active_named_string_unmatched_and_unselected_source_controls() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("active.ts"),
            "foo(1); // ast-grep-ignore: selected\n",
        )
        .unwrap();
        fs::write(
            dir.path().join("string.ts"),
            "const s = 'ast-grep-ignore'; // prose ast-grep-ignore\n",
        )
        .unwrap();
        let p = plan(dir.path());
        let snapshot = Snapshot::capture(&p).unwrap();
        let mut active = report(&["active.ts"], &["selected"]);
        apply(&p, &mut active, &snapshot);
        assert!(!active.complete);
        assert!(active.observations.is_empty());
        let mut unmatched = report(&["active.ts"], &["other"]);
        apply(&p, &mut unmatched, &snapshot);
        assert!(unmatched.complete);
        let mut clean = report(&["string.ts"], &["selected"]);
        apply(&p, &mut clean, &snapshot);
        assert!(clean.complete); // active.ts is explicitly not scanned
    }
    #[test]
    fn inventory_binds_ignored_source_changes_and_refuses_symlink_escapes() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".gitignore"), "ignored.ts\n").unwrap();
        fs::write(dir.path().join("ignored.ts"), "foo(1);\n").unwrap();
        let p = plan(dir.path());
        let first = Snapshot::capture(&p).unwrap();
        fs::write(dir.path().join("ignored.ts"), "foo(2);\n").unwrap();
        assert_ne!(first.identity, Snapshot::capture(&p).unwrap().identity);
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            fs::write(outside.path().join("outside.ts"), "foo(1);\n").unwrap();
            std::os::unix::fs::symlink(outside.path(), dir.path().join("escape")).unwrap();
            let snapshot = Snapshot::capture(&p).unwrap();
            assert!(!snapshot.files.contains_key("escape/outside.ts"));
            let mut r = report(&["escape/outside.ts"], &["selected"]);
            apply(&p, &mut r, &snapshot);
            assert!(!r.complete);
        }
        assert!(relative(&p, "../outside.ts").is_none());
        assert!(relative(&p, "/private/outside.ts").is_none());
    }

    #[test]
    fn unused_evidence_requires_the_exact_comment_range_and_changed_source_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        let source = "foo(1); // ast-grep-ignore\n";
        fs::write(dir.path().join("source.ts"), source).unwrap();
        let p = plan(dir.path());
        let snapshot = Snapshot::capture(&p).unwrap();
        let parsed = engine::source_suppressions(&p.format, "source.ts", source.as_bytes());
        let comment = &parsed.directives[0];
        let mut unused = report(&["source.ts"], &["selected"]);
        unused.facts.insert(
            "unused_suppression_ranges".into(),
            json!([{"file":"source.ts","start":comment.byte_start,"end":comment.byte_end}]),
        );
        // This tests the evidence join only; actual upstream unused findings are
        // separately retained and continue to fail a no-observations criterion.
        apply(&p, &mut unused, &snapshot);
        assert!(unused.complete);
        let mut wrong = report(&["source.ts"], &["selected"]);
        wrong.facts.insert(
            "unused_suppression_ranges".into(),
            json!([{"file":"source.ts","start":comment.byte_start + 1,"end":comment.byte_end}]),
        );
        apply(&p, &mut wrong, &snapshot);
        assert!(!wrong.complete);
        fs::write(dir.path().join("source.ts"), "foo(2); // ast-grep-ignore\n").unwrap();
        let mut changed = report(&["source.ts"], &["other"]);
        apply(&p, &mut changed, &snapshot);
        assert!(!changed.complete);
        assert!(changed.limitations.iter().any(|s| s.contains("changed")));
    }

    #[test]
    fn legacy_file_selection_is_exact_and_directory_selection_needs_entities() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.ts"), "foo(1);\n").unwrap();
        let mut p = plan(dir.path());
        p.argv = [
            "ast-grep",
            "scan",
            "--inspect=summary",
            "--config",
            "sg.yml",
            "source.ts",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let snapshot = Snapshot::capture(&p).unwrap();
        let mut r = engine::decode("ast-grep-json-v1", b"[]").unwrap();
        engine::apply_capture_diagnostics(&mut r, b"sg: summary|file: scannedFileCount=1,skippedFileCount=0\nsg: summary|rule: effectiveRuleCount=1,skippedRuleCount=0\n");
        apply(&p, &mut r, &snapshot);
        assert!(r.complete);
        *p.argv.last_mut().unwrap() = ".".into();
        apply(&p, &mut r, &snapshot);
        assert!(!r.complete);
    }

    #[test]
    fn legacy_semgrep_and_disable_nosem_keep_distinct_meaning_and_safe_mapping() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("source.py"),
            "value = eval(user_text) # nosemgrep: selected\n",
        )
        .unwrap();
        let mut p = plan(dir.path());
        p.pack = "semgrep".into();
        p.format = "semgrep-json-v1".into();
        p.argv = ["semgrep", "scan", "--json", "/input/source.py"]
            .into_iter()
            .map(String::from)
            .collect();
        p.container = Some(container::Spec {
            docker: "/unused/docker".into(),
            socket: "/unused/socket".into(),
            image: format!("sha256:{}", "a".repeat(64)),
            environment: BTreeMap::new(),
        });
        let raw = br#"{"version":"1.176.0","results":[],"errors":[],"paths":{"scanned":["/input/source.py"]}}"#;
        let mut legacy = engine::decode(&p.format, raw).unwrap();
        apply(&p, &mut legacy, &Snapshot::capture(&p).unwrap());
        assert!(!legacy.complete);
        assert!(legacy.observations.is_empty());
        p.argv.insert(3, "--disable-nosem".into());
        let snapshot = Snapshot::capture(&p).unwrap();
        assert!(snapshot.files.is_empty()); // no source buffer or inventory needed
        let mut disabled = engine::decode(&p.format, raw).unwrap();
        apply(&p, &mut disabled, &snapshot);
        assert!(disabled.complete); // actual finding behavior is an upstream test
        assert_eq!(
            relative(&p, "/input/source.py").as_deref(),
            Some("source.py")
        );
        assert!(relative(&p, "/input/../outside.py").is_none());
        assert!(relative(&p, "/etc/passwd").is_none());
    }

    #[test]
    fn disabled_semgrep_still_requires_valid_nonempty_scanned_scope() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.py"), "value = 1\n").unwrap();
        let mut p = plan(dir.path());
        p.pack = "semgrep".into();
        p.format = "semgrep-json-v1".into();
        p.argv = ["semgrep", "scan", "--json", "--disable-nosem", "."]
            .into_iter()
            .map(String::from)
            .collect();
        let snapshot = Snapshot::capture(&p).unwrap();
        for paths in [
            json!([]),
            json!(["../outside.py"]),
            json!(["source.py", "/etc/passwd"]),
        ] {
            let raw = serde_json::to_vec(
                &json!({"version":"1.176.0", "results":[], "errors":[], "paths":{"scanned":paths}}),
            )
            .unwrap();
            let mut r = engine::decode(&p.format, &raw).unwrap();
            engine::apply_capture_diagnostics(&mut r, b"Nothing to scan.\n");
            apply(&p, &mut r, &snapshot);
            assert!(
                !r.complete,
                "invalid or empty selection must not establish a clean result"
            );
            assert!(r.observations.is_empty());
        }
        let mut r = engine::decode(
            &p.format,
            br#"{"version":"1.176.0","results":[],"errors":[],"paths":{"scanned":["source.py"]}}"#,
        )
        .unwrap();
        apply(&p, &mut r, &snapshot);
        assert!(r.complete);
    }

    #[test]
    fn selected_read_budget_is_checked_before_materializing_the_file() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("source.ts"), "let a = 1;\n").unwrap();
        let p = plan(dir.path());
        assert!(selected_bytes(&p, "source.ts", 4)
            .unwrap_err()
            .to_string()
            .contains("ceiling"));
        let large = fs::File::create(dir.path().join("large.ts")).unwrap();
        large.set_len(8 * 1024 * 1024 + 1).unwrap();
        assert!(selected_bytes(&p, "large.ts", 64 * 1024 * 1024)
            .unwrap_err()
            .to_string()
            .contains("ceiling"));
    }
}
