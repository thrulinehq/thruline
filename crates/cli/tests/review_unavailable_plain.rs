//! T-4-1a-FIX (founder ruling R-2026-09-28-5 (a)) — when no review model is
//! reachable, every review door says plainly that review findings aren't
//! available, and why. Never an empty "no findings", never an invented one.
//!
//! **Why this file exists.** The T-4-1a verifier drove every door on a machine
//! with nothing on the review model's port and no review tool registered, and
//! every door refused honestly but in engine words ("Change-bound review has
//! no PR-Agent plan", "An empty or unavailable plan cannot be accepted as a
//! review", "Invalid review identifier"), and no surface said that review
//! findings were unavailable at all. Each door is driven here through the
//! compiled binary, with the review tool's registry pointed at a file that
//! does not exist, so the "not set up" answer is the machine's real state and
//! not a stub. The "model isn't answering" branch needs a registered tool and
//! a silent port; it is pinned by the unit tests beside the probe, through an
//! injected fake, because no test may depend on what listens on a real port.
//!
//! Every fixture is synthetic. No string here came off a real machine.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// The two sentences, word for word. `hq-copy.js` carries the same two, and
/// `review-unavailable.test.js` holds the two files to each other.
const TOOL_NOT_SET_UP: &str =
    "Review findings aren't available: the review tool isn't set up on this machine.";

/// What no surface may say about a review that could not happen.
const NEVER: &[&str] = &[
    "no findings",
    "0 findings",
    "found nothing",
    "raised nothing",
    "Complete",
];

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Every run points the review tool's registry at a path that does not exist:
/// the review tool is not set up on this machine, for real.
fn run(home: &Path, cwd: &Path, db: &Path, args: &[&str]) -> Run {
    let out = Command::new(bin())
        .env("HOME", home)
        .env("THRULINE_HOME", home.join(".thruline"))
        .env(
            "THRULINE_INTEGRATION_REGISTRY",
            home.join("no-such-registry.json"),
        )
        .env_remove("THRULINE_REVIEW_PROJECTS")
        .current_dir(cwd)
        .arg("--db")
        .arg(db)
        .args(args)
        .output()
        .expect("run thruline");
    Run {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

fn git(cwd: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@t")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?}: {out:?}");
}

/// A repository with one commit, a store whose one project points at it, and
/// one recorded session that edited a file in it.
fn fixture(base: &Path) -> (PathBuf, PathBuf) {
    let repo = base.join("repo");
    std::fs::create_dir_all(repo.join("src/upload")).expect("mkdir");
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("src/upload/retry.rs"), "// retry\n").expect("write");
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "seed"]);
    std::fs::write(
        repo.join("s.jsonl"),
        format!("{{\"type\":\"user\",\"cwd\":\"{}\"}}\n", repo.display()),
    )
    .expect("transcript");

    let db = base.join("ledger.db");
    let ledger = thruline_core::ledger::Ledger::open(&db).expect("open");
    ledger
        .conn()
        .execute(
            "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES('cccccccc-0000-4000-8000-00000000p001', ?1, 'claude_code', 'Synthetic',
                    '2026-01-01T00:00:00Z')",
            rusqlite::params![repo.display().to_string()],
        )
        .expect("project");
    ledger
        .conn()
        .execute(
            "INSERT INTO sessions(id, project_id, file_path, started_at)
             VALUES('cccccccc-0000-4000-8000-00000000s001',
                    'cccccccc-0000-4000-8000-00000000p001', ?1, '2026-01-02T09:30:00Z')",
            rusqlite::params![repo.join("s.jsonl").display().to_string()],
        )
        .expect("session");
    ledger
        .conn()
        .execute(
            "INSERT INTO events(id, project_id, session_id, seq, ts, kind, tool_name,
                 tool_input, raw_file, raw_line, raw_sha256, prev_hash, chain_hash)
             VALUES('cccccccc-ev-1','cccccccc-0000-4000-8000-00000000p001',
                    'cccccccc-0000-4000-8000-00000000s001',1,'2026-01-02T09:31:00Z',
                    'tool_use','Edit', ?1, ?2, 1, 'x', '', '')",
            rusqlite::params![
                format!(
                    "{{\"file_path\":\"{}\"}}",
                    repo.join("src/upload/retry.rs").display()
                ),
                repo.join("s.jsonl").display().to_string()
            ],
        )
        .expect("event");
    (repo, db)
}

