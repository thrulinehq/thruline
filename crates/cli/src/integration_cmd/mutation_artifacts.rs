//! Seal the native log/diff closure named by cargo-mutants outcomes.json.
//! Paths in native output authorize reads only below that accepted run folder.
use super::*;
use std::{collections::BTreeSet, io::Read, path::Component};

const FILE_LIMIT: u64 = 8 * 1024 * 1024;
const TOTAL_LIMIT: u64 = 32 * 1024 * 1024;
const REFERENCES: usize = 4096;

#[derive(Default)]
pub(super) struct Sealed {
    pub artifacts: Vec<ArtifactDigest>,
    pub manifest: Value,
    pub errors: Vec<String>,
}

fn relative(path: &Path) -> Result<()> {
    anyhow::ensure!(
        !path.as_os_str().is_empty()
            && path
                .components()
                .all(|part| matches!(part, Component::Normal(_))),
        "Native mutation artifact path is not a contained relative file"
    );
    Ok(())
}

fn read_below(root: &Path, path: &Path, limit: u64) -> Result<Vec<u8>> {
    relative(path)?;
    #[cfg(unix)]
    let file = {
        use rustix::fs::{open, openat, Mode, OFlags};
        let flags = OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC;
        let mut directory = open(root, flags | OFlags::DIRECTORY, Mode::empty())?;
        let components: Vec<_> = path.components().collect();
        for component in &components[..components.len() - 1] {
            directory = openat(
                &directory,
                component.as_os_str(),
                flags | OFlags::DIRECTORY,
                Mode::empty(),
            )?;
        }
        fs::File::from(openat(
            &directory,
            components.last().unwrap().as_os_str(),
            flags,
            Mode::empty(),
        )?)
    };
    #[cfg(not(unix))]
    let file = {
        let mut target = root.to_owned();
        for component in path.components() {
            target.push(component);
            anyhow::ensure!(
                !fs::symlink_metadata(&target)?.file_type().is_symlink(),
                "Mutation artifact symlink rejected"
            );
        }
        fs::File::open(target)?
    };
    let metadata = file.metadata()?;
    anyhow::ensure!(
        metadata.is_file() && metadata.len() <= limit,
        "Native mutation artifact is not a bounded regular file"
    );
    let mut bytes = Vec::new();
    file.take(limit + 1).read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= limit,
        "Native mutation artifact exceeds retention budget"
    );
    Ok(bytes)
}

