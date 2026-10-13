//! AI-REVIEW-OUT-OF-LAUNCH (founder ruling R-2026-09-28-20) — the launch build
//! offers no AI review, and "Review my change" runs the deterministic review
//! only.
//!
//! The launch build is the default build: AI review exists only behind the
//! `ai-review` cargo feature, which the release path does not pass. This file
//! pins the launch configuration, so it compiles only without that feature.
//!
//! Every door is driven through the compiled binary with a runtime registry
//! that DOES register a PR-Agent runtime beside one deterministic tool
//! (`ast-grep`, answered by `/bin/echo`, as `integration_workflow.rs` does).
//! That is the hard case: a machine where PR-Agent is installed must still
//! not be offered it by the launch build. A registry that merely lacks it
//! would prove nothing.
//!
//! Every fixture is synthetic.
#![cfg(all(target_os = "macos", not(feature = "ai-review")))]

use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;

#[path = "support/review_fixture.rs"]
mod fixture;
use fixture::{Fixture, Server, NOT_A_SPELLING, ROUND_4, ROUND_5};

/// The T-4-1a plain sentence a path that still reaches PR-Agent leads with.
const TOOL_NOT_SET_UP: &str =
    "Review findings aren't available: the review tool isn't set up on this machine.";

/// What no surface may say about a review that could not happen.
const NEVER: &[&str] = &[
    "no findings",
    "0 findings",
    "found nothing",
    "raised nothing",
];

fn mentions_pr_agent(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    lower.contains("pr-agent") || lower.contains("pr_agent") || lower.contains("11436")
}

/* ── the served recipe and catalog lists ───────────────────────────────── */

#[test]
fn the_launch_build_lists_no_pr_agent_recipe_and_no_pr_agent_source() {
    let f = Fixture::new();
    let root = f.root.display().to_string();
    let exec = f.exec.display().to_string();

    let (code, out, err) = f.run(&[
        "integration",
        "workflow",
        "--root",
        &root,
        "--exec-db",
        &exec,
        "--action",
        "status",
    ]);
    assert_eq!(code, 0, "{err}");
    let workflow: Value = serde_json::from_str(&out).expect("json");
    let ids: Vec<&str> = workflow["recipes"]
        .as_array()
        .expect("recipes")
        .iter()
        .filter_map(|r| r["id"].as_str())
        .collect();
    assert!(
        ids.contains(&"ast-grep"),
        "the deterministic recipes stay: {ids:?}"
    );
    assert!(
        !ids.contains(&"pr-agent"),
        "no PR-Agent recipe is listed: {ids:?}"
    );
    assert_eq!(
        workflow["ai_review"],
        json!(false),
        "the served flag says so"
    );
    assert!(!mentions_pr_agent(&out), "{out}");

    let (code, out, err) = f.run(&["integration", "catalog"]);
    assert_eq!(code, 0, "{err}");
    let catalog: Value = serde_json::from_str(&out).expect("json");
    assert!(
        catalog["projects"]
            .as_array()
            .expect("projects")
            .iter()
            .all(|p| p["id"] != json!("pr-agent")),
        "the listed sources hold no PR-Agent"
    );
    assert!(!mentions_pr_agent(&out));

    let (code, out, err) = f.run(&[
        "integration",
        "project",
        "--presentation",
        "--root",
        &root,
        "--exec-db",
        &exec,
    ]);
    assert_eq!(code, 0, "{err}");
    assert!(
        !mentions_pr_agent(&out),
        "the served tool-evidence projection names no PR-Agent: {out}"
    );

    // Asked for by name, the recipe is refused, and nothing is claimed.
    for action in ["preview", "accept", "run"] {
        let (code, out, err) = f.run(&[
            "integration",
            "workflow",
            "--root",
            &root,
            "--exec-db",
            &exec,
            "--action",
            action,
            "--recipe",
            "pr-agent",
        ]);
        assert_ne!(code, 0, "{action}: {out}");
        for word in NEVER {
            assert!(!err.contains(word), "{word:?} in {err}");
        }
    }
}

/* ── "Review my change" ────────────────────────────────────────────────── */