/// `change new` → `associate --confirm` → `accept --all` → `prepare`, through
/// the binary, exactly as `change_exceptions_cli.rs` does it.
fn an_accepted_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Retry failed uploads",
            "--purpose",
            "The build stays clean on the packages that changed.",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    let change = opened
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", opened.stdout))
        .to_string();
    for args in [
        vec![
            "change",
            "associate",
            &change,
            "cccccccc-0000-4000-8000-00000000s001",
            "--confirm",
            "--actor",
            "sam",
            "--reason",
            "this is the retry work",
        ],
        vec![
            "change",
            "accept",
            &change,
            "--all",
            "--actor",
            "sam",
            "--rationale",
            "this is what I want checked",
        ],
        vec!["change", "prepare", &change],
    ] {
        let out = run(home, repo, db, &args);
        assert_eq!(out.code, 0, "{args:?}: {}{}", out.stdout, out.stderr);
    }
    change
}

/// A Change that has been opened and nothing else: no policy, so no review.
fn a_new_change(home: &Path, repo: &Path, db: &Path) -> String {
    let opened = run(
        home,
        repo,
        db,
        &[
            "change",
            "new",
            "--title",
            "Unrelated",
            "--requester",
            "sam",
        ],
    );
    assert_eq!(opened.code, 0, "{}", opened.stderr);
    opened
        .stdout
        .lines()
        .map(str::trim)
        .find(|l| l.starts_with("chg_"))
        .unwrap_or_else(|| panic!("no Change id was printed: {}", opened.stdout))
        .to_string()
}

/// Accept a second scope version whose check profile records that this Change
/// needs a review. The one hand-written row: the policy has no editor yet.
fn expecting_a_review(db: &Path, change: &str) {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    let (criteria, exclusions, allowed, constraints): (String, String, String, String) = ledger
        .conn()
        .query_row(
            "SELECT criteria_json, exclusions_json, allowed_scope_json, constraints_json
               FROM change_scope_revisions WHERE change_id=?1 ORDER BY scope_version DESC LIMIT 1",
            rusqlite::params![change],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("the accepted revision exists");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_revisions(change_id, scope_version, proposed_at,
                 criteria_json, exclusions_json, allowed_scope_json, constraints_json,
                 check_profile_json, content_sha256)
             VALUES(?1, 2, '2026-01-03T09:00:00Z', ?2, ?3, ?4, ?5,
                    '[\"policy:review-required\"]', 'x')",
            rusqlite::params![change, criteria, exclusions, allowed, constraints],
        )
        .expect("a second scope version");
    ledger
        .conn()
        .execute(
            "INSERT INTO change_scope_acceptances(change_id, scope_version, accepted_by,
                                                  accepted_at, rationale)
             VALUES(?1, 2, 'sam', '2026-01-03T09:05:00Z', 'this needs a review')",
            rusqlite::params![change],
        )
        .expect("accept it");
}

fn review_rows(db: &Path) -> i64 {
    let ledger = thruline_core::ledger::Ledger::open(db).expect("open");
    ledger
        .conn()
        .query_row("SELECT COUNT(*) FROM change_reviews", [], |r| r.get(0))
        .expect("count")
}

/// The refusal's first line (the one `error: ` opens) is the plain sentence,
/// and the engine's own words are still there, one level down. Lines a child
/// process printed before the refusal are not the refusal, so they are skipped.
fn assert_plain_first(stderr: &str, technical: &str) {
    let mut lines = stderr.lines().skip_while(|l| !l.starts_with("error: "));
    let first = lines.next().unwrap_or_default();
    assert_eq!(
        first.strip_prefix("error: ").unwrap_or(first),
        TOOL_NOT_SET_UP,
        "the refusal leads with the plain sentence: {stderr}"
    );
    let rest: String = lines.collect::<Vec<_>>().join("\n");
    assert!(
        rest.contains(technical),
        "the technical detail stays, one level down: {stderr}"
    );
    for word in NEVER {
        assert!(!stderr.contains(word), "{word:?} in {stderr}");
    }
}

/* ── the terminal doors: preview, accept, step ─────────────────────────── */