pub(super) fn seal(db: &Path, report_path: &Path, raw: &[u8], selected_scope: bool) -> Sealed {
    let mut sealed = Sealed::default();
    let mut rows = vec![];
    let attempt = (|| -> Result<()> {
        let root = artifact_dir(db).canonicalize()?;
        let report_relative = report_path
            .strip_prefix(&root)
            .context("Mutation report is outside its profile evidence store")?;
        relative(report_relative)?;
        let parts: Vec<_> = report_relative.components().collect();
        anyhow::ensure!(
            parts.len() == 4
                && parts[0].as_os_str() == "cargo-mutants"
                && parts[1]
                    .as_os_str()
                    .to_string_lossy()
                    .starts_with("run-output-")
                && parts[2].as_os_str() == "mutants.out"
                && parts[3].as_os_str() == "outcomes.json",
            "Mutation report does not belong to an accepted managed run folder"
        );
        // Reopening the accepted report through directory descriptors checks
        // the same path boundary as every child and detects changed outcomes.
        anyhow::ensure!(
            read_below(&root, report_relative, FILE_LIMIT)? == raw,
            "Native mutation report changed while sealing its attachments"
        );
        let value: Value = engine::parse_json(raw)?;
        let outcomes = value["outcomes"]
            .as_array()
            .context("Native mutation outcomes array missing")?;
        anyhow::ensure!(
            outcomes.len() <= REFERENCES / 2,
            "Native mutation outcomes exceed attachment count budget"
        );
        let folder = report_relative
            .parent()
            .context("Native mutation folder missing")?;
        let mut total = 0;
        let mut seen = BTreeSet::new();
        for (index, outcome) in outcomes.iter().enumerate() {
            for key in ["log_path", "diff_path"] {
                let item = &outcome[key];
                if key == "diff_path" && item.is_null() && outcome["scenario"] == "Baseline" {
                    continue;
                }
                let read = (|| -> Result<()> {
                    let path = Path::new(
                        item.as_str()
                            .context("Native mutation log/diff path missing")?,
                    );
                    relative(path)?;
                    let bytes = read_below(
                        &root,
                        &folder.join(path),
                        FILE_LIMIT.min(TOTAL_LIMIT - total),
                    )?;
                    total += bytes.len() as u64;
                    let digest = sha256_hex(&bytes);
                    let target = private_write(&root, &bytes)?;
                    rows.push(
                        json!({"outcome_index":index,"role":key,"source_reference":path,
                        "path":target,"sha256":digest,"bytes":bytes.len()}),
                    );
                    if seen.insert(digest.clone()) {
                        sealed.artifacts.push(ArtifactDigest {
                            path: target.display().to_string(),
                            sha256: Some(digest),
                        });
                    }
                    Ok(())
                })();
                if let Err(error) = read {
                    sealed
                        .errors
                        .push(format!("Mutation outcome {index} {key}: {error}"));
                }
            }
        }
        if selected_scope {
            for (name, role) in [
                ("debug.log", "native_debug"),
                ("mutants.json", "mutation_inventory"),
            ] {
                let read = (|| -> Result<()> {
                    let bytes = read_below(
                        &root,
                        &folder.join(name),
                        FILE_LIMIT.min(TOTAL_LIMIT - total),
                    )?;
                    total += bytes.len() as u64;
                    let digest = sha256_hex(&bytes);
                    let target = private_write(&root, &bytes)?;
                    rows.push(json!({"role":role,"source_reference":name,"path":target,"sha256":digest,"bytes":bytes.len()}));
                    if seen.insert(digest.clone()) {
                        sealed.artifacts.push(ArtifactDigest {
                            path: target.display().to_string(),
                            sha256: Some(digest),
                        });
                    }
                    Ok(())
                })();
                if let Err(error) = read {
                    sealed
                        .errors
                        .push(format!("Mutation scope artifact {name}: {error}"));
                }
            }
        }
        Ok(())
    })();
    if let Err(error) = attempt {
        sealed.errors.push(error.to_string());
    }
    sealed.manifest = json!({"schema":"thruline.cargo-mutants-artifacts.v1",
        "raw_report_sha256":sha256_hex(raw),"native_artifacts":rows,
        "complete":sealed.errors.is_empty(),"errors":sealed.errors,
        "limits":{"file_bytes":FILE_LIMIT,"total_bytes":TOTAL_LIMIT,"references":REFERENCES},
        "provenance":"Hashes bind captured native bytes; upstream outcomes do not supply child digests."});
    sealed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf, Vec<u8>) {
        let temp = tempfile::tempdir().unwrap();
        let db = temp.path().canonicalize().unwrap().join("ledger.db");
        let folder = artifact_dir(&db).join("cargo-mutants/run-output-test/mutants.out");
        fs::create_dir_all(folder.join("log")).unwrap();
        fs::create_dir_all(folder.join("diff")).unwrap();
        fs::write(folder.join("log/baseline.log"), b"native baseline output").unwrap();
        fs::write(
            folder.join("log/mutant.log"),
            b"native failed assertion output",
        )
        .unwrap();
        fs::write(
            folder.join("diff/mutant.diff"),
            b"- return a > 0\n+ return true\n",
        )
        .unwrap();
        let bytes = serde_json::to_vec(&json!({"outcomes":[
            {"scenario":"Baseline","log_path":"log/baseline.log","diff_path":null},
            {"scenario":{"Mutant":{}},"log_path":"log/mutant.log","diff_path":"diff/mutant.diff"}]})).unwrap();
        let report = folder.join("outcomes.json");
        fs::write(&report, &bytes).unwrap();
        (temp, db, report, bytes)
    }

    #[test]
    fn seals_complete_closure_without_rewriting_native_report() {
        let (_temp, db, path, bytes) = setup();
        let sealed = seal(&db, &path, &bytes, false);
        assert!(sealed.errors.is_empty(), "{:?}", sealed.errors);
        assert_eq!(sealed.artifacts.len(), 3);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
        for artifact in sealed.artifacts {
            assert_eq!(
                sha256_hex(&fs::read(artifact.path).unwrap()),
                artifact.sha256.unwrap()
            );
        }
    }

    #[test]
    fn missing_child_retains_other_bytes_and_explicit_gap() {
        let (_temp, db, path, bytes) = setup();
        fs::remove_file(path.parent().unwrap().join("diff/mutant.diff")).unwrap();
        let sealed = seal(&db, &path, &bytes, false);
        assert_eq!(sealed.artifacts.len(), 2);
        assert_eq!(sealed.errors.len(), 1);
        assert_eq!(sealed.manifest["complete"], false);
    }

    #[test]
    fn changed_report_and_foreign_profile_are_rejected() {
        let (_temp, db, path, bytes) = setup();
        let mut changed = bytes.clone();
        changed.push(b'\n');
        assert!(!seal(&db, &path, &changed, false).errors.is_empty());
        assert!(!seal(&db.with_file_name("other.db"), &path, &bytes, false)
            .errors
            .is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn traversal_and_symlink_never_read_outside_bytes() {
        let (_temp, db, path, bytes) = setup();
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["outcomes"][0]["log_path"] = json!("../../outside");
        let raw = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &raw).unwrap();
        let folder = path.parent().unwrap();
        fs::remove_dir_all(folder.join("log")).unwrap();
        std::os::unix::fs::symlink(folder.join("diff"), folder.join("log")).unwrap();
        let sealed = seal(&db, &path, &raw, false);
        assert_eq!(sealed.errors.len(), 2);
        assert_eq!(sealed.artifacts.len(), 1);
        assert_eq!(sealed.manifest["native_artifacts"][0]["role"], "diff_path");
    }
    #[test]
    fn selected_scope_requires_and_seals_native_debug_and_inventory() {
        let (_temp, db, path, bytes) = setup();
        let incomplete = seal(&db, &path, &bytes, true);
        assert_eq!(incomplete.errors.len(), 2);
        assert_eq!(incomplete.artifacts.len(), 3);
        fs::write(
            path.parent().unwrap().join("debug.log"),
            b"native two worker trace",
        )
        .unwrap();
        fs::write(path.parent().unwrap().join("mutants.json"), b"[]").unwrap();
        let complete = seal(&db, &path, &bytes, true);
        assert!(complete.errors.is_empty());
        assert_eq!(complete.artifacts.len(), 5);
        assert!(complete.manifest["native_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["role"] == "native_debug"));
        assert!(complete.manifest["native_artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["role"] == "mutation_inventory"));
        fs::remove_dir_all(path.parent().unwrap()).unwrap();
        for artifact in complete.artifacts {
            assert_eq!(
                sha256_hex(&fs::read(artifact.path).unwrap()),
                artifact.sha256.unwrap()
            );
        }
    }
}
