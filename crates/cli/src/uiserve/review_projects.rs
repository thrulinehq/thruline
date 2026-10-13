//! Browser project identifiers resolve only through the operator-owned registry.
use super::*;

pub(super) fn resolve(state: &State, session: &str) -> Result<Option<(PathBuf, PathBuf)>> {
    if let Some(id) = session.strip_prefix("review-") {
        let project = crate::integration_cmd::review::registered_projects()?
            .into_iter()
            .find(|p| p.id == id)
            .context("Unknown registered review project")?;
        return Ok(Some((project.root, project.exec_db)));
    }
    Ok(state
        .verification
        .as_ref()
        .filter(|_| session == "local-verification")
        .map(|v| (v.repo.clone(), v.db.clone())))
}
pub(super) fn list(state: &State, stream: &mut TcpStream) -> Result<()> {
    let mut projects:Vec<serde_json::Value>=crate::integration_cmd::review::registered_projects()?.into_iter().map(|p|serde_json::json!({"session":format!("review-{}",p.id),"id":p.id,"label":p.label,"root":p.root,"exec_db":p.exec_db,"change_review_available":p.base_commit.is_some(),"base_commit":p.base_commit})).collect();
    if let Some(v) = &state.verification {
        if !projects
            .iter()
            .any(|p| p["root"] == serde_json::json!(v.repo))
        {
            projects.insert(0,serde_json::json!({"session":"local-verification","id":"local-verification","label":"Configured local project","root":v.repo,"exec_db":v.db,"change_review_available":false}));
        }
    }
    let mut left_out = 0u64;
    for project in &mut projects {
        let session = project["session"]
            .as_str()
            .context("review project has no session")?
            .to_owned();
        let root = project["root"]
            .as_str()
            .context("review project has no root")?
            .to_owned();
        let policy =
            match super::integration_policy_for_session(state, &session, Some(Path::new(&root))) {
                Ok(policy) => policy,
                Err(problem) if super::error_is_store_held(&problem) => {
                    return super::store_held(state, stream)
                }
                Err(_) => return super::projection_policy_unavailable(stream, &[]),
            };
        left_out += match present_project_listing(project, &policy) {
            Ok(count) => count,
            Err(_) => return super::projection_policy_unavailable(stream, &[]),
        };
    }
    let mut answer = serde_json::json!({"schema":"thruline.review-projects.v1","projects":projects,"boundary":"Only these operator-registered roots and acceptance stores can be selected. Browser requests never supply source paths or commands."});
    if left_out > 0 {
        answer[thruline_core::capture_exclusion::LEFT_OUT_FIELD] = serde_json::json!(left_out);
    }
    respond(
        stream,
        200,
        "application/json",
        &serde_json::to_vec(&answer)?,
    )
}