// AI-REVIEW-OUT-OF-LAUNCH (R-2026-09-28-20): this pins the ai-review build,
// where the deterministic review's preview and plain accept also speak for the
// PR-Agent route. In the launch build they are the deterministic review only
// and say nothing about review findings; `ai_review_out_of_launch.rs` pins
// that. Run with `--features ai-review`.
#[cfg(feature = "ai-review")]
#[test]
fn preview_says_review_findings_are_unavailable_and_why() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let exec = base.path().join("exec.db");
    let root = repo.display().to_string();
    let exec_arg = exec.display().to_string();

    let out = run(
        &home,
        &repo,
        &db,
        &[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec_arg,
            "--action",
            "preview",
        ],
    );
    assert_eq!(out.code, 0, "{}{}", out.stdout, out.stderr);
    let doc: serde_json::Value = serde_json::from_str(&out.stdout).expect("json");
    let said = &doc["review_findings"];
    assert_eq!(said["available"], serde_json::json!(false), "{said}");
    assert_eq!(said["why"], serde_json::json!("tool_not_set_up"));
    assert_eq!(said["sentence"], serde_json::json!(TOOL_NOT_SET_UP));
    assert!(
        said["technical"]
            .as_str()
            .is_some_and(|t| t.contains("not provisioned")),
        "the engine's reason is kept, one level down: {said}"
    );
    assert!(
        out.stderr.lines().any(|l| l == TOOL_NOT_SET_UP),
        "a person at the terminal reads it too: {}",
        out.stderr
    );
    for word in NEVER {
        assert!(!said.to_string().contains(word), "{word:?}");
    }
}

#[test]
fn accept_bound_to_a_change_refuses_in_plain_words_and_records_nothing() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);
    let exec = base.path().join("exec.db");
    let root = repo.display().to_string();
    let exec_arg = exec.display().to_string();

    let out = run(
        &home,
        &repo,
        &db,
        &[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec_arg,
            "--action",
            "accept",
            "--change-id",
            &change,
            "--examined-inputs-id",
            "exi_synthetic",
        ],
    );
    assert_ne!(out.code, 0, "an unavailable review is never accepted");
    assert_plain_first(&out.stderr, "Change-bound review has no PR-Agent plan");
    assert_eq!(review_rows(&db), 0, "nothing is invented");
}

// AI-REVIEW-OUT-OF-LAUNCH (R-2026-09-28-20): this pins the ai-review build,
// where the deterministic review's preview and plain accept also speak for the
// PR-Agent route. In the launch build they are the deterministic review only
// and say nothing about review findings; `ai_review_out_of_launch.rs` pins
// that. Run with `--features ai-review`.
#[cfg(feature = "ai-review")]
#[test]
fn accept_of_an_empty_plan_refuses_in_plain_words() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let exec = base.path().join("exec.db");
    let root = repo.display().to_string();
    let exec_arg = exec.display().to_string();
    let preview = run(
        &home,
        &repo,
        &db,
        &[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec_arg,
            "--action",
            "preview",
        ],
    );
    assert_eq!(preview.code, 0, "{}", preview.stderr);
    let doc: serde_json::Value = serde_json::from_str(&preview.stdout).expect("json");
    let digest = doc["digest"].as_str().expect("digest").to_string();

    let out = run(
        &home,
        &repo,
        &db,
        &[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec_arg,
            "--action",
            "accept",
            "--digest",
            &digest,
        ],
    );
    assert_ne!(out.code, 0);
    assert_plain_first(
        &out.stderr,
        "An empty or unavailable plan cannot be accepted as a review",
    );
}

#[test]
fn step_refuses_in_plain_words() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let exec = base.path().join("exec.db");
    let root = repo.display().to_string();
    let exec_arg = exec.display().to_string();

    let out = run(
        &home,
        &repo,
        &db,
        &[
            "integration",
            "review",
            "--root",
            &root,
            "--exec-db",
            &exec_arg,
            "--action",
            "step",
        ],
    );
    assert_ne!(out.code, 0);
    assert_plain_first(&out.stderr, "Invalid review identifier");
}

