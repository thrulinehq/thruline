//! C46 Settings permission rows: synthetic folders and a scratch ledger only.

use super::*;
use thruline_core::run_permission as rp;

const MEMBER_A: &str = "cccccccc-0000-4000-8000-00000000aa01";
const MEMBER_B: &str = "cccccccc-0000-4000-8000-00000000bb01";
const CHANGE_A: &str = "cccccccc-0000-4000-8000-00000000aa02";
const CHANGE_B: &str = "cccccccc-0000-4000-8000-00000000bb02";
const CHANGE_SAME: &str = "cccccccc-0000-4000-8000-00000000bb03";
const FOREIGN_MEMBER: &str = "cccccccc-0000-4000-8000-00000000cc01";
const FOREIGN_CHANGE: &str = "cccccccc-0000-4000-8000-00000000cc02";

fn fixture() -> (TempDir, Arc<State>, String, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().expect("scratch");
    let root = temp.path().join("repo");
    let other = root.join("private");
    std::fs::create_dir_all(&other).expect("worktree folders");
    assert!(std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&root)
        .status()
        .expect("git init")
        .success());
    // The Change binder records the Git worktree's root. Give the second
    // synthetic folder its own repository so its recorded root stays distinct
    // from the enclosing HQ project instead of collapsing to the outer root.
    assert!(std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&other)
        .status()
        .expect("git init second worktree")
        .success());
    let root = root.canonicalize().expect("canonical root");
    let other = other.canonicalize().expect("canonical other");
    let ledger =
        thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("scratch ledger");
    for member in [MEMBER_A, MEMBER_B] {
        ledger.conn().execute(
            "INSERT INTO projects(id,root_path,source,display_name,created_at) VALUES(?1,?2,'codex','Synthetic','2026-01-01T00:00:00Z')",
            rusqlite::params![member, root.display().to_string()],
        ).expect("ledger member");
    }
    for (id, member, folder) in [
        (CHANGE_A, MEMBER_A, &root),
        (CHANGE_SAME, MEMBER_B, &root),
        (CHANGE_B, MEMBER_B, &other),
    ] {
        ledger.conn().execute(
            "INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source) VALUES(?1,?2,'2026-01-02T00:00:00Z','Synthetic','Test','sam','person')",
            rusqlite::params![id, member],
        ).expect("Change");
        thruline_core::examined_inputs::bind_intended_at(
            &ledger,
            id,
            "2026-01-02T00:01:00Z",
            Some(folder),
        )
        .expect("bind root")
        .expect("binding");
    }
    drop(ledger);
    let hq = thruline_core::hq::project_id_for_root(&root.display().to_string());
    let state = test_state(temp.path(), MockEngine::available());
    (temp, state, hq, root, other)
}

