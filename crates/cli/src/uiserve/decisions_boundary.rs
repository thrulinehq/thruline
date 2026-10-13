//! A serve-time presentation boundary around the decisions engine's JSON replies.
//! The engine continues to derive and serialize its own answer. This scope
//! only replaces excluded display text immediately before the socket write.

use std::borrow::Cow;
use std::cell::RefCell;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use thruline_core::capture_exclusion::ExclusionPolicy;
use thruline_core::hq;
use thruline_core::ledger::Ledger;

use super::percent_decode;

enum Scope {
    Project { db: PathBuf, encoded_id: String },
    Job { db: PathBuf },
}

const GROUP_MARKER: &str = "__thruline_decisions_presentation_entry";
const DETAIL_GROUP: &str = "__thruline_decisions_detail_presentation";

thread_local! {
    static CURRENT: RefCell<Option<Scope>> = const { RefCell::new(None) };
}

pub(super) struct Guard(Option<Scope>);

/// The confirmation-token address has its own response contract; every
/// project JSON read and write response enters the same presentation scope.
pub(super) fn covers(method: &str, path: &str) -> bool {
    matches!(method, "GET" | "POST") && path != "/api/hq/decisions/confirmation-token"
}

/// One connection is handled on one thread. Restore any prior scope on every
/// exit path, including errors and unwinding, so unrelated responses cannot
/// inherit a decisions policy.
pub(super) fn enter(db: &Path, path: &str) -> Guard {
    let scope = if path == "/api/hq/decisions/check" {
        Scope::Job {
            db: db.to_path_buf(),
        }
    } else {
        let encoded_id = path
            .trim_matches('/')
            .split('/')
            .nth(3)
            .unwrap_or_default()
            .to_owned();
        Scope::Project {
            db: db.to_path_buf(),
            encoded_id,
        }
    };
    install(scope)
}

fn install(scope: Scope) -> Guard {
    Guard(CURRENT.with(|slot| slot.replace(Some(scope))))
}

impl Drop for Guard {
    fn drop(&mut self) {
        CURRENT.with(|slot| {
            slot.replace(self.0.take());
        });
    }
}

/// `None` leaves the ordinary writer alone. Successful decisions read and
/// write replies can carry evidence; errors are fixed codes from the engine.
pub(super) fn present<'a>(
    status: u16,
    mime: &str,
    body: &'a [u8],
) -> Option<Result<Cow<'a, [u8]>>> {
    if status != 200 || !mime.starts_with("application/json") {
        return None;
    }
    CURRENT.with(|slot| slot.borrow().as_ref().map(|scope| present_for(scope, body)))
}

fn present_for<'a>(scope: &Scope, body: &'a [u8]) -> Result<Cow<'a, [u8]>> {
    let (db, project_id) = match scope {
        Scope::Project { db, encoded_id } => {
            let id = percent_decode(encoded_id).context("invalid decisions project id")?;
            anyhow::ensure!(
                !id.is_empty() && !id.contains('/') && id != "." && id != "..",
                "invalid decisions project id"
            );
            (db, id)
        }
        Scope::Job { db } => {
            let doc: serde_json::Value = serde_json::from_slice(body)?;
            let id = doc["project_id"]
                .as_str()
                .context("decisions job has no project policy key")?;
            (db, id.to_owned())
        }
    };
    let ledger = Ledger::open(db).context("reading decisions capture holds")?;
    let project = hq::find_project(&ledger, &project_id)?
        .context("decisions project has no capture policy root")?;
    let policy = crate::exclusion_serve::policy(&ledger, &project.root)?;
    present_decisions(body, &policy)
}

/// A decision's source path can sit in `sources` while its `span`, verdict
/// reason, or proof prose sits beside it without repeating that path. Treat
/// each decision as one display entry for exclusion purposes, then discard
/// the temporary grouping. The original bytes leave unchanged on a miss.
fn present_decisions<'a>(body: &'a [u8], policy: &ExclusionPolicy) -> Result<Cow<'a, [u8]>> {
    if policy.is_empty() {
        return Ok(Cow::Borrowed(body));
    }
    let mut doc: serde_json::Value = serde_json::from_slice(body)?;
    let detail_grouped = group_detail(&mut doc);
    group_post_review(&mut doc);
    group_entries(&mut doc);
    if crate::exclusion_serve::checked_apply(&mut doc, policy)? == 0 {
        return Ok(Cow::Borrowed(body));
    }
    ungroup_entries(&mut doc)?;
    if detail_grouped {
        let fields = doc
            .as_object_mut()
            .context("decisions detail lost its object")?;
        let group = fields
            .remove(DETAIL_GROUP)
            .context("decisions detail presentation group missing")?;
        let parts = group
            .as_object()
            .context("decisions detail presentation group changed shape")?;
        for (key, value) in parts {
            fields.insert(key.clone(), value.clone());
        }
    }
    Ok(Cow::Owned(serde_json::to_vec(&doc)?))
}