/* ── the served doors ──────────────────────────────────────────────────── */

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(home: &Path, repo: &Path, db: &Path, projects: Option<&Path>) -> Self {
        let mut command = Command::new(bin());
        command
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env(
                "THRULINE_INTEGRATION_REGISTRY",
                home.join("no-such-registry.json"),
            )
            .env_remove("THRULINE_REVIEW_PROJECTS");
        if let Some(projects) = projects {
            command.env("THRULINE_REVIEW_PROJECTS", projects);
        }
        let mut child = command
            .current_dir(repo)
            .arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .unwrap_or_default()
                            .parse()
                            .expect("port in the printed URL");
                        return Self { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("the server never printed its address");
                }
            }
        }
    }

    fn request(&self, verb: &str, path: &str, body: Option<&str>) -> (u16, String) {
        let mut stream =
            TcpStream::connect(("127.0.0.1", self.port)).expect("connect to the server");
        let head = match body {
            Some(b) => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}",
                self.port,
                self.port,
                b.len()
            ),
            None => format!(
                "{verb} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n",
                self.port
            ),
        };
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
        (
            status,
            String::from_utf8_lossy(&raw[split + 4..]).to_string(),
        )
    }
}

#[test]
fn where_the_policy_expects_a_review_the_served_readiness_says_why_there_is_none() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let change = an_accepted_change(&home, &repo, &db);
    let plain = a_new_change(&home, &repo, &db);
    expecting_a_review(&db, &change);

    let server = Server::start(&home, &repo, &db, None);
    for route in ["coverage", "readiness"] {
        let (status, body) = server.request("GET", &format!("/api/changes/{change}/{route}"), None);
        assert_eq!(status, 200, "{body}");
        let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
        assert!(
            doc["readiness"]["obligations"]
                .as_array()
                .is_some_and(|o| o.iter().any(|o| o["id"] == serde_json::json!("review"))),
            "the accepted policy's review obligation is named: {body}"
        );
        let said = &doc["review_findings"];
        assert_eq!(
            said["available"],
            serde_json::json!(false),
            "{route}: {body}"
        );
        assert_eq!(said["why"], serde_json::json!("tool_not_set_up"), "{route}");
        assert_eq!(
            said["sentence"],
            serde_json::json!(TOOL_NOT_SET_UP),
            "{route}"
        );
        for word in NEVER {
            assert!(!said.to_string().contains(word), "{route}: {word:?}");
        }

        // A Change whose policy expects no review is not told about one.
        let (_, other) = server.request("GET", &format!("/api/changes/{plain}/{route}"), None);
        let other: serde_json::Value = serde_json::from_str(&other).expect("json");
        assert_eq!(other["review_findings"], serde_json::Value::Null, "{route}");
    }
    assert_eq!(review_rows(&db), 0, "reading the answer invents no review");
}

// AI-REVIEW-OUT-OF-LAUNCH (R-2026-09-28-20): this pins the ai-review build,
// where the deterministic review's preview and plain accept also speak for the
// PR-Agent route. In the launch build they are the deterministic review only
// and say nothing about review findings; `ai_review_out_of_launch.rs` pins
// that. Run with `--features ai-review`.
#[cfg(feature = "ai-review")]
#[test]
fn the_served_review_preview_and_step_say_it_plainly_too() {
    let base = tempfile::tempdir().expect("tempdir");
    let home = base.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    let (repo, db) = fixture(base.path());
    let exec = base.path().join("exec.db");
    let projects = base.path().join("projects.json");
    std::fs::write(
        &projects,
        serde_json::to_vec(&serde_json::json!({
            "schema": "thruline.review-projects.v1",
            "projects": [{"id": "t", "label": "Synthetic", "root": repo, "exec_db": exec}],
        }))
        .expect("json"),
    )
    .expect("projects");

    let server = Server::start(&home, &repo, &db, Some(&projects));
    let (status, body) = server.request(
        "POST",
        "/api/integrations/action",
        Some(r#"{"session":"review-t","action":"review-preview","recipe":"full"}"#),
    );
    assert_eq!(status, 200, "{body}");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
    assert_eq!(
        doc["review_findings"]["sentence"],
        serde_json::json!(TOOL_NOT_SET_UP),
        "{body}"
    );

    let (status, body) = server.request(
        "POST",
        "/api/integrations/action",
        Some(r#"{"session":"review-t","action":"review-step","recipe":"full","digest":""}"#),
    );
    assert_eq!(status, 409, "{body}");
    let doc: serde_json::Value = serde_json::from_str(&body).expect("json");
    let error = doc["error"].as_str().expect("error text");
    assert!(
        error
            .strip_prefix("error: ")
            .unwrap_or(error)
            .starts_with(TOOL_NOT_SET_UP),
        "the served refusal leads with the plain sentence: {error}"
    );
    for word in NEVER {
        assert!(!error.contains(word), "{word:?} in {error}");
    }
}