// Every response in this suite crosses this boundary, including malformed
// requests sent directly instead of through `get` or `post`.
fn checked_exchange(state: Arc<State>, request: &[u8]) -> String {
    type ScratchFiles = BTreeMap<PathBuf, (&'static str, Vec<u8>)>;
    type Snapshot = (Vec<Vec<rusqlite::types::Value>>, ScratchFiles);

    fn files(dir: &Path, db: &Path, out: &mut ScratchFiles) {
        for entry in std::fs::read_dir(dir).expect("scratch contents") {
            let path = entry.expect("scratch entry").path();
            // SQLite may maintain its own read sidecars. The permission rows
            // are compared separately; every other name and byte is pinned.
            if path == db
                || path == db.with_extension("db-wal")
                || path == db.with_extension("db-shm")
            {
                continue;
            }
            let meta = std::fs::symlink_metadata(&path).expect("scratch metadata");
            if meta.is_symlink() {
                out.insert(
                    path.clone(),
                    (
                        "link",
                        std::fs::read_link(&path)
                            .expect("link target")
                            .as_os_str()
                            .as_encoded_bytes()
                            .to_vec(),
                    ),
                );
            } else if meta.is_dir() {
                out.insert(path.clone(), ("directory", Vec::new()));
                files(&path, db, out);
            } else {
                out.insert(
                    path.clone(),
                    ("file", std::fs::read(&path).expect("scratch file")),
                );
            }
        }
    }
    fn snapshot(state: &State) -> Snapshot {
        let conn = rusqlite::Connection::open_with_flags(
            &state.db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .expect("scratch ledger");
        let mut stmt = conn
            .prepare("SELECT * FROM run_permissions ORDER BY seq")
            .expect("permission rows");
        let columns = stmt.column_count();
        let rows = stmt
            .query_map([], |row| {
                (0..columns)
                    .map(|column| row.get(column))
                    .collect::<rusqlite::Result<Vec<_>>>()
            })
            .expect("permission rows")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("rows");
        let mut tree = BTreeMap::new();
        files(
            state.db.parent().expect("scratch root"),
            &state.db,
            &mut tree,
        );
        (rows, tree)
    }
    fn no_absolute_path(value: &serde_json::Value) {
        match value {
            serde_json::Value::String(text) => {
                for part in text.split(|c: char| {
                    c.is_whitespace() || matches!(c, '"' | '\'' | '`' | '(' | ')' | '[' | ']')
                }) {
                    assert!(
                        !Path::new(part).is_absolute(),
                        "absolute path in response: {text}"
                    );
                    let bytes = part.as_bytes();
                    assert!(
                        !(bytes.len() >= 3
                            && bytes[0].is_ascii_alphabetic()
                            && bytes[1] == b':'
                            && matches!(bytes[2], b'/' | b'\\')),
                        "absolute path in response: {text}"
                    );
                }
            }
            serde_json::Value::Array(items) => items.iter().for_each(no_absolute_path),
            serde_json::Value::Object(fields) => fields.values().for_each(no_absolute_path),
            _ => {}
        }
    }
    let before = snapshot(&state);
    let answer = exchange(Arc::clone(&state), request);
    let raw = answer.split_once("\r\n\r\n").expect("HTTP response body").1;
    let body: serde_json::Value =
        serde_json::from_str(raw).unwrap_or_else(|_| serde_json::Value::String(raw.to_string()));
    no_absolute_path(&body);
    let text = body.to_string();
    assert!(
        !text.contains("$HOME"),
        "home placeholder in response: {text}"
    );
    if let Some(home) = std::env::var_os("HOME") {
        assert!(
            !text.contains(home.to_string_lossy().as_ref()),
            "home path in response: {text}"
        );
    }
    assert!(
        !text.contains(
            state
                .db
                .parent()
                .expect("scratch root")
                .to_string_lossy()
                .as_ref()
        ),
        "scratch/HQ root in response: {text}"
    );
    if request.starts_with(b"GET ") || status(&answer) >= 400 {
        assert_eq!(
            snapshot(&state),
            before,
            "GET or refusal changed a permission row, cache, directory or seal key: {answer}"
        );
    }
    answer
}

fn get(state: &Arc<State>, path: &str) -> String {
    checked_exchange(
        Arc::clone(state),
        format!("GET {path} HTTP/1.1\r\nHost: {ORIGIN}\r\n\r\n").as_bytes(),
    )
}

fn post(state: &Arc<State>, path: &str, body: &serde_json::Value, guarded: bool) -> String {
    let headers = if guarded {
        format!(
            "Origin: http://{ORIGIN}\r\n{}: {}\r\n",
            run_route::TOKEN_HEADER,
            state.run_route.token().expect("run token"),
        )
    } else {
        String::new()
    };
    checked_exchange(
        Arc::clone(state),
        &framed("POST", path, body.to_string().as_bytes(), &headers),
    )
}

fn permissions(state: &Arc<State>, hq: &str) -> serde_json::Value {
    let answer = get(state, &format!("/api/hq/projects/{hq}/run-permissions"));
    assert_eq!(status(&answer), 200, "{answer}");
    response_json(&answer)
}

fn add_foreign(state: &Arc<State>, temp: &TempDir) -> (String, PathBuf) {
    let root = temp.path().join("foreign");
    std::fs::create_dir_all(&root).expect("foreign folder");
    assert!(std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&root)
        .status()
        .expect("git init")
        .success());
    let root = root.canonicalize().expect("foreign root");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    ledger.conn().execute(
        "INSERT INTO projects(id,root_path,source,display_name,created_at) VALUES(?1,?2,'codex','Other','2026-01-01T00:00:00Z')",
        rusqlite::params![FOREIGN_MEMBER, root.display().to_string()],
    ).expect("foreign project");
    ledger.conn().execute(
        "INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source) VALUES(?1,?2,'2026-01-02T00:00:00Z','Foreign','Test','sam','person')",
        rusqlite::params![FOREIGN_CHANGE, FOREIGN_MEMBER],
    ).expect("foreign Change");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        FOREIGN_CHANGE,
        "2026-01-02T00:01:00Z",
        Some(&root),
    )
    .expect("foreign bind")
    .expect("foreign binding");
    (
        thruline_core::hq::project_id_for_root(&root.display().to_string()),
        root,
    )
}