fn group_post_review(doc: &mut serde_json::Value) {
    let Some(review) = doc
        .as_object_mut()
        .and_then(|fields| fields.get_mut("review"))
    else {
        return;
    };
    if review.is_object() {
        *review = grouped(std::mem::take(review));
    }
}

fn group_detail(doc: &mut serde_json::Value) -> bool {
    let Some(fields) = doc.as_object_mut() else {
        return false;
    };
    if !fields.contains_key("decision") {
        return false;
    }
    let mut parts = serde_json::Map::new();
    for key in ["decision", "verdict", "chain"] {
        if let Some(value) = fields.remove(key) {
            parts.insert(key.to_owned(), value);
        }
    }
    fields.insert(
        DETAIL_GROUP.to_owned(),
        grouped(serde_json::Value::Object(parts)),
    );
    true
}

fn group_entries(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(fields) => {
            for (key, child) in fields {
                if matches!(
                    key.as_str(),
                    "decisions"
                        | "proposals"
                        | "notes"
                        | "conflicts"
                        | "revivals"
                        | "threads"
                        | "missing_evidence"
                        | "findings"
                ) {
                    if let Some(rows) = child.as_array_mut() {
                        for row in rows {
                            if row.is_object() {
                                *row = grouped(std::mem::take(row));
                            } else {
                                group_entries(row);
                            }
                        }
                        continue;
                    }
                }
                group_entries(child);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                group_entries(item);
            }
        }
        _ => {}
    }
}

fn grouped(evidence: serde_json::Value) -> serde_json::Value {
    let mut summary = Vec::new();
    collect_text(&evidence, &mut summary);
    let mut fields = serde_json::Map::new();
    fields.insert(GROUP_MARKER.into(), serde_json::Value::Bool(true));
    fields.insert("summary".into(), serde_json::json!(summary));
    fields.insert("evidence".into(), evidence);
    serde_json::Value::Object(fields)
}

fn collect_text(value: &serde_json::Value, out: &mut Vec<String>) {
    match value {
        serde_json::Value::String(text) => out.push(text.clone()),
        serde_json::Value::Array(items) => {
            for item in items {
                collect_text(item, out);
            }
        }
        serde_json::Value::Object(fields) => {
            for child in fields.values() {
                collect_text(child, out);
            }
        }
        _ => {}
    }
}