fn present_project_listing(
    project: &mut serde_json::Value,
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<u64> {
    crate::exclusion_serve::checked_apply(project, policy)
}
pub(super) fn download(state: &State, query: &str, stream: &mut TcpStream) -> Result<()> {
    let parts: Vec<_> = query.split('&').collect();
    anyhow::ensure!(
        parts.len() == 2,
        "One selected project and export identifier are required"
    );
    let session = parts
        .iter()
        .find_map(|p| p.strip_prefix("session="))
        .context("Export project missing")?;
    let export = parts
        .iter()
        .find_map(|p| p.strip_prefix("export="))
        .context("Export identifier missing")?;
    anyhow::ensure!(
        session
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "Invalid export project"
    );
    let (root, _) =
        resolve(state, session)?.context("Export requires a registered review project")?;
    let policy = match super::integration_policy_for_session(state, session, Some(&root)) {
        Ok(policy) => policy,
        Err(problem) if super::error_is_store_held(&problem) => {
            return super::store_held(state, stream)
        }
        Err(_) => return super::projection_policy_unavailable(stream, &[]),
    };
    let result = if policy.is_empty() {
        crate::integration_cmd::review::export_bytes(&state.db, &root, export)
            .map(|bytes| (bytes, None))
    } else {
        crate::integration_cmd::review::export_with_manifest(&state.db, &root, export)
            .map(|(bytes, manifest)| (bytes, Some(manifest)))
    };
    match result {
        Ok((bytes, manifest)) => {
            if let Some(manifest) = manifest {
                match archive_if_permitted(&manifest, &bytes, &policy) {
                    Ok(ArchiveDecision::Allowed(archive)) => {
                        return respond(stream, 200, "application/zip", archive);
                    }
                    Ok(ArchiveDecision::Excluded(left_out)) => {
                        return respond(
                            stream,
                            409,
                            "application/json",
                            &serde_json::to_vec(&serde_json::json!({
                                "error": "export_contains_excluded_path",
                                "left_out_by_exclusion": left_out,
                            }))?,
                        );
                    }
                    Ok(ArchiveDecision::OpaqueAttachments) => {
                        return respond(
                            stream,
                            409,
                            "application/json",
                            br#"{"error":"export_has_opaque_attachments","detail":"This preserved archive contains attachments whose contents cannot be checked against your exclusions."}"#,
                        );
                    }
                    Err(_) => return super::projection_policy_unavailable(stream, &[]),
                }
            }
            respond(stream, 200, "application/zip", &bytes)
        }
        Err(error) => {
            // A download link cannot render a refusal, so say it in the log and answer with an
            // explicit status; a dropped connection reads as a cancelled download.
            // The verification error can contain an attachment path, so the
            // browser gets fixed words rather than that diagnostic.
            tracing::warn!("review export download refused: {error:#}");
            respond(
                stream,
                409,
                "application/json",
                br#"{"error":"export_download_refused","detail":"The preserved evidence archive could not be verified for this download."}"#,
            )
        }
    }
}

enum ArchiveDecision<'a> {
    Allowed(&'a [u8]),
    Excluded(u64),
    OpaqueAttachments,
}

/// The ZIP is evidence with a recorded hash. A matching path in its
/// authenticated manifest refuses the whole download. A manifest with opaque
/// attachments is also refused under an active policy: its digest-only names
/// cannot prove that the contained bytes omit excluded file content. No byte
/// of the ZIP is rewritten, and a clear attachment-free archive is returned
/// as the original slice.
fn archive_if_permitted<'a>(
    manifest: &[u8],
    archive: &'a [u8],
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<ArchiveDecision<'a>> {
    if policy.is_empty() {
        return Ok(ArchiveDecision::Allowed(archive));
    }
    let doc: serde_json::Value = serde_json::from_slice(manifest)?;
    let mut texts = Vec::new();
    manifest_texts(&doc, None, &mut texts);
    let generated =
        crate::exclusion_serve::generated_for_texts(policy, texts.iter().map(|(_, text)| *text))?;
    for (name, text) in texts {
        let key = policy
            .key_for_path(text, generated.contains(text))
            .or_else(|| {
                if matches!(name, Some("command" | "argv" | "args")) {
                    policy.key_in_command_with_generated(text, |path| generated.contains(path))
                } else {
                    None
                }
            })
            .or_else(|| policy.key_in_text_with_generated(text, |path| generated.contains(path)));
        if key.is_some() {
            return Ok(ArchiveDecision::Excluded(1));
        }
    }
    let attachments = doc["attachments"]
        .as_array()
        .context("authenticated review manifest has no attachment inventory")?;
    if !attachments.is_empty() {
        return Ok(ArchiveDecision::OpaqueAttachments);
    }
    Ok(ArchiveDecision::Allowed(archive))
}