#[test]
fn settings_permission_rows_group_exact_folders_across_ledger_members() {
    let (_tmp, state, hq, root, other) = fixture();
    let doc = permissions(&state, &hq);
    assert_eq!(doc["project"].as_str(), Some(hq.as_str()));
    assert_eq!(doc["schema_version"], 1);
    let rows = doc["folders"].as_array().expect("folder rows");
    assert_eq!(
        rows.len(),
        2,
        "two folders, not three Changes or two member ids: {doc}"
    );
    assert_eq!(rows[0]["change_id"], CHANGE_A);
    assert_eq!(rows[1]["change_id"], CHANGE_B);
    assert!(rows
        .iter()
        .all(|r| r["state"] == "never" && r["action"].is_null()));
    let a = rows
        .iter()
        .find(|r| r["change_id"] == CHANGE_A)
        .expect("shared folder");
    let b = rows
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("second folder");
    assert_ne!(a["id"], b["id"]);
    assert_eq!(a["latest_row"], serde_json::Value::Null);
    assert_eq!(a["folder"], "Project folder");
    assert_eq!(b["folder"], "private");
    assert!(!doc.to_string().contains(&root.display().to_string()));
    assert!(!doc.to_string().contains(&other.display().to_string()));
}

#[test]
fn opaque_folder_ids_survive_a_new_launch_without_path_text() {
    let (tmp, state, hq, _root, _other) = fixture();
    let first = permissions(&state, &hq);
    let old_token = state.run_route.token().expect("first token").to_string();
    drop(state);
    let relaunched = test_state(tmp.path(), MockEngine::available());
    assert_ne!(relaunched.run_route.token(), Some(old_token.as_str()));
    let second = permissions(&relaunched, &hq);
    assert_eq!(
        first, second,
        "a new launch must retain the same folder identities"
    );
    for row in second["folders"].as_array().expect("folders") {
        let id = row["id"].as_str().expect("opaque id");
        assert_eq!(id.len(), 64);
        assert!(id.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(!id.contains("repo") && !id.contains("private"));
    }
}

#[test]
fn shared_folder_permission_is_global_across_the_hq_members() {
    let (_tmp, state, hq, root, _other) = fixture();
    let initial = permissions(&state, &hq);
    let shared = initial["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_A)
        .expect("shared folder");
    let id = shared["id"].as_str().expect("id").to_string();
    let allowed = post(
        &state,
        &format!("/api/changes/{CHANGE_SAME}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&allowed), 200, "{allowed}");
    let given = permissions(&state, &hq);
    let shared = given["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("shared folder");
    assert_eq!(shared["change_id"], CHANGE_A);
    assert_eq!(shared["state"], "given");
    assert_eq!(shared["action"], "revoke");
    let latest = shared["latest_row"].as_str().expect("grant row");
    let revoked = post(
        &state,
        &format!("/api/changes/{CHANGE_A}/run/permission/revoke"),
        &serde_json::json!({"expected_folder":id,"expected_row":latest}),
        true,
    );
    assert_eq!(status(&revoked), 200, "{revoked}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    let history = rp::history(&ledger, &root.display().to_string()).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].project_id, MEMBER_B);
    assert_eq!(history[1].project_id, MEMBER_A);
    assert_eq!(history[1].revokes.as_deref(), Some(history[0].id.as_str()));
}

#[test]
fn permission_folder_labels_are_distinct_for_external_worktrees_and_hide_an_exclusion() {
    let (_tmp, state, hq, root, other) = fixture();
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::capture_hold::record_for_person(
        &ledger,
        thruline_core::capture_hold::Act::Excluded,
        &thruline_core::capture_hold::Hold {
            // Holds are scoped by the exact HQ root folder; a hold recorded
            // through member A also filters member B's Change under it.
            project_id: MEMBER_A,
            project_folder: root.to_str().expect("utf8 root"),
            actor: "synthetic person",
            recorded_at: "2026-01-03T00:00:00Z",
        },
        Some("private"),
    )
    .expect("exclude private");
    for (id, side) in [
        ("cccccccc-0000-4000-8000-00000000dd01", "outside-a"),
        ("cccccccc-0000-4000-8000-00000000dd02", "outside-b"),
    ] {
        let external = _tmp.path().join(side).join("same-leaf");
        std::fs::create_dir_all(&external).expect("external folder");
        let external = external.canonicalize().expect("external root");
        ledger.conn().execute(
            "INSERT INTO changes(id,project_id,opened_at,title,purpose,requester,request_source) VALUES(?1,?2,'2026-01-02T00:00:00Z','External','Test','sam','person')",
            rusqlite::params![id, MEMBER_A],
        ).expect("external Change");
        thruline_core::examined_inputs::bind_intended_at(
            &ledger,
            id,
            "2026-01-02T00:01:00Z",
            Some(&external),
        )
        .expect("external bind")
        .expect("binding");
    }
    drop(ledger);
    let doc = permissions(&state, &hq);
    let rows = doc["folders"].as_array().expect("folders");
    let hidden = rows
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("private");
    assert_eq!(hidden["folder"], "left out by your exclusion (`private`)");
    assert!(!doc.to_string().contains(&other.display().to_string()));
    let external: Vec<_> = rows
        .iter()
        .filter(|r| {
            r["change_id"] == "cccccccc-0000-4000-8000-00000000dd01"
                || r["change_id"] == "cccccccc-0000-4000-8000-00000000dd02"
        })
        .collect();
    assert_eq!(external.len(), 2);
    assert_ne!(external[0]["folder"], external[1]["folder"]);
    for row in external {
        let label = row["folder"].as_str().expect("label");
        assert!(label.starts_with("Other worktree · "), "{label}");
        assert!(!label.contains("same-leaf"));
        assert!(!label.starts_with('/'));
    }
    assert!(!doc.to_string().contains(&_tmp.path().display().to_string()));
}

#[test]
fn settings_permission_write_refuses_a_stale_folder_and_orphan_revoke_is_row_bound() {
    let (_tmp, state, hq, _root, other) = fixture();
    let before = permissions(&state, &hq);
    let rows = before["folders"].as_array().expect("folders");
    let a = rows.iter().find(|r| r["change_id"] == CHANGE_A).expect("A");
    let b = rows.iter().find(|r| r["change_id"] == CHANGE_B).expect("B");
    let cache_dir_before = other.join(".thruline").exists();
    let key_path = state
        .db
        .parent()
        .expect("store directory")
        .join("run-permission.key");
    let key_before = std::fs::read(&key_path).ok();
    let wrong = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":a["id"],"expected_row":null}),
        true,
    );
    assert_eq!(status(&wrong), 409, "{wrong}");
    assert_eq!(response_json(&wrong)["error"], "permission_target_changed");
    assert_eq!(
        response_json(&wrong)["plain"],
        "This permission changed in the record. Read it again."
    );
    assert!(!wrong.contains(&other.display().to_string()));
    assert_eq!(other.join(".thruline").exists(), cache_dir_before);
    assert_eq!(std::fs::read(&key_path).ok(), key_before);
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("history")
            .len(),
        0
    );
    drop(ledger);

    let allowed = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":b["id"],"expected_row":null}),
        true,
    );
    assert_eq!(status(&allowed), 200, "{allowed}");
    let given = permissions(&state, &hq);
    let row = given["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == b["id"])
        .expect("B row");
    assert_eq!(row["state"], "given");
    assert_eq!(row["action"], "revoke");
    assert!(row["actor"].as_str().is_some_and(|s| !s.is_empty()));
    assert!(row["at"].as_str().is_some_and(|s| !s.is_empty()));
    let shown_grant = row["latest_row"].as_str().expect("grant row").to_string();
    let cache_before = std::fs::read(other.join(run_route::PERMISSION_FILE)).expect("grant cache");
    let key_before_revoke = std::fs::read(&key_path).expect("seal key");

    let stale_row = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission/revoke"),
        &serde_json::json!({"expected_folder":b["id"],"expected_row":"older-row"}),
        true,
    );
    assert_eq!(status(&stale_row), 409, "{stale_row}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("history")
            .len(),
        1
    );
    assert_eq!(
        std::fs::read(other.join(run_route::PERMISSION_FILE)).expect("unchanged cache"),
        cache_before
    );
    assert_eq!(
        std::fs::read(&key_path).expect("unchanged key"),
        key_before_revoke
    );
    drop(ledger);

    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&_root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    let moved = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission/revoke"),
        &serde_json::json!({"expected_folder":b["id"],"expected_row":shown_grant}),
        true,
    );
    assert_eq!(status(&moved), 409, "{moved}");
    let orphan = permissions(&state, &hq);
    let old = orphan["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == b["id"])
        .expect("historical B");
    assert_eq!(old["change_id"], serde_json::Value::Null);
    assert_eq!(old["historical"], true);
    assert_eq!(old["action"], "revoke");

    let refused = post(
        &state,
        &format!(
            "/api/hq/projects/{hq}/run-permissions/{}/revoke",
            b["id"].as_str().expect("id")
        ),
        &serde_json::json!({"expected_row":"stale"}),
        true,
    );
    assert_eq!(status(&refused), 409, "{refused}");
    let revoked = post(
        &state,
        &format!(
            "/api/hq/projects/{hq}/run-permissions/{}/revoke",
            b["id"].as_str().expect("id")
        ),
        &serde_json::json!({"expected_row":shown_grant}),
        true,
    );
    assert_eq!(status(&revoked), 200, "{revoked}");
    let after = permissions(&state, &hq);
    let old = after["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == b["id"])
        .expect("historical B");
    assert_eq!(old["state"], "revoked");
    assert!(old["action"].is_null());
    assert!(old["actor"].is_null());
    assert!(old["at"].is_null());
}

#[test]
fn a_partial_settings_guard_cannot_fall_back_to_the_check_sheet_body() {
    let (_tmp, state, _hq, _root, other) = fixture();
    let grant_path = format!("/api/changes/{CHANGE_B}/run/permission");
    for body in [
        serde_json::json!({"expected_row":"older-row"}),
        serde_json::json!({"expected_folder":null,"expected_row":"older-row"}),
        serde_json::json!({"expected_folder":"bad","expected_row":null}),
    ] {
        let answer = post(&state, &grant_path, &body, true);
        assert_eq!(status(&answer), 400, "{answer}");
    }
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert!(rp::history(&ledger, &other.display().to_string())
        .expect("history")
        .is_empty());
    assert!(!other.join(run_route::PERMISSION_FILE).exists());
}

#[test]
fn historical_revoke_guards_refuse_foreign_and_unknown_targets_without_writing() {
    let (tmp, state, hq, root, other) = fixture();
    let local_before = permissions(&state, &hq);
    let local_id = local_before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let local_grant = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":local_id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&local_grant), 200, "{local_grant}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    let local = permissions(&state, &hq);
    let local_row = local["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == local_id)
        .expect("orphan grant");
    assert_eq!(local_row["historical"], true);
    let local_latest = local_row["latest_row"]
        .as_str()
        .expect("local grant")
        .to_string();
    let valid_path = format!("/api/hq/projects/{hq}/run-permissions/{local_id}/revoke");
    let local_cache = std::fs::read(other.join(run_route::PERMISSION_FILE)).expect("local cache");
    let (foreign_hq, foreign_root) = add_foreign(&state, &tmp);
    let given = post(
        &state,
        &format!("/api/changes/{FOREIGN_CHANGE}/run/permission"),
        &serde_json::json!({}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    let foreign = permissions(&state, &foreign_hq);
    let foreign_id = foreign["folders"][0]["id"].as_str().expect("foreign id");
    let foreign_row = foreign["folders"][0]["latest_row"]
        .as_str()
        .expect("foreign row");
    let bounded = permissions(&state, &hq);
    assert_eq!(
        bounded["folders"].as_array().expect("local folders").len(),
        2
    );
    assert!(!bounded["folders"]
        .as_array()
        .expect("local folders")
        .iter()
        .any(|r| r["id"] == foreign_id));
    let foreign_cache =
        std::fs::read(foreign_root.join(run_route::PERMISSION_FILE)).expect("cache");
    let foreign_path = format!("/api/hq/projects/{hq}/run-permissions/{foreign_id}/revoke");
    let unknown = "0000000000000000000000000000000000000000000000000000000000000000";
    let unknown_hq = &unknown[..16];
    let unknown_change = post(
        &state,
        &format!("/api/changes/{unknown}/run/permission"),
        &serde_json::json!({"expected_folder":foreign_id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&unknown_change), 404, "{unknown_change}");
    let read_query = get(
        &state,
        &format!("/api/hq/projects/{hq}/run-permissions?ignored=1"),
    );
    assert_eq!(status(&read_query), 400, "{read_query}");
    let read_method = post(
        &state,
        &format!("/api/hq/projects/{hq}/run-permissions"),
        &serde_json::json!({}),
        true,
    );
    assert_eq!(status(&read_method), 405, "{read_method}");
    let attempts = [
        (
            "cross HQ",
            foreign_path,
            serde_json::json!({"expected_row":foreign_row}),
            true,
            404,
        ),
        (
            "unknown folder",
            format!("/api/hq/projects/{hq}/run-permissions/{unknown}/revoke"),
            serde_json::json!({"expected_row":foreign_row}),
            true,
            404,
        ),
        (
            "unknown HQ",
            format!("/api/hq/projects/{unknown_hq}/run-permissions/{foreign_id}/revoke"),
            serde_json::json!({"expected_row":foreign_row}),
            true,
            404,
        ),
        (
            "query",
            format!("{valid_path}?ignored=1"),
            serde_json::json!({"expected_row":local_latest}),
            true,
            400,
        ),
        (
            "no origin and token",
            valid_path.clone(),
            serde_json::json!({"expected_row":local_latest}),
            false,
            403,
        ),
        (
            "malformed body",
            valid_path.clone(),
            serde_json::json!({"folder":local_id}),
            true,
            400,
        ),
    ];
    for (name, path, body, guarded, expected) in attempts {
        let answer = post(&state, &path, &body, guarded);
        assert_eq!(status(&answer), expected, "{name}: {answer}");
        assert!(
            !answer.contains(&other.display().to_string())
                && !answer.contains(&foreign_root.display().to_string()),
            "{name}: {answer}"
        );
    }
    let no_token = checked_exchange(
        Arc::clone(&state),
        &framed(
            "POST",
            &valid_path,
            serde_json::json!({"expected_row":local_latest})
                .to_string()
                .as_bytes(),
            &format!("Origin: http://{ORIGIN}\r\n"),
        ),
    );
    assert_eq!(status(&no_token), 403, "{no_token}");
    let bad_origin = checked_exchange(
        Arc::clone(&state),
        &framed(
            "POST",
            &valid_path,
            serde_json::json!({"expected_row":local_latest})
                .to_string()
                .as_bytes(),
            &format!(
                "Origin: http://other.invalid\r\n{}: {}\r\n",
                run_route::TOKEN_HEADER,
                state.run_route.token().expect("token")
            ),
        ),
    );
    assert_eq!(status(&bad_origin), 403, "{bad_origin}");
    let method = get(&state, &valid_path);
    assert_eq!(status(&method), 405, "{method}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("local history")
            .len(),
        1
    );
    assert_eq!(
        rp::history(&ledger, &foreign_root.display().to_string())
            .expect("history")
            .len(),
        1
    );
    assert_eq!(
        std::fs::read(foreign_root.join(run_route::PERMISSION_FILE)).expect("cache"),
        foreign_cache
    );
    assert_eq!(
        std::fs::read(other.join(run_route::PERMISSION_FILE)).expect("local cache"),
        local_cache
    );
}

#[test]
fn a_historical_grant_with_a_missing_cache_stays_visible_and_can_be_taken_back() {
    let (_tmp, state, hq, root, other) = fixture();
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let given = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    std::fs::remove_file(other.join(run_route::PERMISSION_FILE)).expect("remove cache");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    std::fs::remove_dir_all(&other).expect("removed historical worktree");
    let doc = permissions(&state, &hq);
    let historical = doc["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("historical row");
    assert_eq!(historical["state"], "file_missing");
    assert_eq!(historical["historical"], true);
    assert_eq!(historical["action"], "revoke");
    assert!(historical["actor"].is_null());
    assert!(historical["at"].is_null());
    let expected_row = historical["latest_row"].as_str().expect("grant row");
    let taken_back = post(
        &state,
        &format!("/api/hq/projects/{hq}/run-permissions/{id}/revoke"),
        &serde_json::json!({"expected_row":expected_row}),
        true,
    );
    assert_eq!(status(&taken_back), 200, "{taken_back}");
    let after = permissions(&state, &hq);
    let historical = after["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("historical row");
    assert_eq!(historical["state"], "revoked");
    assert!(historical["action"].is_null());
    assert!(historical["actor"].is_null());
    assert!(historical["at"].is_null());
}

#[test]
fn a_sealed_cache_without_a_record_is_shown_as_not_in_record() {
    let (_tmp, state, hq, _root, other) = fixture();
    run_route::write_sealed_file_for_test(
        &state.db,
        &other,
        "synthetic person",
        "2026-01-03T00:00:00Z",
        "rpm_missing",
    )
    .expect("sealed cache");
    let doc = permissions(&state, &hq);
    let row = doc["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B");
    assert_eq!(row["state"], "not_in_record");
    assert_eq!(row["action"], "grant");
    assert!(row["latest_row"].is_null());
    assert!(row["actor"].is_null());
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert!(rp::history(&ledger, &other.display().to_string())
        .expect("history")
        .is_empty());
    assert!(!doc.to_string().contains(&other.display().to_string()));
}

#[cfg(unix)]
#[test]
fn a_link_at_the_cache_name_disables_allow_again_and_refuses_the_write() {
    let (tmp, state, hq, _root, other) = fixture();
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|row| row["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("folder id")
        .to_string();
    let given = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    let cache = other.join(run_route::PERMISSION_FILE);
    std::fs::remove_file(&cache).expect("remove cache");
    let sentinel = tmp.path().join("outside-cache-sentinel");
    std::fs::write(&sentinel, b"unchanged sentinel").expect("sentinel");
    std::os::unix::fs::symlink(&sentinel, &cache).expect("replace cache with link");
    let doc = permissions(&state, &hq);
    let row = doc["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|row| row["id"] == id)
        .expect("B");
    assert_eq!(row["state"], "file_missing");
    assert!(row["action"].is_null());
    let latest = row["latest_row"].as_str().expect("grant row");
    let refused = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":latest}),
        true,
    );
    assert_eq!(status(&refused), 409, "{refused}");
    assert_eq!(
        response_json(&refused)["error"],
        "permission_file_not_plain"
    );
    assert!(std::fs::symlink_metadata(&cache)
        .expect("cache name")
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::read(&sentinel).expect("sentinel"),
        b"unchanged sentinel"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("history")
            .len(),
        1
    );
}

#[test]
fn settings_grant_refuses_a_change_rebound_after_the_row_was_read() {
    let (_tmp, state, hq, root, other) = fixture();
    let before = permissions(&state, &hq);
    let old = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B");
    let expected_folder = old["id"].as_str().expect("folder");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    let answer = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":expected_folder,"expected_row":null}),
        true,
    );
    assert_eq!(status(&answer), 409, "{answer}");
    assert_eq!(response_json(&answer)["error"], "permission_target_changed");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert!(rp::history(&ledger, &root.display().to_string())
        .expect("root history")
        .is_empty());
    assert!(rp::history(&ledger, &other.display().to_string())
        .expect("old history")
        .is_empty());
    assert!(!root.join(run_route::PERMISSION_FILE).exists());
    assert!(!other.join(run_route::PERMISSION_FILE).exists());
}

#[test]
fn the_check_sheet_empty_object_still_grants_and_revokes_the_current_folder() {
    let (_tmp, state, _hq, _root, other) = fixture();
    let path = format!("/api/changes/{CHANGE_B}/run/permission");
    let allowed = post(&state, &path, &serde_json::json!({}), true);
    assert_eq!(status(&allowed), 200, "{allowed}");
    assert!(other.join(run_route::PERMISSION_FILE).exists());
    let revoked = post(
        &state,
        &format!("{path}/revoke"),
        &serde_json::json!({}),
        true,
    );
    assert_eq!(status(&revoked), 200, "{revoked}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    let history = rp::history(&ledger, &other.display().to_string()).expect("history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[1].revokes.as_deref(), Some(history[0].id.as_str()));
    assert!(!other.join(run_route::PERMISSION_FILE).exists());
}

#[test]
fn a_deleted_current_worktree_refuses_permission_writes_without_a_row() {
    let (_tmp, state, hq, _root, other) = fixture();
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("folder identity")
        .to_string();
    std::fs::remove_dir_all(&other).expect("remove worktree");
    for (body, expected_error) in [
        (
            serde_json::json!({"expected_folder":id,"expected_row":null}),
            "permission_target_changed",
        ),
        (serde_json::json!({}), "no_project_folder"),
    ] {
        let answer = post(
            &state,
            &format!("/api/changes/{CHANGE_B}/run/permission"),
            &body,
            true,
        );
        assert_eq!(status(&answer), 409, "{answer}");
        assert_eq!(response_json(&answer)["error"], expected_error);
        assert!(!answer.contains(&other.display().to_string()));
    }
    let guarded_revoke = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission/revoke"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&guarded_revoke), 409, "{guarded_revoke}");
    assert_eq!(
        response_json(&guarded_revoke)["error"],
        "permission_target_changed"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert!(rp::history(&ledger, &other.display().to_string())
        .expect("history")
        .is_empty());
}

#[cfg(unix)]
#[test]
fn a_replaced_current_worktree_symlink_refuses_guarded_grant_and_revoke() {
    let (tmp, state, hq, _root, other) = fixture();
    let target = tmp.path().join("different-folder");
    std::fs::create_dir_all(target.join(".thruline")).expect("target");
    let sentinel = target.join(run_route::PERMISSION_FILE);
    std::fs::write(&sentinel, b"untouched sentinel").expect("sentinel");
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let stored = tmp.path().join("stored-private");
    std::fs::rename(&other, &stored).expect("move old worktree");
    std::os::unix::fs::symlink(&target, &other).expect("replace with symlink");
    let refused_grant = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&refused_grant), 409, "{refused_grant}");
    let legacy_grant = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({}),
        true,
    );
    assert_eq!(status(&legacy_grant), 409, "{legacy_grant}");
    assert_eq!(
        std::fs::read(&sentinel).expect("sentinel"),
        b"untouched sentinel"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert!(rp::history(&ledger, &other.display().to_string())
        .expect("history")
        .is_empty());
    drop(ledger);
    std::fs::remove_file(&other).expect("unlink replacement");
    std::fs::rename(&stored, &other).expect("restore worktree");
    let given = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    let granted = permissions(&state, &hq);
    let row = granted["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("B");
    let latest = row["latest_row"].as_str().expect("grant row");
    std::fs::rename(&other, &stored).expect("move old worktree again");
    std::os::unix::fs::symlink(&target, &other).expect("replace with symlink again");
    let refused_revoke = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission/revoke"),
        &serde_json::json!({"expected_folder":id,"expected_row":latest}),
        true,
    );
    assert_eq!(status(&refused_revoke), 409, "{refused_revoke}");
    let legacy_revoke = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission/revoke"),
        &serde_json::json!({}),
        true,
    );
    assert_eq!(status(&legacy_revoke), 409, "{legacy_revoke}");
    assert_eq!(
        std::fs::read(&sentinel).expect("sentinel"),
        b"untouched sentinel"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("history")
            .len(),
        1
    );
}

#[cfg(unix)]
#[test]
fn historical_revoke_refuses_a_replacement_symlink_without_appending_or_removing() {
    let (tmp, state, hq, root, other) = fixture();
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let given = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    let historical = permissions(&state, &hq);
    let latest = historical["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("historical B")["latest_row"]
        .as_str()
        .expect("grant row")
        .to_string();
    let stored = tmp.path().join("stored-private");
    std::fs::rename(&other, &stored).expect("move old worktree");
    let target = tmp.path().join("different-folder");
    std::fs::create_dir_all(target.join(".thruline")).expect("target");
    let sentinel = target.join(run_route::PERMISSION_FILE);
    std::fs::write(&sentinel, b"untouched sentinel").expect("sentinel");
    std::os::unix::fs::symlink(&target, &other).expect("replace with symlink");
    let inaccessible = permissions(&state, &hq);
    let row = inaccessible["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|row| row["id"] == id)
        .expect("historical grant");
    assert_eq!(row["state"], "file_missing");
    assert!(
        row["action"].is_null(),
        "a replaced link cannot be taken back"
    );
    let answer = post(
        &state,
        &format!("/api/hq/projects/{hq}/run-permissions/{id}/revoke"),
        &serde_json::json!({"expected_row":latest}),
        true,
    );
    assert_eq!(status(&answer), 409, "{answer}");
    assert_eq!(
        std::fs::read(&sentinel).expect("sentinel"),
        b"untouched sentinel"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    let history = rp::history(&ledger, &other.display().to_string()).expect("history");
    assert_eq!(history.len(), 1);
    assert!(stored.join(run_route::PERMISSION_FILE).exists());
}

#[cfg(unix)]
#[test]
fn historical_revoke_refuses_a_replaced_ancestor_symlink() {
    let (tmp, state, hq, root, other) = fixture();
    let before = permissions(&state, &hq);
    let id = before["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["change_id"] == CHANGE_B)
        .expect("B")["id"]
        .as_str()
        .expect("id")
        .to_string();
    let given = post(
        &state,
        &format!("/api/changes/{CHANGE_B}/run/permission"),
        &serde_json::json!({"expected_folder":id,"expected_row":null}),
        true,
    );
    assert_eq!(status(&given), 200, "{given}");
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        CHANGE_B,
        "2026-01-02T00:02:00Z",
        Some(&root),
    )
    .expect("rebind")
    .expect("binding");
    drop(ledger);
    let latest = permissions(&state, &hq)["folders"]
        .as_array()
        .expect("folders")
        .iter()
        .find(|r| r["id"] == id)
        .expect("historical B")["latest_row"]
        .as_str()
        .expect("grant row")
        .to_string();
    let stored = tmp.path().join("stored-repo");
    std::fs::rename(&root, &stored).expect("move ancestor");
    let replacement = tmp.path().join("replacement-repo");
    std::fs::create_dir_all(replacement.join("private/.thruline")).expect("replacement");
    assert!(std::process::Command::new("git")
        .args(["init", "-q"])
        .arg(&replacement)
        .status()
        .expect("git init replacement")
        .success());
    let sentinel = replacement.join("private").join(run_route::PERMISSION_FILE);
    std::fs::write(&sentinel, b"untouched ancestor sentinel").expect("sentinel");
    std::os::unix::fs::symlink(&replacement, &root).expect("replace ancestor with symlink");
    let answer = post(
        &state,
        &format!("/api/hq/projects/{hq}/run-permissions/{id}/revoke"),
        &serde_json::json!({"expected_row":latest}),
        true,
    );
    assert_eq!(status(&answer), 409, "{answer}");
    assert_eq!(
        std::fs::read(&sentinel).expect("sentinel"),
        b"untouched ancestor sentinel"
    );
    let ledger = thruline_core::ledger::Ledger::open(&state.db).expect("ledger");
    assert_eq!(
        rp::history(&ledger, &other.display().to_string())
            .expect("history")
            .len(),
        1
    );
    assert!(stored
        .join("private")
        .join(run_route::PERMISSION_FILE)
        .exists());
}