fn ungroup_entries(value: &mut serde_json::Value) -> Result<()> {
    if let serde_json::Value::Object(fields) = value {
        if fields.get(GROUP_MARKER) == Some(&serde_json::Value::Bool(true)) {
            let evidence = fields
                .remove("evidence")
                .context("decisions presentation entry lost its evidence")?;
            *value = evidence;
            return ungroup_entries(value);
        }
        for child in fields.values_mut() {
            ungroup_entries(child)?;
        }
    } else if let serde_json::Value::Array(items) = value {
        for item in items {
            ungroup_entries(item)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::{TcpListener, TcpStream};

    use tempfile::TempDir;
    use thruline_core::capture_hold::{self, Act, Hold};
    use thruline_core::model::{ProjectRef, Source};

    use super::*;

    const FIRST_ROOT: &str = "/workspace/c46-decisions-first";
    const SECOND_ROOT: &str = "/workspace/c46-decisions-second";

    fn fixture() -> (TempDir, PathBuf, String, String) {
        let temp = tempfile::tempdir().expect("temporary ledger folder");
        let db = temp.path().join("ledger.db");
        let ledger = Ledger::open(&db).expect("synthetic ledger");
        for (id, root) in [("member-one", FIRST_ROOT), ("member-two", SECOND_ROOT)] {
            ledger
                .upsert_project(
                    &ProjectRef {
                        id: id.into(),
                        root_path: root.into(),
                        source: Source::Codex,
                        display_name: None,
                    },
                    "2026-10-08T00:00:00Z",
                )
                .expect("synthetic HQ member");
        }
        let hold = Hold {
            project_id: "member-one",
            project_folder: FIRST_ROOT,
            actor: "synthetic person",
            recorded_at: "2026-10-08T00:00:01Z",
        };
        capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("src/private.rs"))
            .expect("excluded path");
        (
            temp,
            db,
            hq::project_id_for_root(FIRST_ROOT),
            hq::project_id_for_root(SECOND_ROOT),
        )
    }

    fn project(db: &Path, id: &str) -> Scope {
        Scope::Project {
            db: db.to_path_buf(),
            encoded_id: id.into(),
        }
    }

    fn socket_response(scope: Scope, body: Vec<u8>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accepted connection");
            let _guard = install(scope);
            super::super::respond(&mut stream, 200, "application/json; charset=utf-8", &body)
                .expect("one response");
        });
        let mut client = TcpStream::connect(address).expect("loopback client");
        let mut response = String::new();
        client.read_to_string(&mut response).expect("HTTP response");
        server.join().expect("server thread");
        response
    }

    #[test]
    fn only_the_matching_hq_project_policy_replaces_decisions_display_text() {
        let (_temp, db, first, second) = fixture();
        let body = format!(
            "{{ \"project_id\":\"{first}\",\"decisions\":[{{\"id\":\"d-1\",\"file\":\"src/private.rs\",\"summary\":\"pathless file contents\",\"count\":2}}] }}"
        );
        let filtered = present_for(&project(&db, &first), body.as_bytes()).expect("first policy");
        let doc: serde_json::Value = serde_json::from_slice(&filtered).expect("filtered JSON");
        assert_eq!(doc["decisions"][0]["id"], "d-1");
        assert_eq!(doc["decisions"][0]["count"], 2);
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert!(filtered
            .windows(b"left out by your exclusion".len())
            .any(|w| w == b"left out by your exclusion"));
        assert!(!filtered
            .windows(b"pathless file contents".len())
            .any(|w| w == b"pathless file contents"));

        // Same bytes under another HQ project have no matching choice. Keep
        // their original spacing and field order exactly.
        let untouched =
            present_for(&project(&db, &second), body.as_bytes()).expect("second policy");
        assert!(matches!(&untouched, Cow::Borrowed(_)));
        assert_eq!(untouched.as_ref(), body.as_bytes());
    }

    #[test]
    fn nested_source_path_hides_its_pathless_decision_span_but_not_another_decision() {
        let (_temp, db, first, _) = fixture();
        let body = serde_json::json!({
            "project_id": first,
            "decisions": [
                {"id":"d-private", "span":"PRIVATE SPAN WITHOUT A PATH",
                 "sources":[{"path":"src/private.rs", "line":7}],
                 "current_verdict":{"id":"v-private", "verdict":"pass", "checked_places":3,
                                    "reason":"PRIVATE REASON WITHOUT A PATH"}},
                {"id":"d-public", "span":"PUBLIC SPAN",
                 "sources":[{"path":"src/public.rs", "line":8}],
                 "current_verdict":{"id":"v-public", "verdict":"fail", "checked_places":2,
                                    "reason":"PUBLIC REASON"}}
            ]
        });
        let raw = serde_json::to_vec(&body).expect("synthetic decisions");
        let filtered = present_for(&project(&db, &first), &raw).expect("decisions policy");
        let doc: serde_json::Value = serde_json::from_slice(&filtered).expect("filtered decisions");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert_eq!(doc["decisions"][0]["id"], "d-private");
        assert_eq!(doc["decisions"][0]["current_verdict"]["verdict"], "pass");
        assert_eq!(doc["decisions"][0]["current_verdict"]["checked_places"], 3);
        assert_eq!(
            doc["decisions"][0]["sources"][0]["path"],
            thruline_core::capture_exclusion::marker("src/private.rs")
        );
        assert_eq!(doc["decisions"][1], body["decisions"][1]);
        let served = String::from_utf8_lossy(&filtered);
        assert!(!served.contains("PRIVATE SPAN WITHOUT A PATH"));
        assert!(!served.contains("PRIVATE REASON WITHOUT A PATH"));
    }

    #[test]
    fn detail_groups_decision_verdict_and_chain_display_without_changing_identity() {
        let (_temp, db, first, _) = fixture();
        let body = serde_json::json!({
            "schema_version":1,
            "decision":{"id":"d-private", "span":"PRIVATE DETAIL SPAN",
                        "sources":[{"path":"src/private.rs"}]},
            "verdict":{"id":"v-private", "verdict":"pass", "reason":"PRIVATE VERDICT REASON"},
            "chain":[{"id":"proof-1", "summary":"PRIVATE PROOF TEXT"}],
            "approval_gate":{"active":true}
        });
        let raw = serde_json::to_vec(&body).expect("synthetic detail");
        let filtered = present_for(&project(&db, &first), &raw).expect("detail policy");
        let doc: serde_json::Value = serde_json::from_slice(&filtered).expect("filtered detail");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert_eq!(doc["decision"]["id"], "d-private");
        assert_eq!(doc["verdict"]["id"], "v-private");
        assert_eq!(doc["verdict"]["verdict"], "pass");
        assert_eq!(doc["chain"][0]["id"], "proof-1");
        assert_eq!(doc["approval_gate"]["active"], true);
        assert!(doc.get(DETAIL_GROUP).is_none());
        let served = String::from_utf8_lossy(&filtered);
        assert!(!served.contains("PRIVATE DETAIL SPAN"));
        assert!(!served.contains("PRIVATE VERDICT REASON"));
        assert!(!served.contains("PRIVATE PROOF TEXT"));
    }

    #[test]
    fn project_post_replies_enter_the_boundary_and_hide_review_and_decision_evidence() {
        assert!(covers(
            "POST",
            "/api/hq/projects/project-1/decisions/d-1/native-review"
        ));
        assert!(covers(
            "POST",
            "/api/hq/projects/project-1/decisions/d-1/accept"
        ));
        assert!(!covers("POST", "/api/hq/decisions/confirmation-token"));

        let (_temp, db, first, _) = fixture();
        for (name, body) in [
            (
                "native review",
                serde_json::json!({"schema_version":1, "review_sha256":"digest",
                    "review":{"id":"review-1", "sources":[{"path":"src/private.rs"}],
                              "reason":"PRIVATE REVIEW WITHOUT PATH"},
                    "preparation_is_person_act":false}),
            ),
            (
                "person act",
                serde_json::json!({"schema_version":1, "id":"act-1", "state":"pending_bridge",
                    "decision":{"id":"decision-1", "sources":[{"path":"src/private.rs"}],
                                "reason":"PRIVATE DECISION WITHOUT PATH"}}),
            ),
        ] {
            let raw = serde_json::to_vec(&body).expect(name);
            let filtered = present_for(&project(&db, &first), &raw).expect(name);
            let served: serde_json::Value = serde_json::from_slice(&filtered).expect(name);
            assert_eq!(served["schema_version"], 1, "{name}");
            assert_eq!(served["id"], body["id"], "{name}");
            assert_eq!(served["state"], body["state"], "{name}");
            assert_eq!(served["review"]["id"], body["review"]["id"], "{name}");
            assert_eq!(served["decision"]["id"], body["decision"]["id"], "{name}");
            let text = String::from_utf8_lossy(&filtered);
            assert!(
                !text.contains("PRIVATE REVIEW WITHOUT PATH"),
                "{name}: {text}"
            );
            assert!(
                !text.contains("PRIVATE DECISION WITHOUT PATH"),
                "{name}: {text}"
            );
            assert!(
                !text.contains("\"path\":\"src/private.rs\""),
                "{name}: {text}"
            );
        }
    }

    #[test]
    fn job_reads_resolve_the_project_key_inside_the_completed_json() {
        let (_temp, db, first, _) = fixture();
        let body = format!(
            "{{\"project_id\":\"{first}\",\"job_id\":\"j-1\",\"errors\":[{{\"file\":\"src/private.rs\",\"text\":\"pathless output\"}}]}}"
        );
        let filtered = present_for(&Scope::Job { db }, body.as_bytes()).expect("job policy");
        let doc: serde_json::Value = serde_json::from_slice(&filtered).expect("filtered job");
        assert_eq!(doc["job_id"], "j-1");
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert!(!String::from_utf8_lossy(&filtered).contains("pathless output"));
    }

    #[test]
    fn unreadable_policy_writes_one_fixed_refusal_before_any_original_bytes() {
        let temp = tempfile::tempdir().expect("temporary folder");
        let db = temp.path().join("ledger.db");
        std::fs::write(&db, b"not a sqlite ledger").expect("corrupt ledger");
        let response = socket_response(
            project(&db, "missing"),
            br#"{"secret":"src/private.rs"}"#.to_vec(),
        );
        assert_eq!(response.matches("HTTP/1.1 ").count(), 1);
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");
        assert!(response.ends_with("{\"error\":\"exclusion_policy_unavailable\"}"));
        assert!(!response.contains("src/private.rs"));
    }

    #[test]
    fn generated_lookup_failure_also_writes_one_fixed_refusal() {
        let (_temp, db, first, _) = fixture();
        let ledger = Ledger::open(&db).expect("synthetic ledger");
        let hold = Hold {
            project_id: "member-one",
            project_folder: FIRST_ROOT,
            actor: "synthetic person",
            recorded_at: "2026-10-08T00:00:02Z",
        };
        capture_hold::record_for_person(&ledger, Act::Excluded, &hold, Some("generated"))
            .expect("generated choice");
        drop(ledger);
        let response = socket_response(
            project(&db, &first),
            br#"{"decisions":[{"id":"d-1","file":"src/generated.rs","summary":"pathless generated contents"}]}"#.to_vec(),
        );
        assert_eq!(response.matches("HTTP/1.1 ").count(), 1);
        assert!(response.starts_with("HTTP/1.1 503"), "{response}");
        assert!(!response.contains("src/generated.rs"));
        assert!(!response.contains("pathless generated contents"));
    }
}