/// Scan every manifest string, including fields the JSON display walker
/// preserves as classification or identity. The archive is refused on a
/// match, so no string is changed and no recorded digest is invalidated.
fn manifest_texts<'a>(
    value: &'a serde_json::Value,
    name: Option<&'a str>,
    out: &mut Vec<(Option<&'a str>, &'a str)>,
) {
    match value {
        serde_json::Value::String(text) => out.push((name, text)),
        serde_json::Value::Array(items) => {
            for item in items {
                manifest_texts(item, name, out);
            }
        }
        serde_json::Value::Object(fields) => {
            for (name, child) in fields {
                out.push((None, name));
                manifest_texts(child, Some(name), out);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod exclusion_download_tests {
    use super::*;
    use thruline_core::capture_hold::{self, Act, Hold};
    use thruline_core::model::{ProjectRef, Source};

    #[test]
    fn review_project_list_hides_a_registered_root_inside_an_excluded_folder() {
        let policy = thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            "/repo",
            &[thruline_core::capture_hold::ExclusionView {
                key: "private".into(),
                covers: "private".into(),
                note: None,
                built_in: false,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .unwrap();
        let mut row = serde_json::json!({
            "session": "review-one",
            "root": "/repo/private",
            "exec_db": "/store/exec.db",
            "label": "Registered review"
        });
        assert_eq!(present_project_listing(&mut row, &policy).unwrap(), 1);
        assert_eq!(row["root"], "left out by your exclusion (`private`)");
        assert_eq!(row["session"], "review-one");
        assert_eq!(row["left_out_by_exclusion"], 1);
    }

    const PROJECT: &str = "review-export-synthetic";

    fn ledger(root: &str, db: &Path) -> thruline_core::ledger::Ledger {
        let ledger = thruline_core::ledger::Ledger::open(db).expect("synthetic ledger");
        ledger
            .upsert_project(
                &ProjectRef {
                    id: PROJECT.into(),
                    root_path: root.into(),
                    source: Source::Codex,
                    display_name: Some("Synthetic review".into()),
                },
                "2026-01-01T00:00:00Z",
            )
            .expect("synthetic project");
        ledger
    }

    #[test]
    fn a_preserved_review_archive_is_identical_until_its_manifest_names_an_excluded_path() {
        let temp = tempfile::tempdir().expect("scratch directory");
        let root = temp.path().join("project");
        std::fs::create_dir(&root).expect("project folder");
        let root = root.to_string_lossy().to_string();
        let ledger = ledger(&root, &temp.path().join("ledger.db"));
        let manifest = serde_json::to_vec(&serde_json::json!({
            "schema": "thruline.review-export.v1",
            "attachments": [],
            "receipts": [{"path": "private/report.rs", "sha256": "synthetic-digest"}],
        }))
        .expect("manifest");
        let archive = b"preserved synthetic ZIP bytes";
        let baseline = crate::exclusion_serve::policy(&ledger, &root).expect("baseline policy");
        let allowed =
            archive_if_permitted(&manifest, archive, &baseline).expect("baseline decision");
        assert!(matches!(allowed, ArchiveDecision::Allowed(bytes) if bytes == archive));

        capture_hold::record_for_person(
            &ledger,
            Act::Excluded,
            &Hold {
                project_id: PROJECT,
                project_folder: &root,
                actor: "synthetic person",
                recorded_at: "2026-01-02T00:00:00Z",
            },
            Some("private/report.rs"),
        )
        .expect("exclude path");
        let excluded = crate::exclusion_serve::policy(&ledger, &root).expect("excluded policy");
        assert!(matches!(
            archive_if_permitted(&manifest, archive, &excluded).expect("excluded decision"),
            ArchiveDecision::Excluded(1)
        ));
        let classification_field = br#"{"attachments":[],"source":"private/report.rs"}"#;
        assert!(matches!(
            archive_if_permitted(classification_field, archive, &excluded)
                .expect("every authenticated manifest string is checked"),
            ArchiveDecision::Excluded(1)
        ));
        let keyed_path =
            br#"{"attachments":[],"files":{"private/report.rs":{"sha256":"synthetic-digest"}}}"#;
        assert!(matches!(
            archive_if_permitted(keyed_path, archive, &excluded)
                .expect("manifest object keys are checked"),
            ArchiveDecision::Excluded(1)
        ));

        capture_hold::record_for_person(
            &ledger,
            Act::Included,
            &Hold {
                project_id: PROJECT,
                project_folder: &root,
                actor: "synthetic person",
                recorded_at: "2026-01-03T00:00:00Z",
            },
            Some("private/report.rs"),
        )
        .expect("include path");
        let restored = crate::exclusion_serve::policy(&ledger, &root).expect("restored policy");
        let allowed =
            archive_if_permitted(&manifest, archive, &restored).expect("restored decision");
        assert!(matches!(allowed, ArchiveDecision::Allowed(bytes) if bytes == archive));
        assert_eq!(archive, b"preserved synthetic ZIP bytes");
    }

    #[test]
    fn a_pathless_opaque_attachment_blocks_a_review_archive_under_an_active_policy() {
        let temp = tempfile::tempdir().expect("scratch directory");
        let root = temp.path().join("project");
        std::fs::create_dir(&root).expect("project folder");
        let root = root.to_string_lossy().to_string();
        let ledger = ledger(&root, &temp.path().join("ledger.db"));
        capture_hold::record_for_person(
            &ledger,
            Act::Excluded,
            &Hold {
                project_id: PROJECT,
                project_folder: &root,
                actor: "synthetic person",
                recorded_at: "2026-01-02T00:00:00Z",
            },
            Some("private/report.rs"),
        )
        .expect("exclude path");
        let policy = crate::exclusion_serve::policy(&ledger, &root).expect("active policy");
        let manifest = br#"{"attachments":[{"path":"attachments/012345","role":"native-stdout","source":"/managed/evidence/012345"}]}"#;
        assert!(matches!(
            archive_if_permitted(manifest, b"opaque report bytes", &policy)
                .expect("conservative archive decision"),
            ArchiveDecision::OpaqueAttachments
        ));
        let clear = br#"{"attachments":[],"receipts":[{"path":"public/report.rs"}]}"#;
        assert!(matches!(
            archive_if_permitted(clear, b"preserved bytes", &policy)
                .expect("clear attachment-free archive"),
            ArchiveDecision::Allowed(bytes) if bytes == b"preserved bytes"
        ));
    }

    #[test]
    fn a_generated_rule_refuses_the_archive_when_git_attributes_cannot_be_read() {
        let temp = tempfile::tempdir().expect("scratch directory");
        let root = temp.path().join("not-a-git-repository");
        std::fs::create_dir(&root).expect("project folder");
        let root = root.to_string_lossy().to_string();
        let ledger = ledger(&root, &temp.path().join("ledger.db"));
        capture_hold::record_for_person(
            &ledger,
            Act::Excluded,
            &Hold {
                project_id: PROJECT,
                project_folder: &root,
                actor: "synthetic person",
                recorded_at: "2026-01-02T00:00:00Z",
            },
            Some("generated"),
        )
        .expect("exclude generated");
        let policy = crate::exclusion_serve::policy(&ledger, &root).expect("generated policy");
        let manifest = br#"{"attachments":[{"path":"src/generated.rs"}]}"#;
        assert!(archive_if_permitted(manifest, b"archive", &policy).is_err());
    }
}
