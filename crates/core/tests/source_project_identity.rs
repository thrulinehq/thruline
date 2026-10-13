//! Source-project identity is independent of presentation roots and is scoped
//! by the physical source owner. These tests stay on the public ledger API so
//! they exercise the same transaction boundary as production acceptance.

use rusqlite::{Connection, OptionalExtension};
use thruline_core::chain::sha256_hex;
use thruline_core::derive::derive_project;
use thruline_core::export::export_project;
use thruline_core::ledger::{
    AcceptedGeneration, GenerationAcceptance, Ledger, SourceProjectIdentity, SourceSessionIdentity,
};
use thruline_core::{EventKind, NormalizedEvent, ProjectRef, RawLineRef, SessionRef, Source};

const ROOT: &str = "/work/shared";
const NOW: &str = "2026-08-01T00:00:00Z";

fn initial_acceptance(
    project_id: &str,
    session_id: &str,
    owner_key: &str,
    source: Source,
    source_project_key: &str,
    source_session_key: &str,
) -> GenerationAcceptance {
    let payload_sha256 =
        sha256_hex(format!("{owner_key}:{source_project_key}:{source_session_key}").as_bytes());
    let generation_id = format!("generation-{project_id}-{session_id}");

    GenerationAcceptance {
        project: ProjectRef {
            id: project_id.to_string(),
            root_path: ROOT.to_string(),
            source,
            display_name: Some("shared".to_string()),
        },
        project_identity: SourceProjectIdentity {
            project_id: project_id.to_string(),
            owner_key: owner_key.to_string(),
            source: source.as_str().to_string(),
            source_project_key: source_project_key.to_string(),
            normalized_root_hint: Some(ROOT.to_string()),
        },
        session: SessionRef {
            id: session_id.to_string(),
            project_id: project_id.to_string(),
            file_path: format!("/source/{owner_key}/{source_session_key}.jsonl"),
            started_at: Some(NOW.to_string()),
            raw_line_count: 0,
            raw_digest: Some(payload_sha256.clone()),
        },
        session_identity: SourceSessionIdentity {
            session_id: session_id.to_string(),
            project_id: project_id.to_string(),
            owner_key: owner_key.to_string(),
            source: source.as_str().to_string(),
            source_session_key: source_session_key.to_string(),
            source_locator: Some(format!("/source/{owner_key}/{source_session_key}.jsonl")),
        },
        generation: AcceptedGeneration {
            id: generation_id.clone(),
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            predecessor_id: None,
            mirror_payload_path: format!("generations/{generation_id}/payload"),
            mirror_seal_path: format!("generations/{generation_id}/seal"),
            payload_sha256,
            payload_bytes: 0,
            seal_sha256: sha256_hex(generation_id.as_bytes()),
            inherited_prefix_bytes: 0,
            total_envelopes: 0,
            local_envelopes: 0,
            local_events: 0,
            accepted_at: NOW.to_string(),
        },
        expected_predecessor: None,
        dispositions: Vec::new(),
        events: Vec::new(),
    }
}

fn accept(ledger: &mut Ledger, acceptance: &GenerationAcceptance) {
    let placed = ledger
        .accept_source_generation(acceptance)
        .expect("source generation accepts");
    assert!(placed.is_empty(), "identity fixture carries no events");
}