#[test]
fn review_my_change_runs_the_deterministic_review_only() {
    let f = Fixture::new();
    let (code, out, err) = f.review("preview", "change", "");
    assert_eq!(code, 0, "{out}{err}");
    let plan: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(plan["mode"], json!("change"));
    assert_eq!(plan["inventory"]["change"]["base_commit"], json!(f.base));
    let recipes: Vec<&str> = plan["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter_map(|c| c["recipe"].as_str())
        .collect();
    assert_eq!(
        recipes,
        ["ast-grep"],
        "only the deterministic check is planned"
    );
    assert!(
        !mentions_pr_agent(&out),
        "the plan names no AI reviewer anywhere: {out}"
    );
    assert!(!mentions_pr_agent(&err), "nor does the terminal: {err}");
    assert_eq!(
        plan["review_findings"],
        Value::Null,
        "the deterministic review says nothing about AI review findings"
    );

    let digest = plan["digest"].as_str().expect("digest").to_string();
    let (code, out, err) = f.review("accept", "change", &digest);
    assert_eq!(code, 0, "{out}{err}");
    let job: Value = serde_json::from_str(&out).expect("json");
    assert!(!mentions_pr_agent(&out), "{out}");
    let id = job["id"].as_str().expect("job id").to_string();

    let (code, out, err) = f.review("step", "change", &id);
    assert_eq!(code, 0, "the deterministic check runs: {out}{err}");
    let stepped: Value = serde_json::from_str(&out).expect("json");
    let checks = stepped["checks"].as_array().expect("checks");
    assert_eq!(checks.len(), 1, "{out}");
    assert_eq!(checks[0]["recipe"], json!("ast-grep"));
    assert_ne!(
        checks[0]["state"],
        json!("queued"),
        "the step ran the deterministic check: {out}"
    );
    assert!(
        !checks[0]["attempts"]
            .as_array()
            .expect("attempts")
            .is_empty(),
        "and kept its attempt: {out}"
    );
    assert!(!mentions_pr_agent(&out), "{out}");
    assert!(!out.contains(TOOL_NOT_SET_UP) && !err.contains(TOOL_NOT_SET_UP));
}

/* ── a path that still reaches PR-Agent keeps T-4-1a's plain words ─────── */

#[test]
fn a_change_bound_review_still_says_review_findings_are_unavailable() {
    let f = Fixture::new();
    let (_, out, _) = f.review("preview", "change", "");
    let plan: Value = serde_json::from_str(&out).expect("json");
    let digest = plan["digest"].as_str().expect("digest").to_string();
    let root = f.root.display().to_string();
    let exec = f.exec.display().to_string();
    let (code, out, err) = f.run(&[
        "integration",
        "review",
        "--root",
        &root,
        "--exec-db",
        &exec,
        "--action",
        "accept",
        "--mode",
        "change",
        "--digest",
        &digest,
        "--change-id",
        "chg_synthetic",
        "--examined-inputs-id",
        "exi_synthetic",
    ]);
    assert_ne!(code, 0, "an AI review is never accepted: {out}");
    let first = err
        .lines()
        .find(|l| l.starts_with("error: "))
        .unwrap_or_default();
    assert_eq!(
        first.strip_prefix("error: ").unwrap_or(first),
        TOOL_NOT_SET_UP,
        "the refusal leads with the plain sentence: {err}"
    );
    for word in NEVER {
        assert!(!err.contains(word), "{word:?} in {err}");
    }
}

/* ── the HQ routes ─────────────────────────────────────────────────────── */

#[test]
fn the_hq_routes_serve_and_accept_no_pr_agent() {
    let f = Fixture::new();
    let server = Server::start(&f);

    let (status, body) = server.request("GET", "/api/integrations?session=review-t", None);
    assert_eq!(status, 200, "{body}");
    let doc: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(doc["workflow"]["ai_review"], json!(false), "{body}");
    assert!(
        !mentions_pr_agent(&body),
        "the served recipe list and catalog name no PR-Agent: {body}"
    );

    let (status, body) = server.request(
        "POST",
        "/api/integrations/action",
        Some(r#"{"session":"review-t","action":"review-preview","recipe":"change"}"#),
    );
    assert_eq!(status, 200, "{body}");
    assert!(!mentions_pr_agent(&body), "{body}");
    let doc: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(doc["review_findings"], Value::Null, "{body}");

    for action in ["preview", "accept", "run", "agent-repair-preview"] {
        let request =
            json!({"session": "review-t", "action": action, "recipe": "pr-agent", "digest": ""});
        let (status, body) = server.request(
            "POST",
            "/api/integrations/action",
            Some(&request.to_string()),
        );
        assert!(
            (400..500).contains(&status),
            "{action} with PR-Agent is refused by the allow-list: {status} {body}"
        );
        for word in NEVER {
            assert!(!body.contains(word), "{word:?} in {body}");
        }
    }

    // A step whose queue cannot be read may concern the review model, so it
    // still leads with the plain sentence (T-4-1a), never with "no findings".
    let (status, body) = server.request(
        "POST",
        "/api/integrations/action",
        Some(r#"{"session":"review-t","action":"review-step","recipe":"change","digest":""}"#),
    );
    assert_eq!(status, 409, "{body}");
    let doc: Value = serde_json::from_str(&body).expect("json");
    let error = doc["error"].as_str().expect("error text");
    assert!(
        error
            .strip_prefix("error: ")
            .unwrap_or(error)
            .starts_with(TOOL_NOT_SET_UP),
        "{error}"
    );
}

/* ── round 2: every served byte ────────────────────────────────────────── */

/// AI-REVIEW-OUT-OF-LAUNCH-R2. What no served UI asset may carry: a PR-Agent
/// entry point, label, capability or catalog row. Case-insensitive, so it
/// catches the display name ("PR-Agent"), the capability id ("pr-agent"),
/// its report format ("pr-agent-json-v1"), its upstream repository
/// ("The-PR-Agent/pr-agent"), its facts ("pr_agent_options") and the recipe
/// title ("Review a change with PR-Agent").
///
/// `11436` is deliberately NOT a needle here, unlike in `mentions_pr_agent`:
/// the served bundle carries it only inside T-4-1a's plain sentence
/// "Review findings aren't available: the review model at 127.0.0.1:11436
/// isn't answering.", which round 1's item 3 keeps. It is reported to the
/// orchestrator, not hidden.
fn names_pr_agent(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    ["pr-agent", "pr_agent", "pr agent", "review a change with"]
        .into_iter()
        .find(|needle| lower.contains(needle))
}

fn dist_files(dir: &Path, base: &Path, out: &mut Vec<String>) {
    for entry in std::fs::read_dir(dir).expect("read ui/dist") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            dist_files(&path, base, out);
        } else {
            let rel = path.strip_prefix(base).expect("under dist");
            out.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
}

impl Server {
    fn get_bytes(&self, path: &str) -> (u16, Vec<u8>) {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect");
        let head = format!(
            "GET {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
            self.port
        );
        stream.write_all(head.as_bytes()).expect("write");
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).expect("read");
        let split = raw
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .expect("header terminator");
        let status = String::from_utf8_lossy(&raw[..split])
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        (status, raw[split + 4..].to_vec())
    }
}

#[test]
fn the_launch_build_serves_no_pr_agent_asset_label_or_catalog_row() {
    let f = Fixture::new();
    let server = Server::start(&f);

    // Every file the launch binary embeds is every file under ui/dist
    // (crates/cli/build.rs walks the whole tree). Fetch each one over HTTP,
    // so this is what a client of `thruline ui` actually receives.
    let dist = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/dist");
    let dist = dist.canonicalize().expect("ui/dist exists");
    let mut files = Vec::new();
    dist_files(&dist, &dist, &mut files);
    files.sort();
    assert!(
        files.iter().any(|f| f.starts_with("assets/integrations-")),
        "the tool-evidence chunk is among the served files: {files:?}"
    );
    let mut paths = vec!["/".to_string()];
    paths.extend(files.iter().map(|f| format!("/{f}")));
    let mut carried = Vec::new();
    for path in &paths {
        let (status, body) = server.get_bytes(path);
        assert_eq!(status, 200, "{path} is served");
        if let Some(needle) = names_pr_agent(&String::from_utf8_lossy(&body)) {
            carried.push(format!("{path}: {needle:?}"));
        }
    }
    assert!(
        carried.is_empty(),
        "served UI assets name PR-Agent: {carried:?}"
    );
}

#[test]
fn every_served_json_route_of_the_launch_build_names_no_pr_agent() {
    let f = Fixture::new();
    // One project and one Change, so the per-Change routes answer with a
    // real Change rather than a 404.
    {
        let ledger = thruline_core::ledger::Ledger::open(&f.db).expect("ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
                 VALUES('cccccccc-0000-4000-8000-00000000r201', ?1, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                rusqlite::params![f.root.display().to_string()],
            )
            .expect("project");
    }
    let (code, out, err) = f.run(&[
        "change",
        "new",
        "--title",
        "Fix addition",
        "--requester",
        "Sam",
        "--purpose",
        "add() should add.",
    ]);
    assert_eq!(code, 0, "{out}{err}");
    let change = out
        .lines()
        .nth(1)
        .expect("the change id")
        .trim()
        .to_string();
    assert!(!change.is_empty(), "{out}");

    let server = Server::start(&f);
    let mut routes: Vec<String> = [
        "/api/integrations?session=review-t",
        "/api/review-projects",
        "/api/state",
        "/api/sessions",
        "/api/changes",
        "/api/lanes",
        "/api/storage",
        "/api/licence",
        "/api/first-use",
        "/api/mind/status",
        "/api/acceptance",
        "/api/run/token",
        "/api/phases?session=review-t",
        "/api/explain?session=review-t",
        "/api/claims?session=review-t",
        "/api/sessions/review-t/changes",
    ]
    .iter()
    .map(|r| r.to_string())
    .collect();
    for tail in [
        "",
        "/coverage",
        "/readiness",
        "/findings",
        "/applicability",
        "/proposals",
        "/inputs",
        "/export",
        "/run",
    ] {
        routes.push(format!("/api/changes/{change}{tail}"));
    }
    let mut answered = 0;
    for route in &routes {
        let (status, body) = server.request("GET", route, None);
        if status == 200 {
            answered += 1;
        }
        assert!(
            !mentions_pr_agent(&body) && names_pr_agent(&body).is_none(),
            "{route} ({status}) names PR-Agent: {body}"
        );
    }
    let (status, body) = server.request("GET", &format!("/api/changes/{change}/coverage"), None);
    assert_eq!(status, 200, "{body}");
    let coverage: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(coverage["ai_review"], json!(false), "{body}");
    assert!(
        answered >= 10,
        "most routes answered 200: {answered} of {}",
        routes.len()
    );
}

/* ── round 3: an operator registry that selects the withheld review ────── */

/// AI-REVIEW-OUT-OF-LAUNCH-R3. What a project's registered selection of the
/// withheld review becomes in the launch build: it is dropped from the
/// selection, and one gap says so without naming it.
///
/// R5 (D-050): this was "a review this build does not offer", the round-3
/// sentence, which called every dropped selection a review. It says "this build
/// runs" now, the phrase both count sentences share (`SELECTION_ONE` and
/// `SELECTION_MANY`, below, which the round-5 tests match exactly).
const NOT_OFFERED: &str = "this build runs";

fn assert_withheld_selection_is_unnamed(plan: &Value, raw: &str) {
    assert!(
        !mentions_pr_agent(raw),
        "the plan names the withheld review: {raw}"
    );
    assert!(names_pr_agent(raw).is_none(), "{raw}");
    let gaps = plan["gaps"].as_array().expect("gaps");
    let unnamed: Vec<&Value> = gaps
        .iter()
        .filter(|g| {
            g["reason"]
                .as_str()
                .is_some_and(|r| r.contains(NOT_OFFERED))
        })
        .collect();
    assert_eq!(
        unnamed.len(),
        1,
        "one gap says a selection was dropped: {raw}"
    );
    assert_eq!(unnamed[0].get("recipe"), None, "and names no recipe: {raw}");
    assert!(
        plan["project"]["recipes"]
            .as_array()
            .is_some_and(|r| r.iter().all(|id| id != "pr-agent")),
        "the served project carries no withheld selection: {raw}"
    );
}

#[test]
fn a_registry_selecting_the_withheld_review_is_dropped_unnamed_from_the_cli_preview() {
    let f = Fixture::new();
    f.select(&["ast-grep", "pr-agent"]);
    for mode in ["full", "change"] {
        let (code, out, err) = f.review("preview", mode, "");
        assert_eq!(code, 0, "{mode}: {out}{err}");
        assert!(!mentions_pr_agent(&err), "{err}");
        let plan: Value = serde_json::from_str(&out).expect("json");
        assert_withheld_selection_is_unnamed(&plan, &out);
        let recipes: Vec<&str> = plan["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .filter_map(|c| c["recipe"].as_str())
            .collect();
        assert_eq!(recipes, ["ast-grep"], "the deterministic review still runs");
    }

    // Accept and step: the accepted queue keeps the plan's project, and
    // nothing it serves names the withheld review either.
    let (_, out, _) = f.review("preview", "change", "");
    let plan: Value = serde_json::from_str(&out).expect("json");
    let digest = plan["digest"].as_str().expect("digest").to_string();
    let (code, out, err) = f.review("accept", "change", &digest);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        !mentions_pr_agent(&out) && !mentions_pr_agent(&err),
        "{out}{err}"
    );
    let job: Value = serde_json::from_str(&out).expect("json");
    let id = job["id"].as_str().expect("job id").to_string();
    for action in ["step", "status"] {
        let (code, out, err) = f.review(action, "change", &id);
        assert_eq!(code, 0, "{action}: {out}{err}");
        assert!(
            !mentions_pr_agent(&out) && !mentions_pr_agent(&err),
            "{action}: {out}{err}"
        );
    }

    // The adversary's exact selection: nothing but the withheld review.
    f.select(&["pr-agent"]);
    let (code, out, err) = f.review("preview", "full", "");
    assert_eq!(code, 0, "{out}{err}");
    assert!(!mentions_pr_agent(&err), "{err}");
    let plan: Value = serde_json::from_str(&out).expect("json");
    assert_withheld_selection_is_unnamed(&plan, &out);
}

#[test]
fn a_registry_selecting_the_withheld_review_is_dropped_unnamed_from_every_hq_route() {
    let f = Fixture::new();
    f.select(&["pr-agent"]);
    let server = Server::start(&f);
    for mode in ["full", "change"] {
        let request =
            json!({"session": "review-t", "action": "review-preview", "recipe": mode}).to_string();
        let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
        assert_eq!(status, 200, "{mode}: {body}");
        let doc: Value = serde_json::from_str(&body).expect("json");
        assert_withheld_selection_is_unnamed(&doc, &body);
    }
    for route in ["/api/review-projects", "/api/integrations?session=review-t"] {
        let (status, body) = server.request("GET", route, None);
        assert_eq!(status, 200, "{route}: {body}");
        assert!(
            !mentions_pr_agent(&body) && names_pr_agent(&body).is_none(),
            "{route} names the withheld review: {body}"
        );
    }
}

/* ── round 4: the withheld review in any spelling ──────────────────────── */

/// AI-REVIEW-OUT-OF-LAUNCH-R4. The round-3 adversary's spellings and the
/// card's: case, separator, padding, an en dash, and no separator at all.
const SPELLINGS: &[&str] = &[
    "PR-Agent",
    "pr_agent",
    "PR_AGENT",
    " pr-agent ",
    "pr\u{2013}agent",
    "pragent",
    "\u{ff30}\u{ff32}\u{ff0d}\u{ff21}gent",
];

/// The same normalisation the card requires of `withheld()`, used only to
/// look for the id in what is served.
fn normalised(text: &str) -> String {
    text.trim()
        .chars()
        .map(|c| match c as u32 {
            0xFF01..=0xFF5E => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            _ => c,
        })
        .flat_map(char::to_lowercase)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn assert_no_spelling_is_served(plan: &Value, raw: &str, spellings: &[&str]) {
    for spelling in spellings {
        assert!(!raw.contains(spelling), "{spelling:?} is served: {raw}");
    }
    assert!(
        !normalised(raw).contains("pragent"),
        "the served plan carries the withheld id in some spelling: {raw}"
    );
    let unnamed: Vec<&Value> = plan["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .filter(|g| {
            g["reason"]
                .as_str()
                .is_some_and(|r| r.contains(NOT_OFFERED))
        })
        .collect();
    assert_eq!(unnamed.len(), 1, "one unnamed gap: {raw}");
    assert_eq!(unnamed[0].get("recipe"), None, "{raw}");
    assert!(
        plan["gaps"]
            .as_array()
            .expect("gaps")
            .iter()
            .all(|g| g["recipe"]
                .as_str()
                .is_none_or(|r| normalised(r) != "pragent")),
        "{raw}"
    );
    assert!(
        plan["project"]["recipes"]
            .as_array()
            .is_some_and(|r| r.iter().all(|id| id == "ast-grep")),
        "the served project keeps only the deterministic selection: {raw}"
    );
}

#[test]
fn a_registry_selecting_the_withheld_review_in_any_spelling_is_dropped_unnamed_from_the_cli_preview(
) {
    let f = Fixture::new();
    for spelling in SPELLINGS {
        f.select(&["ast-grep", spelling]);
        for mode in ["full", "change"] {
            let (code, out, err) = f.review("preview", mode, "");
            assert_eq!(code, 0, "{spelling:?} {mode}: {out}{err}");
            assert!(!err.contains(spelling), "{err}");
            let plan: Value = serde_json::from_str(&out).expect("json");
            assert_no_spelling_is_served(&plan, &out, &[spelling]);
            let recipes: Vec<&str> = plan["checks"]
                .as_array()
                .expect("checks")
                .iter()
                .filter_map(|c| c["recipe"].as_str())
                .collect();
            assert_eq!(recipes, ["ast-grep"], "the deterministic review still runs");
        }
    }
    // Every spelling at once is still one unnamed gap.
    let mut all = vec!["ast-grep"];
    all.extend_from_slice(SPELLINGS);
    f.select(&all);
    let (code, out, err) = f.review("preview", "full", "");
    assert_eq!(code, 0, "{out}{err}");
    let plan: Value = serde_json::from_str(&out).expect("json");
    assert_no_spelling_is_served(&plan, &out, SPELLINGS);
}

#[test]
fn a_registry_selecting_the_withheld_review_in_any_spelling_is_dropped_unnamed_from_the_hq_route() {
    let f = Fixture::new();
    let mut all = vec!["ast-grep"];
    all.extend_from_slice(SPELLINGS);
    f.select(&all);
    let server = Server::start(&f);
    for mode in ["full", "change"] {
        let request =
            json!({"session": "review-t", "action": "review-preview", "recipe": mode}).to_string();
        let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
        assert_eq!(status, 200, "{mode}: {body}");
        let doc: Value = serde_json::from_str(&body).expect("json");
        assert_no_spelling_is_served(&doc, &body, SPELLINGS);
    }
    for route in ["/api/review-projects", "/api/integrations?session=review-t"] {
        let (status, body) = server.request("GET", route, None);
        assert_eq!(status, 200, "{route}: {body}");
        assert!(
            !normalised(&body).contains("pragent"),
            "{route} names the withheld review: {body}"
        );
    }
    // A direct request for the withheld review, in any spelling, is refused
    // exactly as the canonical id is (round 1's refusal, whose sentence is
    // the build's own reason), echoes nothing the request sent, and spawns
    // nothing.
    let canonical =
        json!({"session": "review-t", "action": "run", "recipe": "pr-agent"}).to_string();
    let refused = server.request("POST", "/api/integrations/action", Some(&canonical));
    assert_eq!(refused.0, 400, "{}", refused.1);
    for spelling in SPELLINGS {
        let request =
            json!({"session": "review-t", "action": "run", "recipe": spelling}).to_string();
        let answer = server.request("POST", "/api/integrations/action", Some(&request));
        assert_eq!(
            answer, refused,
            "{spelling:?} is refused as the withheld review"
        );
    }
}

/* ── round 5: an allow-list, not a list of spellings ───────────────────── */

// AI-REVIEW-OUT-OF-LAUNCH-R5. Four rounds normalised spellings of the withheld
// id, and each lost to the next spelling (the round-4 adversary served the
// NFKC forms, small caps, Cyrillic and Greek look-alikes and ASCII names that
// contain it). The rule is now the other way round: in a build without AI
// review an id is named in what is served or printed ONLY when it is one the
// build ships (the same ids the launch UI twin ships). Every other id, in any
// spelling, is reported as a count, without its text.
//
// The tests below try every id and report every id that fails, not the first.

/// What a selection this build does not run becomes: a count, never the text.
/// hq-copy.js `change.review.not_run_here`, held equal to the Rust constants
/// by ui/src/workspace/ai-review-allow-list.test.js.
const SELECTION_ONE: &str =
    "1 registered selection is not one this build runs, so it was left out of this review.";
const SELECTION_MANY: &str =
    "{n} registered selections are not ones this build runs, so they were left out of this review.";
/// What a request for a check this build does not run is answered with.
const CHECK_NOT_RUN: &str = "This build does not run that check.";
/// The sentence a review mode that is neither `full` nor `change` is refused
/// with, in every build (`review.rs` `inventory`). It never echoes the mode.
const MODE_REFUSED: &str = "Choose full or change review";
/// The round-1 direct-request refusal, which stays as ruled. It is a constant
/// sentence that happens to contain the display name of the withheld review, so
/// an echo check reads a text with it taken out.
const NOT_IN_BUILD: &str = "AI review is not provisioned in this build of ThruLine";
const NOT_IN_BUILD_FULL: &str = "AI review is not provisioned in this build of ThruLine: it was built without the ai-review feature, so no PR-Agent review runs";

fn not_run(n: usize) -> String {
    if n == 1 {
        SELECTION_ONE.to_string()
    } else {
        SELECTION_MANY.replace("{n}", &n.to_string())
    }
}

/// Every id round 4 and round 5 attack with, and one that is no spelling of
/// anything.
fn every_id() -> Vec<&'static str> {
    ROUND_4
        .iter()
        .chain(ROUND_5)
        .chain(NOT_A_SPELLING)
        .copied()
        .collect()
}

fn strings_in(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::String(s) => out.push(s.clone()),
        Value::Array(a) => a.iter().for_each(|x| strings_in(x, out)),
        Value::Object(m) => {
            for (k, x) in m {
                out.push(k.clone());
                strings_in(x, out);
            }
        }
        _ => {}
    }
}

/// The id's text appears nowhere in what was served or printed: not in the raw
/// text, and not in any string of a JSON document (so an escaped spelling is
/// found as well as a raw one).
fn not_echoed(id: &str, what: &str, texts: &[&str]) -> Result<(), String> {
    for text in texts {
        let text = text.replace(NOT_IN_BUILD_FULL, "");
        if text.contains(id) {
            return Err(format!("{what}: {id:?} is echoed in the text"));
        }
        if let Ok(doc) = serde_json::from_str::<Value>(&text) {
            let mut all = Vec::new();
            strings_in(&doc, &mut all);
            if all.iter().any(|s| s.contains(id)) {
                return Err(format!("{what}: {id:?} is echoed in a served string"));
            }
        }
    }
    Ok(())
}

fn assert_not_echoed(id: &str, what: &str, texts: &[&str]) {
    if let Err(e) = not_echoed(id, what, texts) {
        panic!("{e}: {texts:?}");
    }
}

/// A plan whose project registered `dropped` selections this build does not
/// run: it counts them in one gap, names none of them, and keeps only the
/// deterministic selection and check.
fn counted_not_named(plan: &Value, raw: &str, dropped: usize, ids: &[&str]) -> Vec<String> {
    let mut failures = Vec::new();
    for id in ids {
        if let Err(e) = not_echoed(id, "the plan", &[raw]) {
            failures.push(e);
        }
    }
    let sentence = not_run(dropped);
    let counted = plan["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .filter(|g| g["reason"].as_str() == Some(sentence.as_str()))
        .count();
    if counted != 1 {
        failures.push(format!("{counted} gaps say {sentence:?}, not one"));
    }
    if plan["gaps"]
        .as_array()
        .expect("gaps")
        .iter()
        .any(|g| g["reason"].as_str() == Some(sentence.as_str()) && g.get("recipe").is_some())
    {
        failures.push("the count gap names a recipe".into());
    }
    if plan["project"]["recipes"] != json!(["ast-grep"]) {
        failures.push(format!(
            "the served project keeps {}, not only the shipped selection",
            plan["project"]["recipes"]
        ));
    }
    let checks: Vec<&str> = plan["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .filter_map(|c| c["recipe"].as_str())
        .collect();
    if checks != ["ast-grep"] {
        failures.push(format!("the deterministic review changed: {checks:?}"));
    }
    failures
}

#[test]
fn round_4_and_5_inputs_are_the_ones_both_builds_are_driven_with() {
    assert_eq!(SPELLINGS, ROUND_4, "one set of round-4 inputs");
    assert_eq!(ROUND_5.len(), 15, "the adversary's classes, all of them");
    let mut seen = std::collections::BTreeSet::new();
    for id in every_id() {
        assert!(seen.insert(id), "{id:?} is listed twice");
    }
}

#[test]
fn a_registry_selection_of_an_id_this_build_does_not_ship_is_counted_never_named_in_the_cli_preview(
) {
    let f = Fixture::new();
    let ids = every_id();
    let mut failures = Vec::new();
    for id in &ids {
        f.select(&["ast-grep", *id]);
        for mode in ["full", "change"] {
            let (code, out, err) = f.review("preview", mode, "");
            if code != 0 {
                failures.push(format!("{id:?} {mode}: exit {code}"));
                continue;
            }
            if let Err(e) = not_echoed(id, "the CLI output", &[&out, &err]) {
                failures.push(e);
            }
            let plan: Value = serde_json::from_str(&out).expect("json");
            failures.extend(
                counted_not_named(&plan, &out, 1, &[*id])
                    .into_iter()
                    .map(|e| format!("{id:?} {mode}: {e}")),
            );
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} ids fail through the CLI preview: {failures:#?}",
        failures.len(),
        ids.len()
    );

    // Every one at once is one gap that counts them all.
    let mut all = vec!["ast-grep"];
    all.extend(&ids);
    f.select(&all);
    let (code, out, err) = f.review("preview", "change", "");
    assert_eq!(code, 0, "{out}{err}");
    let plan: Value = serde_json::from_str(&out).expect("json");
    let failures = counted_not_named(&plan, &out, ids.len(), &ids);
    assert!(failures.is_empty(), "{failures:#?}");
    for id in &ids {
        assert_not_echoed(id, "the CLI output", &[&err]);
    }

    // And what is accepted and read back carries none of them either.
    let digest = plan["digest"].as_str().expect("digest").to_string();
    let (code, out, err) = f.review("accept", "change", &digest);
    assert_eq!(code, 0, "{out}{err}");
    let job: Value = serde_json::from_str(&out).expect("json");
    let queue = job["id"].as_str().expect("job id").to_string();
    let mut texts = vec![out, err];
    for action in ["step", "status", "export"] {
        let (code, out, err) = f.review(action, "change", &queue);
        assert_eq!(code, 0, "{action}: {out}{err}");
        texts.extend([out, err]);
    }
    let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
    let echoed: Vec<String> = ids
        .iter()
        .filter_map(|id| not_echoed(id, "the accepted queue", &texts).err())
        .collect();
    assert!(echoed.is_empty(), "{echoed:#?}");
}

#[test]
fn a_registry_selection_of_an_id_this_build_does_not_ship_is_counted_never_named_by_every_hq_route()
{
    let f = Fixture::new();
    let ids = every_id();
    let mut failures = Vec::new();
    for id in &ids {
        f.select(&["ast-grep", *id]);
        let server = Server::start(&f);
        for mode in ["full", "change"] {
            let request =
                json!({"session": "review-t", "action": "review-preview", "recipe": mode})
                    .to_string();
            let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
            if status != 200 {
                failures.push(format!("{id:?} {mode}: status {status}"));
                continue;
            }
            let plan: Value = serde_json::from_str(&body).expect("json");
            failures.extend(
                counted_not_named(&plan, &body, 1, &[*id])
                    .into_iter()
                    .map(|e| format!("{id:?} {mode}: {e}")),
            );
        }
        for route in ["/api/review-projects", "/api/integrations?session=review-t"] {
            let (status, body) = server.request("GET", route, None);
            if status != 200 {
                failures.push(format!("{id:?} {route}: status {status}"));
            } else if let Err(e) = not_echoed(id, route, &[&body]) {
                failures.push(e);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures through the HQ routes over {} ids: {failures:#?}",
        failures.len(),
        ids.len()
    );

    // And every one at once.
    let mut all = vec!["ast-grep"];
    all.extend(&ids);
    f.select(&all);
    let server = Server::start(&f);
    let request =
        json!({"session": "review-t", "action": "review-preview", "recipe": "full"}).to_string();
    let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
    assert_eq!(status, 200, "{body}");
    let plan: Value = serde_json::from_str(&body).expect("json");
    let failures = counted_not_named(&plan, &body, ids.len(), &ids);
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn a_registry_selection_of_an_id_the_build_ships_is_still_named() {
    let f = Fixture::new();
    // `in-toto` is one of the catalog's sources, so the build ships it, and
    // it has no native guided adapter: the plan still says so, by name.
    f.select(&["ast-grep", "in-toto"]);
    let (code, out, err) = f.review("preview", "full", "");
    assert_eq!(code, 0, "{out}{err}");
    let plan: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(plan["project"]["recipes"], json!(["ast-grep", "in-toto"]));
    let gaps = plan["gaps"].as_array().expect("gaps");
    assert!(
        gaps.iter().any(|g| g["recipe"] == json!("in-toto")
            && g["reason"]
                == json!("No native guided adapter exists for this registered selection")),
        "the shipped id is named in its gap: {out}"
    );
    assert!(
        gaps.iter().all(|g| !g["reason"]
            .as_str()
            .is_some_and(|r| r.contains("this build runs"))),
        "nothing was dropped, so nothing is counted: {out}"
    );

    let server = Server::start(&f);
    let request =
        json!({"session": "review-t", "action": "review-preview", "recipe": "full"}).to_string();
    let (status, body) = server.request("POST", "/api/integrations/action", Some(&request));
    assert_eq!(status, 200, "{body}");
    let plan: Value = serde_json::from_str(&body).expect("json");
    assert_eq!(plan["project"]["recipes"], json!(["ast-grep", "in-toto"]));
}

#[test]
fn a_review_mode_that_is_not_one_the_build_has_is_refused_without_being_echoed_in_the_cli() {
    let f = Fixture::new();
    let mut ids = every_id();
    ids.push("pr-agent");
    for id in &ids {
        for action in ["preview", "accept"] {
            let (code, out, err) = f.review(action, id, "");
            assert_ne!(code, 0, "{action} --mode {id:?}: {out}");
            assert_not_echoed(id, "the CLI output", &[&out, &err]);
            assert!(
                err.contains(MODE_REFUSED),
                "{action} --mode {id:?} is refused in the build's words: {err}"
            );
        }
    }
    // The two modes the build has are unchanged.
    for mode in ["full", "change"] {
        let (code, out, err) = f.review("preview", mode, "");
        assert_eq!(code, 0, "{mode}: {out}{err}");
        let plan: Value = serde_json::from_str(&out).expect("json");
        assert_eq!(plan["mode"], json!(mode));
    }
}

/// Every action the HQ route admits, so an id is tried at every door.
const ROUTE_ACTIONS: &[&str] = &[
    "preview",
    "accept",
    "run",
    "cancel",
    "repair-preview",
    "review-status",
    "review-preview",
    "review-accept",
    "review-step",
    "review-retry",
    "review-export",
    "native-repair-preview",
    "agent-repair-preview",
    "agent-repair-list",
];

#[test]
fn the_hq_route_admits_a_recipe_only_when_the_build_ships_it_and_echoes_none_it_does_not() {
    let f = Fixture::new();
    let server = Server::start(&f);
    let post = |action: &str, recipe: &str| {
        let request =
            json!({"session": "review-t", "action": action, "recipe": recipe, "digest": ""})
                .to_string();
        server.request("POST", "/api/integrations/action", Some(&request))
    };
    let mut failures = Vec::new();
    for action in ROUTE_ACTIONS {
        // The withheld id, and every spelling that reads as it, is refused in
        // the round-1 words, and identically (round 4).
        let canonical = post(action, "pr-agent");
        if canonical.0 != 400 || !canonical.1.contains(NOT_IN_BUILD) {
            failures.push(format!("{action}: pr-agent is answered {canonical:?}"));
        }
        for id in ROUND_4 {
            if post(action, id) != canonical {
                failures.push(format!("{action}: {id:?} is not refused as pr-agent is"));
            }
        }
        // Every other id the build does not ship is refused in its own plain
        // words, without echoing a character of what was asked.
        for id in ROUND_5.iter().chain(NOT_A_SPELLING) {
            let (status, body) = post(action, id);
            let refused = serde_json::from_str::<Value>(&body)
                .is_ok_and(|v| v == json!({"error": CHECK_NOT_RUN}));
            if status != 400 || !refused {
                failures.push(format!("{action}: {id:?} is answered {status} {body}"));
            }
            if let Err(e) = not_echoed(id, action, &[&body]) {
                failures.push(e);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures: {failures:#?}",
        failures.len()
    );
    // A recipe the build ships is not refused by this door.
    for (action, recipe) in [("review-preview", "full"), ("preview", "ast-grep")] {
        let (status, body) = post(action, recipe);
        assert_ne!(status, 400, "{action} {recipe}: {body}");
    }
}

#[test]
fn every_cli_door_answers_an_id_it_does_not_ship_without_echoing_it() {
    let f = Fixture::new();
    let root = f.root.display().to_string();
    let exec = f.exec.display().to_string();
    let executable = "/bin/echo";
    let ids = every_id();

    // A plan file for a shipped pack, re-labelled with each id.
    let (code, plan_text, err) = f.run(&[
        "integration",
        "plan",
        "ast-grep",
        "--root",
        &root,
        "--executable",
        executable,
    ]);
    assert_eq!(code, 0, "{err}");
    let plan: Value = serde_json::from_str(&plan_text).expect("json");
    let plan_file = f.root.join("..").join("relabelled-plan.json");
    let plan_arg = plan_file.display().to_string();

    let mut failures = Vec::new();
    for id in &ids {
        let id = *id;
        let mut relabelled = plan.clone();
        relabelled["pack"] = json!(id);
        std::fs::write(&plan_file, serde_json::to_vec(&relabelled).expect("json")).expect("plan");
        let doors: Vec<(&str, Vec<&str>)> = vec![
            (
                "plan",
                vec![
                    "integration",
                    "plan",
                    id,
                    "--root",
                    &root,
                    "--executable",
                    executable,
                ],
            ),
            (
                "native-repair",
                vec![
                    "integration",
                    "native-repair",
                    "--root",
                    &root,
                    "--action",
                    "preview",
                    "--recipe",
                    id,
                ],
            ),
            (
                "agent-repair",
                vec![
                    "integration",
                    "agent-repair",
                    "--root",
                    &root,
                    "--action",
                    "preview",
                    "--recipe",
                    id,
                ],
            ),
            (
                "workflow preview",
                vec![
                    "integration",
                    "workflow",
                    "--root",
                    &root,
                    "--exec-db",
                    &exec,
                    "--action",
                    "preview",
                    "--recipe",
                    id,
                ],
            ),
            (
                "workflow run",
                vec![
                    "integration",
                    "workflow",
                    "--root",
                    &root,
                    "--exec-db",
                    &exec,
                    "--action",
                    "run",
                    "--recipe",
                    id,
                ],
            ),
            ("decode", vec!["integration", "decode", id, &plan_arg]),
            (
                "run --plan",
                vec!["integration", "run", "--plan", &plan_arg, "--accept"],
            ),
        ];
        for (door, args) in doors {
            let (code, out, err) = f.run(&args);
            if code == 0 {
                failures.push(format!("{door} {id:?} exits 0"));
            }
            if let Err(e) = not_echoed(id, door, &[&out, &err]) {
                failures.push(e);
            }
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

#[test]
fn agent_repair_and_decode_refuse_the_withheld_review_and_write_nothing() {
    let f = Fixture::new();
    let root = f.root.display().to_string();
    // The round-4 verifier's first observation: `agent-repair --recipe
    // pr-agent` exited 0, named the id and wrote an attempt record.
    let mut failures = Vec::new();
    for action in ["preview", "resume", "verify", "apply", "list"] {
        let (code, out, err) = f.run(&[
            "integration",
            "agent-repair",
            "--root",
            &root,
            "--action",
            action,
            "--recipe",
            "pr-agent",
        ]);
        if code == 0 {
            failures.push(format!("agent-repair {action} exits 0: {out}"));
        }
        if !err.contains(NOT_IN_BUILD) {
            failures.push(format!(
                "agent-repair {action} is not refused in the build's words: {err}"
            ));
        }
        if !out.is_empty() {
            failures.push(format!("agent-repair {action} prints on stdout: {out}"));
        }
    }
    if f.db.with_extension("integration-agent-repairs").exists() {
        failures.push("an agent-repair store, or an attempt record, was written".into());
    }
    assert!(failures.is_empty(), "{failures:#?}");

    // The second: `decode pr-agent <report>` decoded a withheld report. The
    // door refuses before it reads the file (which does not exist here).
    let missing = f.root.join("..").join("no-such-report.json");
    let missing = missing.display().to_string();
    let (code, out, err) = f.run(&["integration", "decode", "pr-agent", &missing]);
    assert_ne!(code, 0, "{out}");
    assert!(err.contains(NOT_IN_BUILD), "{err}");
    assert!(out.is_empty(), "{out}");

    // A pack the build ships still decodes.
    let report = f.root.join("..").join("report.json");
    std::fs::write(&report, "[]").expect("report");
    let (code, out, err) = f.run(&[
        "integration",
        "decode",
        "ast-grep",
        &report.display().to_string(),
    ]);
    assert_eq!(code, 0, "{out}{err}");
    let decoded: Value = serde_json::from_str(&out).expect("json");
    assert_eq!(decoded["provenance"], json!("imported"), "{out}");
}

#[test]
fn a_registry_of_the_wrong_shape_does_not_echo_what_it_selected_in_the_cli_or_on_any_route() {
    let f = Fixture::new();
    let server = Server::start(&f);
    let mut ids = every_id();
    ids.push("pr-agent");
    let mut failures = Vec::new();
    for id in &ids {
        // `recipes` is a string, not a list; and an unknown field of that name.
        for (shape, extra) in [
            ("recipes", json!({"recipes": id})),
            ("field", json!({(*id): true})),
        ] {
            f.registry_with(extra);
            let (code, out, err) = f.review("preview", "full", "");
            if code == 0 {
                failures.push(format!("{shape} {id:?}: an invalid registry is accepted"));
            }
            if let Err(e) = not_echoed(id, &format!("{shape}: the CLI output"), &[&out, &err]) {
                failures.push(e);
            }
            // A listing route may drop the connection when it cannot read the
            // registry; what it answers, if it answers, echoes nothing.
            for route in ["/api/review-projects", "/api/integrations?session=review-t"] {
                if let Some((_, body)) = server.try_request("GET", route, None) {
                    if let Err(e) = not_echoed(id, &format!("{shape}: {route}"), &[&body]) {
                        failures.push(e);
                    }
                }
            }
            let request =
                json!({"session": "review-t", "action": "review-preview", "recipe": "full"})
                    .to_string();
            if let Some((_, body)) =
                server.try_request("POST", "/api/integrations/action", Some(&request))
            {
                if let Err(e) = not_echoed(id, &format!("{shape}: review-preview"), &[&body]) {
                    failures.push(e);
                }
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures: {failures:#?}",
        failures.len()
    );
}

#[test]
fn a_tool_registry_of_the_wrong_shape_does_not_echo_what_it_holds_in_the_cli_or_on_any_route() {
    // THRULINE_INTEGRATION_REGISTRY is the other operator file: the pinned
    // runtimes. A field or a value of the wrong kind in it is quoted by serde
    // into the setup text every recipe serves, so an id typed there reached
    // the screen the same way a registered selection did.
    let f = Fixture::new();
    let server = Server::start(&f);
    let echo = thruline_core::chain::sha256_hex(&std::fs::read("/bin/echo").expect("echo"));
    let runtime = |extra: Value| {
        let mut r = json!({"version": "0.45.3", "executable": "/bin/echo", "sha256": echo, "runtime": [], "prefix": []});
        for (k, v) in extra.as_object().expect("an object") {
            r[k] = v.clone();
        }
        r
    };
    let mut ids = every_id();
    ids.push("pr-agent");
    let mut failures = Vec::new();
    for id in &ids {
        for (shape, doc) in [
            (
                "top-level field",
                json!({"schema": "thruline.integration-runtimes.v1", "tools": {}, (*id): 1}),
            ),
            (
                "tool field",
                json!({"schema": "thruline.integration-runtimes.v1",
                       "tools": {"ast-grep": runtime(json!({(*id): 1}))}}),
            ),
            (
                "tool of the wrong kind",
                json!({"schema": "thruline.integration-runtimes.v1", "tools": {(*id): 5}}),
            ),
        ] {
            std::fs::write(&f.registry, serde_json::to_vec(&doc).expect("json")).expect("registry");
            let root = f.root.display().to_string();
            let exec = f.exec.display().to_string();
            let mut texts = Vec::new();
            for args in [
                vec![
                    "integration",
                    "workflow",
                    "--root",
                    &root,
                    "--exec-db",
                    &exec,
                    "--action",
                    "status",
                ],
                vec![
                    "integration",
                    "workflow",
                    "--root",
                    &root,
                    "--exec-db",
                    &exec,
                    "--action",
                    "preview",
                    "--recipe",
                    "ast-grep",
                ],
                vec![
                    "integration",
                    "project",
                    "--root",
                    &root,
                    "--exec-db",
                    &exec,
                ],
            ] {
                let (_, out, err) = f.run(&args);
                texts.extend([out, err]);
            }
            let (_, out, err) = f.review("preview", "full", "");
            texts.extend([out, err]);
            if let Some((_, body)) =
                server.try_request("GET", "/api/integrations?session=review-t", None)
            {
                texts.push(body);
            }
            let request =
                json!({"session": "review-t", "action": "review-preview", "recipe": "full"})
                    .to_string();
            if let Some((_, body)) =
                server.try_request("POST", "/api/integrations/action", Some(&request))
            {
                texts.push(body);
            }
            let texts: Vec<&str> = texts.iter().map(String::as_str).collect();
            if let Err(e) = not_echoed(id, &format!("{shape}: the tool registry"), &texts) {
                failures.push(e);
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} failures: {failures:#?}",
        failures.len()
    );
}

/* ── round 6: integration evaluate ─────────────────────────────────────── */

// AI-REVIEW-OUT-OF-LAUNCH-R6. The round-5 adversary ran
// `integration evaluate --cases C --results R` on the default build with
// operator-supplied packs, and it exited 0 and printed them: as `by_adapter`
// keys and as each case's `pack`. The only guard was the manifest hash, which
// the operator computes over their own file. Round 5's grep of "every door that
// echoes an id" had missed it. The door census
// (`integration_door_census.rs`, over `tests/support/integration_doors.json`)
// is what keeps a door from being missed again; this is the repro it was found
// with, at the command line the adversary used.

/// Write an evaluation's two files, each case `(id, pack, label, actual)`,
/// into the fixture's `inputs/` (the operator's own; the scan for what a door
/// wrote leaves it alone), and return their two paths.
fn evaluation_files(
    f: &Fixture,
    labels: &[(&str, &str, &str)],
    predicted: &[(&str, &str, &str)],
) -> (String, String) {
    let dir = f._temp.path().join("inputs");
    std::fs::create_dir_all(&dir).expect("inputs");
    let cases = json!({
        "schema": "thruline.integration-qualification-cases.v2",
        "cases": labels.iter().map(|(id, pack, label)| json!({"id": id, "pack": pack, "label": label})).collect::<Vec<_>>(),
    })
    .to_string();
    let cases_path = dir.join("cases.json");
    std::fs::write(&cases_path, &cases).expect("cases");
    let results = json!({
        "schema": "thruline.upstream-qualification.v1",
        "manifest_sha256": thruline_core::chain::sha256_hex(cases.as_bytes()),
        "cases": predicted.iter().map(|(id, pack, actual)| json!({"id": id, "pack": pack, "actual": actual})).collect::<Vec<_>>(),
    })
    .to_string();
    let results_path = dir.join("results.json");
    std::fs::write(&results_path, results).expect("results");
    (
        cases_path.display().to_string(),
        results_path.display().to_string(),
    )
}

#[test]
fn evaluate_prints_no_pack_the_build_does_not_ship_in_either_file() {
    let f = Fixture::new();
    let mut failures = Vec::new();
    let mut check = |what: &str,
                     ids: &[&str],
                     labels: &[(&str, &str, &str)],
                     predicted: &[(&str, &str, &str)],
                     sentence: &str| {
        let (cases, results) = evaluation_files(&f, labels, predicted);
        let (code, out, err) = f.run(&[
            "integration",
            "evaluate",
            "--cases",
            &cases,
            "--results",
            &results,
        ]);
        if code == 0 {
            failures.push(format!("{what}: exits 0 and prints an evaluation: {out}"));
        }
        if !out.is_empty() {
            failures.push(format!("{what}: prints on stdout: {out}"));
        }
        if !err.contains(sentence) {
            failures.push(format!("{what}: is not refused in {sentence:?}: {err}"));
        }
        for id in ids {
            if let Err(e) = not_echoed(id, what, &[&out, &err]) {
                failures.push(e);
            }
            let written = f.written_hits(id, &[NOT_IN_BUILD_FULL]);
            if written != 0 {
                failures.push(format!("{what}: {id:?} is in {written} written place(s)"));
            }
        }
    };

    // The adversary's own files: three packs, in the cases and in the results.
    // The withheld review's sentence leads, as it does at every other door.
    check(
        "the adversary's three packs, in both files",
        &["qodo-merge", "coderabbit9", "PR-Agent"],
        &[
            ("c1", "qodo-merge", "clean"),
            ("c2", "coderabbit9", "failure"),
            ("c3", "PR-Agent", "clean"),
        ],
        &[
            ("c1", "qodo-merge", "passed"),
            ("c2", "coderabbit9", "failed"),
            ("c3", "PR-Agent", "passed"),
        ],
        NOT_IN_BUILD,
    );
    // Each id alone, in both files: the withheld review in the round-1
    // sentence, every other pack in the words a request for a check this build
    // does not run is always answered in.
    for id in every_id() {
        let sentence = if normalised(id) == "pragent" {
            NOT_IN_BUILD
        } else {
            CHECK_NOT_RUN
        };
        check(
            &format!("{id:?} in both files"),
            &[id],
            &[("c1", id, "clean")],
            &[("c1", id, "passed")],
            sentence,
        );
        // Beside a pack the build ships, the evaluation is still not made.
        check(
            &format!("{id:?} beside a shipped pack"),
            &[id],
            &[("c1", "ast-grep", "clean"), ("c2", id, "failure")],
            &[("c1", "ast-grep", "passed"), ("c2", id, "failed")],
            sentence,
        );
        // Only in the predictions.
        check(
            &format!("{id:?} only in the results"),
            &[id],
            &[("c1", "ast-grep", "clean")],
            &[("c1", "ast-grep", "passed"), ("c2", id, "failed")],
            sentence,
        );
    }
    assert!(
        failures.is_empty(),
        "{} failures: {failures:#?}",
        failures.len()
    );
}