#[test]
fn legacy_unique_root_migration_preserves_project_session_and_chain_identity() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("legacy-root-unique.db");
    let project_id = "-Users-legacy-shared";
    let session_id = "legacy-session";
    let event_id: String;
    let chain_before;
    let state_before;
    let meta_before: (String, i64, Option<String>);
    let export_before: Vec<u8>;

    {
        let mut ledger = Ledger::open(&path).expect("create current-shaped fixture");
        ledger
            .upsert_project(
                &ProjectRef {
                    id: project_id.to_string(),
                    root_path: ROOT.to_string(),
                    source: Source::ClaudeCode,
                    display_name: Some("legacy shared".to_string()),
                },
                NOW,
            )
            .expect("legacy project");
        ledger
            .upsert_session(&SessionRef {
                id: session_id.to_string(),
                project_id: project_id.to_string(),
                file_path: "/legacy/session.jsonl".to_string(),
                started_at: Some(NOW.to_string()),
                raw_line_count: 1,
                raw_digest: Some("legacy-raw-digest".to_string()),
            })
            .expect("legacy session");

        let mut normalized = NormalizedEvent::new(EventKind::AssistantText);
        normalized.role = Some("assistant".to_string());
        normalized.text = Some("legacy authority survives".to_string());
        event_id = ledger
            .append_session_events(
                project_id,
                session_id,
                &[(
                    RawLineRef {
                        file: "/legacy/session.jsonl".to_string(),
                        line: 1,
                        sha256: "a".repeat(64),
                    },
                    normalized,
                )],
            )
            .expect("legacy event")
            .remove(0)
            .id;
        ledger
            .update_ledger_meta(project_id, Some(NOW))
            .expect("legacy head metadata");

        chain_before = ledger.session_chain(session_id).expect("legacy chain");
        state_before = ledger.session_state(session_id).expect("legacy state");
        meta_before = ledger
            .conn()
            .query_row(
                "SELECT head_hash, event_count, verified_at FROM ledger_meta WHERE project_id=?1",
                [project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("legacy metadata snapshot");
        export_before = serde_json::to_vec(
            &export_project(
                &ledger,
                &derive_project(&ledger, project_id).expect("legacy derived graph"),
                NOW,
            )
            .expect("legacy public export"),
        )
        .expect("serialize legacy export");
    }

    // Recreate the pre-M4.R2 projects shape. All dependent rows remain in
    // place so Ledger::open must perform its real forward-only rebuild.
    {
        let conn = Connection::open(&path).expect("open fixture for legacy shaping");
        conn.pragma_update(None, "foreign_keys", "OFF")
            .expect("disable foreign keys for schema shaping");
        conn.execute_batch(
            "BEGIN IMMEDIATE;
             CREATE TABLE projects_legacy(
               id TEXT PRIMARY KEY,
               root_path TEXT NOT NULL UNIQUE,
               source TEXT NOT NULL CHECK(source IN ('claude_code','codex')),
               display_name TEXT,
               created_at TEXT NOT NULL);
             INSERT INTO projects_legacy(id, root_path, source, display_name, created_at)
               SELECT id, root_path, source, display_name, created_at FROM projects;
             DROP TABLE projects;
             ALTER TABLE projects_legacy RENAME TO projects;
             COMMIT;",
        )
        .expect("restore legacy unique-root shape");
    }

    let ledger = Ledger::open(&path).expect("migrate legacy root uniqueness");
    assert_eq!(ledger.project_ids().expect("projects"), vec![project_id]);
    assert_eq!(
        ledger.sessions_of(project_id).expect("sessions"),
        vec![(session_id.to_string(), "/legacy/session.jsonl".to_string())]
    );
    assert_eq!(
        ledger.session_chain(session_id).expect("chain"),
        chain_before
    );
    assert_eq!(
        ledger.session_state(session_id).expect("state"),
        state_before
    );
    assert_eq!(
        ledger
            .conn()
            .query_row(
                "SELECT id FROM events WHERE session_id=?1",
                [session_id],
                |row| row.get::<_, String>(0),
            )
            .expect("event id after migration"),
        event_id,
        "legacy event identity must not be re-keyed"
    );
    assert_eq!(
        ledger
            .conn()
            .query_row(
                "SELECT head_hash, event_count, verified_at FROM ledger_meta WHERE project_id=?1",
                [project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("metadata after migration"),
        meta_before,
        "accepted and verified authority must survive the presentation-schema rebuild"
    );
    assert_eq!(
        ledger
            .source_project_id("legacy-local", "claude_code", project_id)
            .expect("legacy project mapping"),
        Some(project_id.to_string())
    );
    assert_eq!(
        ledger
            .source_session_id("legacy-local", "claude_code", session_id)
            .expect("legacy session mapping"),
        Some(session_id.to_string())
    );
    assert_eq!(
        ledger
            .conn()
            .query_row("PRAGMA foreign_key_check", [], |_| Ok(1_i64))
            .optional()
            .expect("foreign key check"),
        None,
        "migration must leave no foreign-key violations"
    );
    let export_after = serde_json::to_vec(
        &export_project(
            &ledger,
            &derive_project(&ledger, project_id).expect("migrated derived graph"),
            NOW,
        )
        .expect("migrated public export"),
    )
    .expect("serialize migrated export");
    assert_eq!(
        export_after, export_before,
        "a legacy Claude database with no Codex data exports byte-identically after migration"
    );
}

#[test]
fn same_root_claude_and_codex_projects_coexist_without_merging() {
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    let claude = initial_acceptance(
        "project-claude",
        "session-claude",
        "owner-a",
        Source::ClaudeCode,
        "native-project",
        "native-session",
    );
    let codex = initial_acceptance(
        "project-codex",
        "session-codex",
        "owner-a",
        Source::Codex,
        "native-project",
        "native-session",
    );

    accept(&mut ledger, &claude);
    accept(&mut ledger, &codex);

    assert_eq!(
        ledger.project_ids().expect("project ids"),
        vec!["project-claude".to_string(), "project-codex".to_string()]
    );
    assert_eq!(
        ledger
            .source_project_id("owner-a", "claude_code", "native-project")
            .expect("Claude identity"),
        Some("project-claude".to_string())
    );
    assert_eq!(
        ledger
            .source_project_id("owner-a", "codex", "native-project")
            .expect("Codex identity"),
        Some("project-codex".to_string())
    );
    assert_eq!(
        ledger
            .source_session_id("owner-a", "claude_code", "native-session")
            .expect("Claude session identity"),
        Some("session-claude".to_string())
    );
    assert_eq!(
        ledger
            .source_session_id("owner-a", "codex", "native-session")
            .expect("Codex session identity"),
        Some("session-codex".to_string())
    );
    assert_eq!(
        ledger
            .sessions_of("project-claude")
            .expect("Claude sessions"),
        vec![(
            "session-claude".to_string(),
            "/source/owner-a/native-session.jsonl".to_string()
        )]
    );
    assert_eq!(
        ledger.sessions_of("project-codex").expect("Codex sessions"),
        vec![(
            "session-codex".to_string(),
            "/source/owner-a/native-session.jsonl".to_string()
        )]
    );

    let rows: Vec<(String, String, String)> = {
        let mut statement = ledger
            .conn()
            .prepare("SELECT id, root_path, source FROM projects ORDER BY id")
            .expect("project query");
        statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .expect("project rows")
            .collect::<Result<_, _>>()
            .expect("project row values")
    };
    assert_eq!(
        rows,
        vec![
            (
                "project-claude".to_string(),
                ROOT.to_string(),
                "claude_code".to_string()
            ),
            (
                "project-codex".to_string(),
                ROOT.to_string(),
                "codex".to_string()
            )
        ],
        "the root is only a hint; each source retains truthful provenance"
    );
}

#[test]
fn identical_native_keys_are_isolated_by_owner_and_session_ids_cannot_rebind() {
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    let owner_a = initial_acceptance(
        "project-owner-a",
        "session-owner-a",
        "owner-a",
        Source::Codex,
        "same-native-project",
        "same-native-session",
    );
    let owner_b = initial_acceptance(
        "project-owner-b",
        "session-owner-b",
        "owner-b",
        Source::Codex,
        "same-native-project",
        "same-native-session",
    );
    accept(&mut ledger, &owner_a);
    accept(&mut ledger, &owner_b);

    assert_eq!(
        ledger
            .source_project_id("owner-a", "codex", "same-native-project")
            .expect("owner A project"),
        Some("project-owner-a".to_string())
    );
    assert_eq!(
        ledger
            .source_project_id("owner-b", "codex", "same-native-project")
            .expect("owner B project"),
        Some("project-owner-b".to_string())
    );
    assert_eq!(
        ledger
            .source_session_id("owner-a", "codex", "same-native-session")
            .expect("owner A session"),
        Some("session-owner-a".to_string())
    );
    assert_eq!(
        ledger
            .source_session_id("owner-b", "codex", "same-native-session")
            .expect("owner B session"),
        Some("session-owner-b".to_string())
    );
    assert_ne!(
        ledger
            .accepted_generation("session-owner-a")
            .expect("owner A generation")
            .expect("owner A head")
            .id,
        ledger
            .accepted_generation("session-owner-b")
            .expect("owner B generation")
            .expect("owner B head")
            .id
    );

    let rebind = ledger.upsert_session(&SessionRef {
        id: "session-owner-a".to_string(),
        project_id: "project-owner-b".to_string(),
        file_path: "/source/owner-b/rebound.jsonl".to_string(),
        started_at: None,
        raw_line_count: 0,
        raw_digest: None,
    });
    let error = rebind.expect_err("a ledger session id must never move between projects");
    assert!(
        error.to_string().contains("different project"),
        "unexpected rejection: {error}"
    );
    assert_eq!(
        ledger
            .sessions_of("project-owner-a")
            .expect("owner A sessions"),
        vec![(
            "session-owner-a".to_string(),
            "/source/owner-a/same-native-session.jsonl".to_string()
        )]
    );
}

#[test]
fn generation_acceptance_rejects_a_session_claimed_by_another_owner() {
    let mut ledger = Ledger::open_in_memory().expect("ledger");
    let mut mismatched = initial_acceptance(
        "project-owner-a",
        "session-owner-b",
        "owner-a",
        Source::Codex,
        "native-project",
        "native-session",
    );
    mismatched.session_identity.owner_key = "owner-b".to_string();
    mismatched.session_identity.source_locator =
        Some("/source/owner-b/native-session.jsonl".to_string());

    let error = ledger
        .accept_source_generation(&mismatched)
        .expect_err("a source session cannot cross its source-project owner boundary");
    assert!(
        error.to_string().contains("identity fields disagree"),
        "unexpected rejection: {error}"
    );
    assert!(
        ledger
            .project_ids()
            .expect("projects after rejection")
            .is_empty(),
        "identity rejection must roll back the entire acceptance transaction"
    );
}
