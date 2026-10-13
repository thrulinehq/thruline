//! P6-5 — the run route, through the door a person's browser uses.
//!
//! One test per security guard (amendment §Security), each written so that
//! removing the guard turns it red; the double-click rule; and the outcomes
//! read back from REAL processes the wrapper ran — a pass, a failure, a run
//! that selected nothing, a program that is not there.
//!
//! The project is a temporary git repository and the store is a temporary
//! ledger. Nothing here touches a real store or a real project.

use super::*;
use crate::check::RunnableCheck;
use crate::uiserve::run_route::{self, RunRoute, PERMISSION_FILE};

/// JavaScript's `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )`.
fn encode_uri_component(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' => (b as char).to_string(),
            b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

#[test]
fn a_run_key_survives_the_screens_address_encoding_unchanged() {
    for _ in 0..50 {
        let key = run_route::mint_run_key();
        assert_eq!(encode_uri_component(&key), key, "{key}");
    }
}

/* ── the fixtures ──────────────────────────────────────────────────────── */

/// A real git repository with one commit, so the wrapper can observe it.
fn a_project(dir: &Path) -> PathBuf {
    let root = dir.join("project");
    std::fs::create_dir_all(&root).expect("project dir");
    std::fs::write(root.join("README.md"), "synthetic project\n").expect("file");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "p65@example.invalid"],
        vec!["config", "user.name", "p65"],
        vec!["add", "."],
        vec!["commit", "-q", "-m", "one"],
    ] {
        let ok = std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs")
            .success();
        assert!(ok, "git {args:?}");
    }
    root.canonicalize().expect("canonical root")
}

/// A Change whose accepted profile names `profile`, bound to a real folder.
fn a_change_with_profile(dir: &Path, profile: &[&str]) -> (String, PathBuf) {
    let id = a_store_with_one_change(dir);
    let root = a_project(dir);
    let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
    let version = thruline_core::change::append_scope(
        &ledger,
        &id,
        "2026-01-02T09:30:00Z",
        &thruline_core::change::AcceptedScope {
            criteria: vec!["The tests pass.".to_string()],
            check_profile: profile.iter().map(|s| s.to_string()).collect(),
            ..Default::default()
        },
    )
    .expect("scope");
    thruline_core::change::accept_scope(
        &ledger,
        &id,
        version,
        &thruline_core::change::ScopeAcceptance {
            actor: "sam".into(),
            accepted_at: "2026-01-02T09:31:00Z".into(),
            rationale: String::new(),
        },
    )
    .expect("acceptance");
    thruline_core::examined_inputs::bind_intended_at(
        &ledger,
        &id,
        "2026-01-02T09:32:00Z",
        Some(&root),
    )
    .expect("bind")
    .expect("a record");
    (id, root)
}

/// A state whose run route starts `catalogue` in place of the allow-list —
/// test-only, so a guard can be proven against a fast, harmless program.
fn state_with(dir: &Path, catalogue: Option<&'static [RunnableCheck]>) -> Arc<State> {
    let base = test_state(dir, MockEngine::available());
    let state = Arc::try_unwrap(base).unwrap_or_else(|_| panic!("the state is not shared yet"));
    Arc::new(State {
        run_route: match catalogue {
            Some(c) => RunRoute::with_catalogue(c),
            None => RunRoute::new(),
        },
        ..state
    })
}

fn token_of(state: &State) -> String {
    state.run_route.token().expect("a token").to_string()
}

fn writer(state: &State) -> String {
    format!(
        "Origin: http://{ORIGIN}\r\n{}: {}\r\n",
        run_route::TOKEN_HEADER,
        token_of(state)
    )
}

fn post(state: &Arc<State>, path: &str, body: &str, headers: &str) -> String {
    exchange(
        Arc::clone(state),
        &framed("POST", path, body.as_bytes(), headers),
    )
}

fn get(state: &Arc<State>, path: &str, headers: &str) -> String {
    exchange(
        Arc::clone(state),
        format!("GET {path} HTTP/1.1\r\nHost: {ORIGIN}\r\n{headers}\r\n").as_bytes(),
    )
}

fn grant(state: &Arc<State>, id: &str) {
    let response = post(
        state,
        &format!("/api/changes/{id}/run/permission"),
        "{}",
        &writer(state),
    );
    assert_eq!(status(&response), 200, "{response}");
}

/// Wait for the route's job table to empty, bounded.
fn wait_idle(state: &Arc<State>, id: &str) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    loop {
        let view = response_json(&get(state, &format!("/api/changes/{id}/run"), ""));
        if view["job"].is_null() {
            return view;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run did not end: {view}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn executions(dir: &Path, id: &str) -> Vec<thruline_core::run_execution::ExecutionRecord> {
    let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
    thruline_core::run_execution::all(&ledger, Some(id)).expect("executions")
}

static TRUE: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "true",
    args: &[],
}];

/* ── guard 1: loopback only ────────────────────────────────────────────── */

/// The server binds the IPv4 loopback and nothing else, and a connection from
/// anything but the loopback is refused before a byte is read.
#[test]
fn guard_1_the_server_binds_the_loopback_and_nothing_else() {
    assert_eq!(
        resolve_bind_address(None).expect("default"),
        std::net::Ipv4Addr::LOCALHOST
    );
    for foreign in ["0.0.0.0", "192.168.1.10", "10.0.0.1", "::", "example.com"] {
        assert!(
            resolve_bind_address(Some(foreign)).is_err(),
            "{foreign} would put the run route on the network"
        );
    }
    assert!(peer_is_allowed("127.0.0.1:5000".parse().expect("addr")));
    assert!(!peer_is_allowed("192.168.1.10:5000".parse().expect("addr")));
    assert!(!peer_is_allowed("0.0.0.0:5000".parse().expect("addr")));
}

/* ── guard 2: same origin and a per-launch token ───────────────────────── */

#[test]
fn guard_2_a_run_needs_this_pages_origin_and_this_launchs_token() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let run = format!("/api/changes/{id}/run");
    let body = r#"{"check":"cargo-test"}"#;
    let token = token_of(&state);
    // Permission is given first, so guard 2 is the ONLY thing between each
    // request below and a real run: remove it and these start runs.
    grant(&state, &id);

    // No Origin at all: a request no browser page of ours would send. (The
    // server-wide check lets an absent Origin through; this guard does not.)
    let r = post(
        &state,
        &run,
        body,
        &format!("{}: {token}\r\n", run_route::TOKEN_HEADER),
    );
    assert_eq!(status(&r), 403, "no origin: {r}");
    assert_eq!(response_json(&r)["error"], "same_origin_required", "{r}");
    // A foreign page.
    let r = post(
        &state,
        &run,
        body,
        &format!(
            "Origin: https://evil.example\r\n{}: {token}\r\n",
            run_route::TOKEN_HEADER
        ),
    );
    assert_eq!(status(&r), 403, "foreign origin: {r}");
    // Our origin, no token.
    let r = post(&state, &run, body, &format!("Origin: http://{ORIGIN}\r\n"));
    assert_eq!(status(&r), 403, "no token: {r}");
    assert_eq!(response_json(&r)["error"], "run_token_required");
    // Our origin, a wrong token.
    let r = post(
        &state,
        &run,
        body,
        &format!(
            "Origin: http://{ORIGIN}\r\n{}: {}\r\n",
            run_route::TOKEN_HEADER,
            "0".repeat(64)
        ),
    );
    assert_eq!(status(&r), 403, "wrong token: {r}");
    assert_eq!(response_json(&r)["error"], "run_token_required");
    // The same refusals hold on the permission address.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run/permission"),
        "{}",
        &format!("Origin: http://{ORIGIN}\r\n"),
    );
    assert_eq!(status(&r), 403, "permission without a token: {r}");
    assert_eq!(response_json(&r)["error"], "run_token_required", "{r}");
    let r = post(
        &state,
        &format!("/api/changes/{id}/run/permission"),
        "{}",
        &format!("{}: {token}\r\n", run_route::TOKEN_HEADER),
    );
    assert_eq!(response_json(&r)["error"], "same_origin_required", "{r}");
    // Nothing reached the store.
    assert!(executions(temp.path(), &id).is_empty());

    // And with both, the run starts.
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 202, "{r}");
    wait_idle(&state, &id);
    assert_eq!(executions(temp.path(), &id).len(), 1);
}

#[test]
fn guard_2_the_token_is_served_to_this_page_only_and_changes_every_launch() {
    let temp = fixture_dir();
    let state = state_with(temp.path(), None);
    let r = get(&state, "/api/run/token", "Sec-Fetch-Site: cross-site\r\n");
    assert_eq!(status(&r), 403, "a foreign page read the token: {r}");
    let r = get(&state, "/api/run/token", "Sec-Fetch-Site: same-site\r\n");
    assert_eq!(status(&r), 403, "a sibling port read the token: {r}");
    let r = get(&state, "/api/run/token", "Origin: https://evil.example\r\n");
    assert_eq!(status(&r), 403, "a foreign origin read the token: {r}");
    let r = get(&state, "/api/run/token", "Sec-Fetch-Site: same-origin\r\n");
    assert_eq!(status(&r), 200, "{r}");
    assert!(
        !r.to_ascii_lowercase()
            .contains("access-control-allow-origin"),
        "the token answer must not be readable cross-origin"
    );
    let served = response_json(&r)["token"]
        .as_str()
        .expect("a token")
        .to_string();
    assert_eq!(served, token_of(&state));
    assert_eq!(served.len(), 64);
    assert!(served.bytes().all(|b| b.is_ascii_hexdigit()));
    // A new launch, a new token.
    let a = run_route::mint_token().expect("a");
    let b = run_route::mint_token().expect("b");
    assert_ne!(a, b);
    assert_ne!(a, served);
}

/* ── guard 3: the command is resolved server-side ──────────────────────── */

#[test]
fn guard_3_a_request_carrying_a_command_arguments_a_path_or_variables_is_refused_whole() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    grant(&state, &id);
    let run = format!("/api/changes/{id}/run");
    let marker = temp.path().join("pwned");
    for body in [
        format!(
            r#"{{"check":"cargo-test","command":"touch {}"}}"#,
            marker.display()
        ),
        r#"{"check":"cargo-test","args":["--","--ignored"]}"#.to_string(),
        r#"{"check":"cargo-test","argv":["sh","-c","id"]}"#.to_string(),
        r#"{"check":"cargo-test","path":"/tmp"}"#.to_string(),
        r#"{"check":"cargo-test","cwd":"/"}"#.to_string(),
        r#"{"check":"cargo-test","env":{"PATH":"/tmp"}}"#.to_string(),
        format!(r#"{{"command":"touch {}"}}"#, marker.display()),
    ] {
        let r = post(&state, &run, &body, &writer(&state));
        assert_eq!(status(&r), 400, "{body} was accepted: {r}");
        assert_eq!(response_json(&r)["error"], "check_id_only", "{body}");
    }
    // An environment is a closed-set id, never a folder.
    let r = post(
        &state,
        &run,
        r#"{"check":"cargo-test","environment":"/usr/bin"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 400, "{r}");
    assert_eq!(response_json(&r)["error"], "unknown_environment");
    assert!(!marker.exists());
    assert!(
        executions(temp.path(), &id).is_empty(),
        "nothing may run for a refused request"
    );
    // The argv that WOULD run is the table's, whatever was sent.
    let real = crate::check::runnable_check("cargo-test").expect("allow-listed");
    assert_eq!(real.argv(), vec!["cargo".to_string(), "test".to_string()]);
    let _ = root;
}

/* ── guard 4: allow-listed, in the profile, nothing installed ──────────── */

#[test]
fn guard_4_only_an_allow_listed_check_from_the_accepted_profile_can_start() {
    // In the allow-list, NOT in this Change's accepted profile.
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["clippy"]);
    let state = state_with(temp.path(), Some(TRUE));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(response_json(&r)["error"], "not_in_check_profile");
    assert!(executions(temp.path(), &id).is_empty());

    // In the profile, NOT in the allow-list: there is no argv for it at all.
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["megalinter", "cargo-test"]);
    let state = state_with(temp.path(), None);
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"megalinter"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(response_json(&r)["error"], "not_runnable_from_here");
    assert!(executions(temp.path(), &id).is_empty());
}

/// The shipped allow-list itself: no installer, no elevation, no shell, no
/// script out of the repository, no network fetcher.
#[test]
fn guard_4_the_allow_list_runs_no_installer_shell_elevation_or_repository_script() {
    const NEVER: &[&str] = &[
        "sh",
        "bash",
        "zsh",
        "fish",
        "dash",
        "env",
        "sudo",
        "su",
        "doas",
        "pkexec",
        "curl",
        "wget",
        "brew",
        "apt",
        "apt-get",
        "yum",
        "dnf",
        "pip",
        "pip3",
        "pipx",
        "npm",
        "npx",
        "pnpm",
        "yarn",
        "make",
        "rustup",
        "cargo-install",
        "python",
        "python3",
        "node",
    ];
    assert!(!crate::check::RUNNABLE_CHECKS.is_empty());
    for c in crate::check::RUNNABLE_CHECKS {
        assert!(!NEVER.contains(&c.program), "{} runs {}", c.id, c.program);
        assert!(
            !c.program.contains('/') && !c.program.starts_with('.'),
            "{} names a path, not a program: {}",
            c.id,
            c.program
        );
        for arg in c.args {
            assert!(
                !["install", "add", "-c", "--eval", "exec", "run-script"].contains(arg)
                    && !arg.contains('/'),
                "{} passes {arg:?}",
                c.id
            );
        }
        assert!(
            thruline_core::change_propose::profile_check_category(c.id).is_some(),
            "{} is not a check the profile table knows",
            c.id
        );
    }
}

/* ── guard 5: no permission, no run ────────────────────────────────────── */

#[test]
fn guard_5_without_this_projects_permission_nothing_runs() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let run = format!("/api/changes/{id}/run");
    let body = r#"{"check":"cargo-test"}"#;

    // Nothing recorded.
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(response_json(&r)["error"], "permission_required");

    // A file somebody else wrote into the repository is not Sam's permission.
    std::fs::create_dir_all(root.join(".thruline")).expect("dir");
    let forged = serde_json::json!({
        "schema_version": 1,
        "project_folder": root.display().to_string(),
        "granted_by": "sam",
        "granted_at": "2026-01-01T00:00:00Z",
        "promise": "It does not install anything and does not touch the network.",
        "seal": "0".repeat(64),
    });
    std::fs::write(
        root.join(PERMISSION_FILE),
        serde_json::to_vec(&forged).expect("json"),
    )
    .expect("forged file");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "a forged permission was honoured: {r}");
    assert!(executions(temp.path(), &id).is_empty());
    std::fs::remove_file(root.join(PERMISSION_FILE)).expect("remove forged");

    // Sam allows it, once; the file lands under the project, and the run goes.
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    assert!(view["permission"].is_null(), "{view}");
    grant(&state, &id);
    assert!(root.join(PERMISSION_FILE).is_file());
    assert!(run_route::permission(&state.db, &root).is_some());
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 202, "{r}");
    wait_idle(&state, &id);
    assert_eq!(executions(temp.path(), &id).len(), 1);

    // Now the seal key exists. A file somebody wrote into the repository,
    // naming THIS folder, is still not Sam's permission: only the key beside
    // the ledger could have sealed it.
    let mut forged_now: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join(PERMISSION_FILE)).expect("read"))
            .expect("json");
    forged_now["granted_by"] = serde_json::json!("an agent");
    forged_now["seal"] = serde_json::json!("0".repeat(64));
    let genuine = std::fs::read(root.join(PERMISSION_FILE)).expect("genuine");
    std::fs::write(
        root.join(PERMISSION_FILE),
        serde_json::to_vec(&forged_now).expect("json"),
    )
    .expect("write");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(
        status(&r),
        403,
        "a forged permission was honoured once a key existed: {r}"
    );
    assert_eq!(response_json(&r)["error"], "permission_required");
    assert_eq!(
        executions(temp.path(), &id).len(),
        1,
        "the forged file started a run"
    );
    std::fs::write(root.join(PERMISSION_FILE), genuine).expect("restore genuine");

    // A permission for another folder is not a permission for this one: the
    // project moved, so it is asked again.
    let mut moved: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join(PERMISSION_FILE)).expect("read"))
            .expect("json");
    moved["project_folder"] = serde_json::json!("/somewhere/else");
    std::fs::write(
        root.join(PERMISSION_FILE),
        serde_json::to_vec(&moved).expect("json"),
    )
    .expect("write");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "{r}");
}

/* ── a double-click is one job ─────────────────────────────────────────── */

static SLOW: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sleep",
    args: &["2"],
}];

#[test]
fn a_double_click_starts_exactly_one_job() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(SLOW));
    grant(&state, &id);
    let run = format!("/api/changes/{id}/run");
    let headers = writer(&state);

    // Two presses at once, from two connections.
    let (a, b) = std::thread::scope(|s| {
        let one = s.spawn(|| post(&state, &run, r#"{"check":"cargo-test"}"#, &headers));
        let two = s.spawn(|| post(&state, &run, r#"{"check":"cargo-test"}"#, &headers));
        (one.join().expect("one"), two.join().expect("two"))
    });
    let mut statuses = [status(&a), status(&b)];
    statuses.sort_unstable();
    assert_eq!(statuses, [202, 409], "two presses, two jobs:\n{a}\n{b}");
    let (started, refused) = if status(&a) == 202 {
        (&a, &b)
    } else {
        (&b, &a)
    };
    assert_eq!(response_json(refused)["error"], "already_running");
    assert_eq!(
        response_json(refused)["run"],
        response_json(started)["run"],
        "the second press names the run that is already going"
    );
    // And a third press while it runs is the same answer.
    let c = post(&state, &run, r#"{"check":"cargo-test"}"#, &headers);
    assert_eq!(status(&c), 409, "{c}");

    wait_idle(&state, &id);
    let runs = executions(temp.path(), &id);
    assert_eq!(runs.len(), 1, "exactly one job was recorded: {runs:?}");
}

/* ── the outcomes, from real processes ─────────────────────────────────── */

static LONG: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sleep",
    args: &["20"],
}];

static PASS: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &["-c", "echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'"],
}];
static NOTHING: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &["-c", "echo 'test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out; finished in 0.00s'"],
}];
static FAIL: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &["-c", "echo 'test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 101"],
}];
static ABSENT: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "thruline-p65-no-such-program",
    args: &[],
}];

fn run_and_read(catalogue: &'static [RunnableCheck]) -> (serde_json::Value, usize) {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(catalogue));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert!(matches!(status(&r), 200 | 202), "{r}");
    let view = wait_idle(&state, &id);
    let n = executions(temp.path(), &id).len();
    (view["checks"][0]["last"].clone(), n)
}

#[test]
fn only_a_run_that_counted_a_passing_suite_reads_as_passed() {
    let (last, n) = run_and_read(PASS);
    assert_eq!(n, 1);
    assert_eq!(last["outcome"], "passed", "{last}");
    assert_eq!(last["tests_passed"], 2);
    assert_eq!(last["settles"], true);

    let (last, _) = run_and_read(NOTHING);
    assert_eq!(last["outcome"], "checked_nothing", "{last}");
    assert_eq!(last["why"], "no_test_ran");
    assert_eq!(last["tests_passed"], 0);

    let (last, _) = run_and_read(FAIL);
    assert_eq!(last["outcome"], "failed", "{last}");
    assert_eq!(last["exit_code"], 101);
    assert_eq!(last["tests_failed"], 1);
}

#[test]
fn a_program_that_is_not_there_is_tool_not_found_and_nothing_is_installed() {
    let (last, n) = run_and_read(ABSENT);
    assert_eq!(
        n, 1,
        "the attempt is recorded as a check that could not start"
    );
    assert_eq!(last["outcome"], "tool_not_found", "{last}");
    assert_eq!(last["state"], "blocked");
    assert_eq!(last["reason"], "prerequisite_absent");
    assert_eq!(last["settles"], false);
}

/* ── OFFLINE-BUILD-OUTCOME (D-051): a real cargo, offline ───────────────── */

/// A real `cargo build`, started by the sheet (which turns networking off
/// itself). `CARGO_HOME` is an empty folder beside the project inside this
/// test's own temporary folder, so no dependency is ever cached and cargo's
/// registry reads and writes nothing outside it; the caller's shared target
/// stays outside the project, so the build moves nothing the run is examining.
static CARGO_BUILD: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &[
        "-c",
        "CARGO_HOME=\"$PWD/../cargo-home\" CARGO_TARGET_DIR=\"$CARGO_TARGET_DIR\" \
         exec cargo build",
    ],
}];

/// Write a Cargo project into the Change's folder, commit it, run
/// [`CARGO_BUILD`] from the sheet, and read the sheet's last outcome.
fn run_cargo_offline(dependency: &str, lib: &str) -> serde_json::Value {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    std::fs::create_dir_all(temp.path().join("cargo-home")).expect("cargo home");
    std::fs::create_dir_all(root.join("src")).expect("src");
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"offline_fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n\
             [dependencies]\n{dependency}"
        ),
    )
    .expect("manifest");
    if dependency.is_empty() {
        std::fs::write(
            root.join("Cargo.lock"),
            "# This file is automatically @generated by Cargo.\n\
             # It is not intended for manual editing.\n\
             version = 4\n\n\
             [[package]]\n\
             name = \"offline_fixture\"\n\
             version = \"0.1.0\"\n",
        )
        .expect("lockfile");
    }
    std::fs::write(root.join("src/lib.rs"), lib).expect("lib");
    for args in [
        vec!["add", "."],
        vec!["commit", "-q", "-m", "cargo project"],
    ] {
        let ok = std::process::Command::new("git")
            .args(&args)
            .current_dir(&root)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .expect("git runs")
            .success();
        assert!(ok, "git {args:?}");
    }
    let state = state_with(temp.path(), Some(CARGO_BUILD));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert!(matches!(status(&r), 200 | 202), "{r}");
    let view = wait_idle(&state, &id);
    view["checks"][0]["last"].clone()
}

const LIB_OK: &str = "pub fn answer() -> u32 {\n    42\n}\n";
const LIB_TYPE_ERROR: &str = "pub fn answer() -> u32 {\n    \"not a number\"\n}\n";

/// **The defect, as a test.** A project that needs a dependency this machine
/// does not have, built offline, never reached its own code: the sheet reads
/// the couldn't-check outcome (`checked_nothing`), never `failed`.
#[test]
fn an_offline_build_missing_a_dependency_reads_checked_nothing_not_failed() {
    let last = run_cargo_offline("serde = \"1\"\n", LIB_OK);
    assert_eq!(last["exit_code"], 101, "{last}");
    assert_ne!(last["outcome"], "failed", "{last}");
    assert_eq!(last["outcome"], "checked_nothing", "{last}");
    assert_eq!(last["why"], "prerequisite_absent", "{last}");
    assert_eq!(last["reason"], "prerequisite_absent", "{last}");
}

/// **The half the fix may not spend.** A compile error in the project's own
/// code still reads `failed`.
#[test]
fn a_type_error_in_the_projects_own_code_still_reads_failed() {
    let last = run_cargo_offline("", LIB_TYPE_ERROR);
    assert_eq!(last["exit_code"], 101, "{last}");
    assert_eq!(last["outcome"], "failed", "{last}");
    assert!(last["reason"].is_null(), "{last}");
}

/// **The mixed case.** A missing dependency AND a type error: offline, cargo
/// stops at resolution and never compiles, so nothing in the record is about
/// the type error. It reads checked-nothing, never a pass and never a failure
/// the run did not see.
#[test]
fn a_fetch_failure_that_stopped_before_compiling_reads_checked_nothing() {
    let last = run_cargo_offline("serde = \"1\"\n", LIB_TYPE_ERROR);
    assert_eq!(last["exit_code"], 101, "{last}");
    assert_eq!(last["outcome"], "checked_nothing", "{last}");
    assert_eq!(last["why"], "prerequisite_absent", "{last}");
}

#[cfg(unix)]
#[test]
fn stopping_a_run_from_the_sheet_reads_as_cancelled_by_you() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(LONG));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let run = response_json(&r)["run"].as_str().expect("run").to_string();
    // Wait for the wrapper's record, then stop it through the T-3-4 address.
    let deadline = std::time::Instant::now() + Duration::from_secs(20);
    while executions(temp.path(), &id).is_empty() {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(25));
    }
    // Exactly the address the screen sends: the key through
    // `encodeURIComponent` (`CHANGE_CANCEL_RUN` in change-workspace.jsx).
    let r = post(
        &state,
        &format!(
            "/api/changes/{id}/runs/{}/cancel",
            encode_uri_component(&run)
        ),
        r#"{"actor":"sam"}"#,
        &format!("Origin: http://{ORIGIN}\r\n"),
    );
    assert_eq!(
        status(&r),
        200,
        "the screen's own Cancel did not reach the run: {r}"
    );
    let view = wait_idle(&state, &id);
    let last = &view["checks"][0]["last"];
    assert_eq!(last["outcome"], "cancelled", "{last}");
    assert_eq!(last["settles"], false);
}

/// The view never serves the token, and names where the run happens in
/// Sam's words — the home folder as `~`.
#[test]
fn the_run_sheet_view_serves_no_token() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), None);
    let r = get(&state, &format!("/api/changes/{id}/run"), "");
    assert_eq!(status(&r), 200, "{r}");
    assert!(!r.contains(&token_of(&state)), "the view leaked the token");
    let view = response_json(&r);
    assert_eq!(view["checks"][0]["id"], "cargo-test");
    assert_eq!(view["checks"][0]["command"], "cargo test");
    assert!(view["project_folder"].is_string());
    assert_eq!(view["environments"][0]["id"], "hq");
}

/* ── the run is recorded with the environment it ran in ────────────────── */

static ENV_ONLY: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "thruline-p65-envprog",
    args: &[],
}];

/// "Look somewhere else" re-runs the check in the environment Sam picked, and
/// the record names the tool THAT environment had — its version, asked of the
/// program that ran — not whatever HQ's own search path would have found.
///
/// ADV-run-security-2 (run lane F2) moved the environment this test picks.
/// It used to plant the program in the project's own `.venv/bin`, pick
/// `project-venv`, and require that program to run and read `passed` — which
/// is the F2 finding itself: a program the repository wrote, recorded as the
/// check. The same program is still planted there, and is now required to be
/// neither offered nor run. The environment picked is a home folder outside
/// the project (inside this test's temporary folder, through the test-only
/// machine seam), and every assertion about the tool that ran is unchanged.
#[cfg(unix)]
#[test]
fn a_run_picked_into_another_environment_is_recorded_with_that_environments_tool() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let project_copy_ran = outside.join("PROJECT-COPY-RAN.txt");
    let plant = |bin: &Path, script: String| {
        std::fs::create_dir_all(bin).expect("bin");
        let prog = bin.join("thruline-p65-envprog");
        std::fs::write(&prog, script).expect("program");
        std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    };
    let home = temp.path().join("home");
    plant(
        &home.join(".local/bin"),
        "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'envprog 9.9.9'; exit 0; fi\n\
         echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n"
            .to_string(),
    );
    plant(
        &root.join(".venv/bin"),
        format!(
            "#!/bin/sh\necho ran >> '{}'\nif [ \"$1\" = --version ]; then echo 'envprog 6.6.6-project'; exit 0; fi\n\
             echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n",
            project_copy_ran.display()
        ),
    );
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            Some(ENV_ONLY),
            std::env::join_paths(["/usr/bin", "/bin"]).expect("path"),
            home.clone(),
        ),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);

    // Where HQ looks by default it is not there: Tool not found, nothing installed.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(response_json(&r)["outcome"], "tool_not_found", "{r}");
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    let environments = view["environments"].as_array().expect("environments");
    let local = environments
        .iter()
        .find(|e| e["id"] == "home-local")
        .cloned()
        .expect("the home folder's environment is found");
    assert_eq!(local["has"], serde_json::json!(["thruline-p65-envprog"]));
    assert!(
        !environments.iter().any(|e| e["id"] == "project-venv"),
        "a folder inside the project is offered as a place to run: {view}"
    );

    // The project's own copy is never run, even when asked for by id.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test","environment":"project-venv"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(response_json(&r)["error"], "program_in_project", "{r}");
    assert!(!project_copy_ran.exists(), "the project's own copy ran");

    // Picked: it runs there, and passes.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test","environment":"home-local"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let view = wait_idle(&state, &id);
    assert_eq!(view["checks"][0]["last"]["outcome"], "passed", "{view}");
    assert_eq!(view["remembered_environment"], "home-local");
    assert!(!project_copy_ran.exists(), "the project's own copy ran");

    // The record: the tool that ran, present, at the version it answered with.
    let runs = executions(temp.path(), &id);
    let binding = runs
        .iter()
        .rev()
        .find_map(|r| r.binding_id.clone())
        .expect("the run left a binding");
    let ledger =
        thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    let inputs = thruline_core::examined_inputs::standing(
        &ledger,
        thruline_core::examined_inputs::SubjectKind::CheckRun,
        &binding,
    )
    .expect("read")
    .expect("an examined-inputs record for the run");
    assert_eq!(inputs.tools.len(), 1);
    assert_eq!(inputs.tools[0].name, "thruline-p65-envprog");
    assert!(
        inputs.tools[0].present,
        "the tool that ran was recorded absent"
    );
    assert_eq!(
        inputs.tools[0].version.value(),
        Some("envprog 9.9.9"),
        "the version of the tool that ran"
    );
}

/* ── saying yes does not move the version ──────────────────────────────── */

/// Granting permission writes a file under the project; the project's version
/// (which counts untracked files) must be the same before and after, or every
/// earlier run would read as "ran on an earlier version" the moment Sam said yes.
#[test]
fn granting_permission_does_not_move_the_projects_version() {
    use thruline_core::workspace_identity::{self as wsid, Observation, Scope};
    let identity = |root: &Path| match wsid::observe(root, Scope::everything()).expect("observe") {
        Observation::Complete(fp) => fp.identity().to_string(),
        Observation::Incomplete { reason, .. } => panic!("incomplete observation: {reason}"),
    };
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let before = identity(&root);
    grant(&state, &id);
    assert!(root.join(PERMISSION_FILE).is_file());
    assert_eq!(identity(&root), before, "saying yes moved the version");
    assert_eq!(
        std::fs::read_to_string(root.join(".thruline/.gitignore")).expect("ignore file"),
        run_route::IGNORE_FILE_TEXT
    );
}

/* ── P6-5B: the permission is in the record, and the record wins ───────── */

fn permission_rows(dir: &Path, root: &Path) -> Vec<thruline_core::run_permission::ConsentRow> {
    let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
    thruline_core::run_permission::history(&ledger, &root.display().to_string()).expect("history")
}

/// **Guard 5B-1.** A permission file with no matching grant in the record is
/// not a permission. The file below is sealed with this store's own key — the
/// file ALONE passes every P6-5 check — so the only thing refusing it is the
/// record. And the other way round: a grant in the record whose file has gone
/// is asked again, because the file going away is how removing the project
/// has always withdrawn it.
#[test]
fn guard_5b_1_a_permission_file_the_record_does_not_hold_is_refused() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let run = format!("/api/changes/{id}/run");
    let body = r#"{"check":"cargo-test"}"#;

    run_route::write_sealed_file_for_test(
        &state.db,
        &root,
        "sam",
        "2026-09-24T09:00:00Z",
        "rpm_not_in_the_record",
    )
    .expect("a sealed file");
    assert!(
        run_route::permission(&state.db, &root).is_some(),
        "the file must be a valid seal, or this test proves nothing"
    );
    assert!(permission_rows(temp.path(), &root).is_empty());

    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "a file the record does not hold ran: {r}");
    assert_eq!(response_json(&r)["error"], "permission_not_in_record");
    assert!(executions(temp.path(), &id).is_empty());
    let view = response_json(&get(&state, &run, ""));
    assert!(view["permission"].is_null(), "{view}");
    assert_eq!(view["consent"]["state"], "not_in_record");

    // Sam allows it: ONE row, then the file, which now names that row.
    grant(&state, &id);
    grant(&state, &id); // a second press while it stands appends nothing
    let rows = permission_rows(temp.path(), &root);
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0].act, thruline_core::run_permission::Act::Granted);
    let file: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join(PERMISSION_FILE)).expect("file"))
            .expect("json");
    assert_eq!(file["grant_id"], serde_json::json!(rows[0].id));
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 202, "{r}");
    wait_idle(&state, &id);
    let view = response_json(&get(&state, &run, ""));
    assert_eq!(view["consent"]["state"], "given");
    assert_eq!(
        view["permission"]["granted_at"],
        serde_json::json!(rows[0].recorded_at)
    );
    assert_eq!(view["consent"]["history"][0]["act"], "granted");
    assert_eq!(
        view["consent"]["history"][0]["receipt"]["table"],
        "run_permissions"
    );
    assert_eq!(
        view["consent"]["history"][0]["receipt"]["id"],
        serde_json::json!(rows[0].id)
    );

    // The record holds the grant; its file is gone: asked again, nothing runs.
    std::fs::remove_file(root.join(PERMISSION_FILE)).expect("remove the cache");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(response_json(&r)["error"], "permission_required");
    assert_eq!(executions(temp.path(), &id).len(), 1);
    let view = response_json(&get(&state, &run, ""));
    assert_eq!(view["consent"]["state"], "file_missing");
    assert!(view["permission"].is_null());
}

/// **Guard 5B-2.** Taking permission back is one row pointing at the grant,
/// and after it no run goes — even when the file is put back exactly as it
/// was. The revoke address carries guard 2 like every write here.
#[test]
fn guard_5b_2_a_revocation_refuses_the_next_run_even_with_the_file_still_there() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let run = format!("/api/changes/{id}/run");
    let revoke = format!("/api/changes/{id}/run/permission/revoke");
    let body = r#"{"check":"cargo-test"}"#;

    grant(&state, &id);
    let kept = std::fs::read(root.join(PERMISSION_FILE)).expect("the file");

    // Guard 2 holds on the new address: no Origin, or no token, is refused.
    let r = post(&state, &revoke, "{}", "");
    assert_eq!(status(&r), 403, "{r}");
    let r = post(
        &state,
        &revoke,
        "{}",
        &format!("Origin: http://{ORIGIN}\r\n"),
    );
    assert_eq!(status(&r), 403, "{r}");
    assert_eq!(permission_rows(temp.path(), &root).len(), 1);

    let r = post(&state, &revoke, "{}", &writer(&state));
    assert_eq!(status(&r), 200, "{r}");
    assert!(
        !root.join(PERMISSION_FILE).exists(),
        "the cache stayed behind"
    );
    let rows = permission_rows(temp.path(), &root);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[1].act, thruline_core::run_permission::Act::Revoked);
    assert_eq!(rows[1].revokes.as_deref(), Some(rows[0].id.as_str()));

    // The file comes back exactly as it was. The record wins.
    std::fs::write(root.join(PERMISSION_FILE), &kept).expect("put the file back");
    assert!(run_route::permission(&state.db, &root).is_some());
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "a revoked permission ran: {r}");
    assert_eq!(response_json(&r)["error"], "permission_revoked");
    assert!(executions(temp.path(), &id).is_empty());
    let view = response_json(&get(&state, &run, ""));
    assert_eq!(view["consent"]["state"], "revoked");
    assert!(view["permission"].is_null());
    assert_eq!(view["consent"]["history"][1]["act"], "revoked");
    assert_eq!(
        view["consent"]["history"][1]["revokes"],
        serde_json::json!(rows[0].id)
    );

    // Nothing to take back twice.
    let r = post(&state, &revoke, "{}", &writer(&state));
    assert_eq!(status(&r), 409, "{r}");
    assert_eq!(response_json(&r)["error"], "no_permission_to_revoke");
    assert_eq!(permission_rows(temp.path(), &root).len(), 2);

    // Allowed again: a new grant, and the whole chain still adds up.
    grant(&state, &id);
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 202, "{r}");
    wait_idle(&state, &id);
    let ledger =
        thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    assert_eq!(
        thruline_core::run_permission::verify_chain(&ledger).expect("chain"),
        3
    );
}

/* ── ADV-run-ledger-4: one standing grant, however many presses race ───── */

/// **ADV-run-ledger-4 (adversary F4).** Ten presses of "Allow" at once, from
/// ten connections, give ONE grant: the record says a person allowed checks
/// here once, and the history the sheet shows has one row, not ten. Then,
/// round after round, a revocation followed by two racing presses appends
/// exactly one new grant each time — and that grant is the one the next
/// revocation takes back, so no grant is left standing that nothing revoked.
#[test]
fn racing_presses_of_allow_append_exactly_one_grant() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let allow = format!("/api/changes/{id}/run/permission");
    let revoke = format!("/api/changes/{id}/run/permission/revoke");
    let headers = writer(&state);

    let answers: Vec<String> = std::thread::scope(|s| {
        let presses: Vec<_> = (0..10)
            .map(|_| s.spawn(|| post(&state, &allow, "{}", &headers)))
            .collect();
        presses
            .into_iter()
            .map(|p| p.join().expect("a press"))
            .collect()
    });
    for answer in &answers {
        assert_eq!(status(answer), 200, "{answer}");
        assert_eq!(response_json(answer)["granted"], true, "{answer}");
    }
    let rows = permission_rows(temp.path(), &root);
    assert_eq!(
        rows.len(),
        1,
        "ten racing presses appended {} grants: {rows:#?}",
        rows.len()
    );
    assert_eq!(rows[0].act, thruline_core::run_permission::Act::Granted);

    for round in 0..8 {
        let r = post(&state, &revoke, "{}", &headers);
        assert_eq!(status(&r), 200, "round {round}: {r}");
        let (a, b) = std::thread::scope(|s| {
            let one = s.spawn(|| post(&state, &allow, "{}", &headers));
            let two = s.spawn(|| post(&state, &allow, "{}", &headers));
            (one.join().expect("one"), two.join().expect("two"))
        });
        assert_eq!(status(&a), 200, "round {round}: {a}");
        assert_eq!(status(&b), 200, "round {round}: {b}");
        let rows = permission_rows(temp.path(), &root);
        let grants = rows
            .iter()
            .filter(|r| r.act == thruline_core::run_permission::Act::Granted)
            .count();
        assert_eq!(
            grants,
            round + 2,
            "round {round}: two racing presses appended more than one grant: {rows:#?}"
        );
    }

    // Every grant but the standing one was taken back by name.
    let rows = permission_rows(temp.path(), &root);
    let revoked: Vec<&str> = rows.iter().filter_map(|r| r.revokes.as_deref()).collect();
    let unrevoked: Vec<&str> = rows
        .iter()
        .filter(|r| r.act == thruline_core::run_permission::Act::Granted)
        .map(|r| r.id.as_str())
        .filter(|g| !revoked.contains(g))
        .collect();
    assert_eq!(
        unrevoked.len(),
        1,
        "grants nothing took back: {unrevoked:?}"
    );
    assert_eq!(
        unrevoked[0],
        rows.last().expect("rows").id,
        "the one grant left standing is the latest row"
    );
    let ledger =
        thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
    assert_eq!(
        thruline_core::run_permission::verify_chain(&ledger).expect("chain") as usize,
        rows.len()
    );
}

/* ── ADV-run-ledger-5: `thruline verify` passes after a run from the sheet ── */

/// Exactly what `thruline verify` decides its exit code on (`Command::Verify`
/// in `main.rs`): every project's digests, receipts and chain, the
/// observation chain and the run-permission chain. `Ok(())` is exit 0; `Err`
/// carries every finding, one line each.
fn thruline_verify(dir: &Path) -> std::result::Result<(), Vec<String>> {
    let ledger = thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
    let report = thruline_core::ingest::verify(&ledger, None).expect("verify runs");
    let mut findings = Vec::new();
    for (project, v) in &report.projects {
        findings.extend(
            v.digest_mismatches
                .iter()
                .map(|m| format!("{project} digest {m}")),
        );
        findings.extend(
            v.missing_files
                .iter()
                .map(|m| format!("{project} missing {m}")),
        );
        findings.extend(
            v.chain_mismatches
                .iter()
                .map(|m| format!("{project} chain {m}")),
        );
        findings.extend(
            v.raw_mismatches
                .iter()
                .map(|m| format!("{project} raw {}:{}", m.file, m.line)),
        );
    }
    if let Err(e) = thruline_core::observation::verify_chain(&ledger) {
        findings.push(format!("observations {e}"));
    }
    if let Err(e) = thruline_core::run_permission::verify_chain(&ledger) {
        findings.push(format!("permissions {e}"));
    }
    if report.ok() && findings.is_empty() {
        Ok(())
    } else {
        findings.push(format!("report.ok() = {}", report.ok()));
        Err(findings)
    }
}

/// **ADV-run-ledger-5 (adversary F5; named by the founder).** After a run
/// started from the sheet, `thruline verify` passes. The shared fixture's
/// transcript row was written by hand with no file behind it, so it is first
/// given one — a real transcript, with its digest taken the way ingest takes
/// it — and verify must pass BEFORE the run, or this test proves nothing.
#[test]
fn after_a_run_from_the_sheet_thruline_verify_passes() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let transcript = temp.path().join("s001.jsonl");
    std::fs::write(
        &transcript,
        "{\"type\":\"user\",\"sessionId\":\"cccccccc-0000-4000-8000-00000000s001\",\
         \"timestamp\":\"2026-01-02T10:00:00.000Z\",\"message\":{\"role\":\"user\",\
         \"content\":\"Retry failed uploads.\"}}\n",
    )
    .expect("transcript");
    let mut stats = thruline_core::parse::ParseStats::default();
    thruline_core::parse::parse_file(&transcript, &mut stats).expect("parse");
    {
        let ledger =
            thruline_core::ledger::Ledger::open(temp.path().join("ledger.db")).expect("ledger");
        let n = ledger
            .conn()
            .execute(
                "UPDATE sessions SET file_path=?1, raw_line_count=?2, raw_digest=?3
                  WHERE id='cccccccc-0000-4000-8000-00000000s001'",
                rusqlite::params![
                    transcript.display().to_string(),
                    stats.raw_lines_total as i64,
                    stats.raw_digest
                ],
            )
            .expect("give the fixture transcript its file");
        assert_eq!(n, 1);
    }
    assert_eq!(
        thruline_verify(temp.path()),
        Ok(()),
        "verify must pass before the run, or this test proves nothing"
    );

    let state = state_with(temp.path(), Some(PASS));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let view = wait_idle(&state, &id);
    assert_eq!(view["checks"][0]["last"]["outcome"], "passed", "{view}");

    assert_eq!(
        thruline_verify(temp.path()),
        Ok(()),
        "thruline verify fails after a run from the sheet"
    );
}

/* ── ADV-run-ledger-3: the run the sheet starts is told to stay offline ─── */

/// Stands in for `cargo test`: it passes only when the run was told to stay
/// off the network the way cargo is told (`CARGO_NET_OFFLINE=true`), and
/// otherwise fails the way cargo fails when a fetch is refused.
static NEEDS_OFFLINE: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &[
        "-c",
        "if [ \"$CARGO_NET_OFFLINE\" = true ]; then \
         echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'; \
         else echo 'error: the run was left free to use the network' >&2; exit 101; fi",
    ],
}];

/// **ADV-run-ledger-3 (adversary F3).** The sheet promises "It does not
/// install anything and does not touch the network", and seals that sentence
/// into the permission file. A run it starts must be told so: cargo is started
/// with its offline switch on, so it never reaches for a registry.
///
/// The stand-in reads the switch from its own environment, which it inherits
/// from this test process; if this process already carried the switch, a pass
/// would prove nothing, so that is refused out loud rather than passed.
#[test]
fn a_run_from_the_sheet_is_told_to_stay_off_the_network() {
    assert_ne!(
        std::env::var("CARGO_NET_OFFLINE").ok().as_deref(),
        Some("true"),
        "this test process already carries CARGO_NET_OFFLINE=true, so the run \
         would inherit it and a pass would prove nothing about the route"
    );
    let (last, n) = run_and_read(NEEDS_OFFLINE);
    assert_eq!(n, 1);
    assert_eq!(
        last["outcome"], "passed",
        "the run was not told to stay off the network: {last}"
    );
}

/// **ADV-run-ledger-3, the fence.** Every program the sheet can start has an
/// offline switch the route sets, so a row cannot be added to the allow-list
/// that the sheet's promise does not cover.
#[test]
fn every_check_the_sheet_can_start_has_an_offline_switch() {
    for c in crate::check::RUNNABLE_CHECKS {
        assert!(
            crate::check::OFFLINE_SWITCHES
                .iter()
                .any(|(program, _, _)| *program == c.program),
            "{} can be started from the sheet with no switch that keeps it off the network",
            c.program
        );
    }
}

/* ── ADV-run-security-1: a link planted under the project is never followed ── */

/// A folder beside the project, inside this test's own temporary folder:
/// outside the project root, and nowhere else on the machine.
fn outside_the_project(temp: &Path, root: &Path) -> PathBuf {
    let outside = temp.join("outside");
    std::fs::create_dir_all(&outside).expect("outside dir");
    let outside = outside.canonicalize().expect("canonical outside");
    let temp = temp.canonicalize().expect("canonical temp");
    assert!(
        outside.starts_with(&temp) && !outside.starts_with(root),
        "the fixture must stay inside its temporary folder and outside the project"
    );
    outside
}

/// **ADV-run-security-1 (run lane F1).** "Allow" writes this machine's
/// permission under the project's own `.thruline` folder — and whoever
/// controls the repository controls what is in that folder. A link planted
/// there must not carry the write, the read or the removal anywhere else:
/// Allow refuses, writes nothing through the link, appends nothing to the
/// record, and the sheet reads no permission through a link. The whole
/// fixture is inside this test's temporary folder: the project, the link, and
/// the link's target, in a sibling folder outside the project root.
#[cfg(unix)]
#[test]
fn allow_refuses_to_follow_a_symlink_whose_target_is_outside_the_project_root() {
    use std::os::unix::fs::symlink;
    const VICTIM: &str = "a file outside the project, which Allow must never write\n";
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let allow = format!("/api/changes/{id}/run/permission");
    let run = format!("/api/changes/{id}/run");
    let body = r#"{"check":"cargo-test"}"#;
    let outside = outside_the_project(temp.path(), &root);
    let victim = outside.join("victim.txt");
    std::fs::write(&victim, VICTIM).expect("victim");

    // 1. The file's own name is a link to a file outside the project.
    std::fs::create_dir_all(root.join(".thruline")).expect(".thruline");
    let link = root.join(PERMISSION_FILE);
    symlink("../../outside/victim.txt", &link).expect("plant the link");
    assert_eq!(
        link.canonicalize().expect("the link resolves"),
        victim,
        "the link must point outside the project, or this test proves nothing"
    );
    let r = post(&state, &allow, "{}", &writer(&state));
    assert_eq!(
        std::fs::read_to_string(&victim).expect("victim"),
        VICTIM,
        "Allow wrote through a link to a file outside the project: {r}"
    );
    assert_eq!(status(&r), 409, "{r}");
    assert_eq!(response_json(&r)["error"], "permission_file_not_plain");
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("the link")
            .file_type()
            .is_symlink(),
        "the link was not left exactly where it was"
    );
    assert!(
        permission_rows(temp.path(), &root).is_empty(),
        "a refused Allow appended a grant to the record"
    );
    let view = response_json(&get(&state, &run, ""));
    assert!(view["permission"].is_null(), "{view}");
    assert_ne!(view["consent"]["state"], "given", "{view}");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "{r}");
    assert!(executions(temp.path(), &id).is_empty());

    // 2. One level up: `.thruline` itself is a link to the outside folder.
    std::fs::remove_file(&link).expect("remove the planted link");
    std::fs::remove_dir(root.join(".thruline")).expect("remove the empty folder");
    symlink("../outside", root.join(".thruline")).expect("plant the folder link");
    let r = post(&state, &allow, "{}", &writer(&state));
    assert!(
        !outside.join("run-permission.json").exists() && !outside.join(".gitignore").exists(),
        "Allow wrote into a folder outside the project through a linked .thruline: {r}"
    );
    assert_eq!(status(&r), 409, "{r}");
    assert_eq!(response_json(&r)["error"], "permission_file_not_plain");
    assert!(permission_rows(temp.path(), &root).is_empty());
    assert_eq!(std::fs::read_to_string(&victim).expect("victim"), VICTIM);

    // 3. A real permission, moved outside and linked back, is not read as one.
    std::fs::remove_file(root.join(".thruline")).expect("remove the folder link");
    grant(&state, &id);
    let view = response_json(&get(&state, &run, ""));
    assert_eq!(view["consent"]["state"], "given", "{view}");
    let moved = outside.join("run-permission.json");
    std::fs::rename(root.join(PERMISSION_FILE), &moved).expect("move the file out");
    symlink(
        "../../outside/run-permission.json",
        root.join(PERMISSION_FILE),
    )
    .expect("link it back");
    assert!(
        run_route::permission(&state.db, &root).is_none(),
        "a permission was read through a link to a file outside the project"
    );
    let view = response_json(&get(&state, &run, ""));
    assert_ne!(view["consent"]["state"], "given", "{view}");
    let r = post(&state, &run, body, &writer(&state));
    assert_eq!(status(&r), 403, "a permission read through a link ran: {r}");
    assert!(executions(temp.path(), &id).is_empty());

    // 4. Taking it back removes nothing outside the project through a linked
    //    `.thruline`: the record says it was taken back, and the file out
    //    there is untouched.
    std::fs::remove_file(root.join(PERMISSION_FILE)).expect("remove the file link");
    std::fs::remove_file(root.join(".thruline/.gitignore")).expect("remove the ignore file");
    std::fs::remove_dir(root.join(".thruline")).expect("remove the folder");
    symlink("../outside", root.join(".thruline")).expect("plant the folder link again");
    let r = post(
        &state,
        &format!("/api/changes/{id}/run/permission/revoke"),
        "{}",
        &writer(&state),
    );
    assert_eq!(status(&r), 200, "{r}");
    assert!(
        moved.is_file(),
        "taking permission back removed a file outside the project through a link"
    );
    let rows = permission_rows(temp.path(), &root);
    assert_eq!(rows.len(), 2, "{rows:?}");
    assert_eq!(rows[1].act, thruline_core::run_permission::Act::Revoked);
}

/// **ADV-run-security-4 (D-038 pre-gate, mutation M1b).** Allow checks the
/// permission file's name without following a link, so a SYMBOLIC link never
/// reaches the write — and that left nothing to tell a write that replaces the
/// name from one that writes through it. A HARD link does: the name is a plain
/// file whose inode is shared with a file outside the project. Allow must
/// replace the name, never write through it: the outside file keeps every
/// byte, and the name no longer shares its inode. A write by path to the name
/// passes every other test in this file and fails this one. The whole fixture
/// is inside this test's temporary folder.
#[cfg(unix)]
#[test]
fn allow_replaces_a_hard_linked_permission_file_and_never_writes_through_it() {
    use std::os::unix::fs::MetadataExt;
    const VICTIM: &str =
        "a file outside the project, hard-linked into it, which Allow must never write\n";
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let state = state_with(temp.path(), Some(TRUE));
    let outside = outside_the_project(temp.path(), &root);
    let victim = outside.join("victim.txt");
    std::fs::write(&victim, VICTIM).expect("victim");
    std::fs::create_dir_all(root.join(".thruline")).expect(".thruline");
    let name = root.join(PERMISSION_FILE);
    std::fs::hard_link(&victim, &name).expect("hard-link the outside file into the project");
    let inode = |path: &Path| {
        let meta = std::fs::metadata(path).expect("metadata");
        (meta.dev(), meta.ino(), meta.nlink())
    };
    let (dev, ino, links) = inode(&victim);
    assert!(
        inode(&name) == (dev, ino, 2) && links == 2,
        "the name and the outside file must be one inode with two names, or this test proves nothing"
    );

    let r = post(
        &state,
        &format!("/api/changes/{id}/run/permission"),
        "{}",
        &writer(&state),
    );
    assert_eq!(
        std::fs::read_to_string(&victim).expect("victim"),
        VICTIM,
        "Allow wrote through a hard link to a file outside the project: {r}"
    );
    assert_eq!(status(&r), 200, "{r}");
    let (name_dev, name_ino, _) = inode(&name);
    assert!(
        (name_dev, name_ino) != (dev, ino),
        "the permission file's name still shares its inode with the file outside the project"
    );
    assert_eq!(
        inode(&victim),
        (dev, ino, 1),
        "the outside file is still reachable through the project"
    );
    // What the name holds now is this machine's permission, read back whole.
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    assert_eq!(view["consent"]["state"], "given", "{view}");
}

/* ── ADV-run-security-2: a program the repository planted never runs ───── */

/// A lying `cargo`: it does none of the work, says two tests passed, exits 0,
/// and leaves one line per start in `marker` — which sits outside the project
/// so the project's version cannot see it.
#[cfg(unix)]
fn plant_lying_cargo(dir: &Path, marker: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).expect("the planted folder");
    let program = dir.join("cargo");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\necho \"ran with: $*\" >> '{}'\necho 'running 2 tests'\n\
             echo 'test tests::a ... ok'\necho 'test tests::b ... ok'\necho ''\n\
             echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n\
             exit 0\n",
            marker.display()
        ),
    )
    .expect("the lying program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    program
}

/// Every place in one served document that reads as a pass: an outcome of
/// `passed`, an observation or state of `proven`, or `ready: true`. Returned
/// as paths, so a failure names where.
fn reads_passed(doc: &serde_json::Value, at: &str, out: &mut Vec<String>) {
    match doc {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                let here = format!("{at}.{key}");
                match (key.as_str(), child) {
                    ("outcome" | "observed" | "state", serde_json::Value::String(word))
                        if word == "passed" || word == "proven" =>
                    {
                        out.push(format!("{here} = {word}"))
                    }
                    ("ready", serde_json::Value::Bool(true)) => out.push(format!("{here} = true")),
                    _ => {}
                }
                reads_passed(child, &here, out);
            }
        }
        serde_json::Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                reads_passed(child, &format!("{at}[{i}]"), out);
            }
        }
        _ => {}
    }
}

/// The founder's four surfaces: the run record (as the sheet serves it, and
/// the execution rows themselves), the served Change, coverage, and readiness.
/// None may read Passed, Proven or Ready, and no criterion may be satisfied.
fn nothing_reads_passed(state: &Arc<State>, dir: &Path, id: &str) -> Vec<String> {
    let mut found = Vec::new();
    let sheet = response_json(&get(state, &format!("/api/changes/{id}/run"), ""));
    reads_passed(&sheet, "run", &mut found);
    for record in executions(dir, id) {
        if record.state == thruline_core::run_execution::ExecutionState::Completed {
            let ledger =
                thruline_core::ledger::Ledger::open(dir.join("ledger.db")).expect("ledger");
            let tally = record
                .binding_id
                .as_deref()
                .and_then(|b| thruline_core::test_tally::of_binding(&ledger, b).expect("tally"));
            if tally.is_some_and(|t| t.counts_a_suite()) {
                found.push(format!(
                    "execution {} counted a passing suite",
                    record.run_key
                ));
            }
        }
    }
    let change = response_json(&get(state, &format!("/api/changes/{id}"), ""));
    reads_passed(&change["change"], "change", &mut found);
    let coverage = response_json(&get(state, &format!("/api/changes/{id}/coverage"), ""));
    reads_passed(&coverage["coverage"], "coverage", &mut found);
    reads_passed(&coverage["readiness"], "coverage.readiness", &mut found);
    let readiness = response_json(&get(state, &format!("/api/changes/{id}/readiness"), ""));
    reads_passed(&readiness["readiness"], "readiness", &mut found);
    for (doc, name) in [
        (&coverage["readiness"], "coverage.readiness"),
        (&readiness["readiness"], "readiness"),
    ] {
        for o in doc["obligations"].as_array().into_iter().flatten() {
            if o["id"]
                .as_str()
                .is_some_and(|i| i.starts_with("criterion:"))
                && o["outcome"] == "satisfied"
            {
                found.push(format!("{name}: {} satisfied", o["id"]));
            }
        }
    }
    found
}

/// **ADV-run-security-2 (run lane F2) — the founder's named test.** Guard 4
/// says nothing is run from the repository. A `cargo` planted in the
/// project's own `node_modules/.bin` — on the search path a run in the
/// `project-node` environment composes — does none of the work, prints
/// "test result: ok. 2 passed" and exits 0. It must never run, and the run
/// record, the served Change, coverage and readiness must never read Passed,
/// Proven or Ready off it. The run is refused before anything starts, with a
/// reason that names the program.
#[cfg(unix)]
#[test]
fn a_lying_binary_planted_in_the_project_is_never_recorded_passed() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let marker = outside.join("LYING-BINARY-RAN.txt");
    plant_lying_cargo(&root.join("node_modules/.bin"), &marker);
    // The real allow-list (`cargo test`), and a machine search path that is
    // this machine's system folders only, so the result cannot depend on
    // whether a real cargo is installed here.
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths(["/usr/bin", "/bin"]).expect("path"),
            temp.path().join("home"),
        ),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);

    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test","environment":"project-node"}"#,
        &writer(&state),
    );
    wait_idle(&state, &id);
    let passed = nothing_reads_passed(&state, temp.path(), &id);
    assert!(
        passed.is_empty(),
        "a program the repository planted was recorded as a pass: {passed:?} — start answered {r}"
    );
    assert!(
        !marker.exists(),
        "the planted program ran: {}",
        std::fs::read_to_string(&marker).unwrap_or_default()
    );
    assert_eq!(status(&r), 403, "{r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "program_in_project", "{refusal}");
    assert_eq!(
        refusal["program"], "cargo",
        "the refusal names the program: {refusal}"
    );
    assert!(
        executions(temp.path(), &id).is_empty(),
        "a refused run left an execution record"
    );
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    assert!(
        view["remembered_environment"].is_null(),
        "a refused environment was remembered for the next press: {view}"
    );
    assert!(
        !view["environments"]
            .as_array()
            .expect("environments")
            .iter()
            .any(|e| e["kind"] == "project"),
        "the sheet still offers a folder inside the project as a place to run: {view}"
    );
}

/// **ADV-run-security-2, the variant.** The same lying `cargo`, with the
/// planted folder put on the search path HQ itself was started with — as an
/// absolute folder, as a relative one (the child runs in the project, so it
/// resolves there), and as a link from a folder outside the project to the
/// planted file. The default environment is used; nothing names the project.
/// Every form is refused before anything starts, and nothing reads a pass.
#[cfg(unix)]
#[test]
fn a_lying_binary_in_a_project_folder_on_hqs_own_path_is_never_recorded_passed() {
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let marker = outside.join("LYING-BINARY-RAN.txt");
    let planted_dir = root.join("node_modules/.bin");
    let planted = plant_lying_cargo(&planted_dir, &marker);
    let linked_dir = outside.join("tools");
    std::fs::create_dir_all(&linked_dir).expect("tools");
    std::os::unix::fs::symlink(&planted, linked_dir.join("cargo"))
        .expect("link to the planted file");

    for (form, first) in [
        ("absolute", planted_dir.clone()),
        ("relative", PathBuf::from("node_modules/.bin")),
        ("linked", linked_dir.clone()),
    ] {
        let state = Arc::new(State {
            run_route: RunRoute::with_machine(
                None,
                std::env::join_paths([first, PathBuf::from("/usr/bin"), PathBuf::from("/bin")])
                    .expect("path"),
                temp.path().join("home"),
            ),
            ..Arc::try_unwrap(state_with(temp.path(), None))
                .unwrap_or_else(|_| panic!("the state is not shared yet"))
        });
        grant(&state, &id);
        let r = post(
            &state,
            &format!("/api/changes/{id}/run"),
            r#"{"check":"cargo-test"}"#,
            &writer(&state),
        );
        wait_idle(&state, &id);
        let passed = nothing_reads_passed(&state, temp.path(), &id);
        assert!(
            passed.is_empty(),
            "{form}: a program the repository planted was recorded as a pass: {passed:?} — start answered {r}"
        );
        assert!(
            !marker.exists(),
            "{form}: the planted program ran: {}",
            std::fs::read_to_string(&marker).unwrap_or_default()
        );
        assert_eq!(status(&r), 403, "{form}: {r}");
        assert_eq!(
            response_json(&r)["error"],
            "program_in_project",
            "{form}: {r}"
        );
        assert_eq!(response_json(&r)["program"], "cargo", "{form}: {r}");
        assert!(executions(temp.path(), &id).is_empty(), "{form}");
    }
}

/// **ADV-run-security-2, what still runs.** A program found outside the
/// project runs as before — and the search path its run is given has no
/// folder inside the project in it, so nothing the program starts by name can
/// be the project's either.
#[cfg(unix)]
#[test]
fn a_program_outside_the_project_runs_with_no_project_folder_on_its_path() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let marker = outside.join("LYING-BINARY-RAN.txt");
    let planted_dir = root.join("node_modules/.bin");
    plant_lying_cargo(&planted_dir, &marker);
    let tools = outside.join("tools");
    std::fs::create_dir_all(&tools).expect("tools");
    let seen = outside.join("PATH-SEEN.txt");
    let program = tools.join("cargo");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 0.0.0-outside'; exit 0; fi\n\
             printf '%s' \"$PATH\" > '{}'\n\
             echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n",
            seen.display()
        ),
    )
    .expect("program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths([
                tools.clone(),
                planted_dir.clone(),
                PathBuf::from("node_modules/.bin"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ])
            .expect("path"),
            temp.path().join("home"),
        ),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let view = wait_idle(&state, &id);
    assert_eq!(view["checks"][0]["last"]["outcome"], "passed", "{view}");
    assert!(!marker.exists(), "the planted program ran");
    let path = std::fs::read_to_string(&seen).expect("the run's search path");
    let canonical_root = root.canonicalize().expect("root");
    for entry in std::env::split_paths(&path) {
        assert!(
            entry.is_absolute(),
            "a relative folder resolves inside the project: {entry:?} in {path}"
        );
        let entry = entry.canonicalize().unwrap_or(entry);
        assert!(
            !entry.starts_with(&canonical_root),
            "the run's search path holds a folder inside the project: {entry:?} in {path}"
        );
    }
}

/// **OFFLINE-KILL-GUARD-R4 (P-2026-09-28-4 (1)), on the sheet.** A cargo
/// outside the toolchain and system directories runs the check with
/// ThruLine's own fetch step skipped. When it fails, the sheet reads Failed —
/// never "checked nothing", which only a failed owned fetch may say — and it
/// serves, in words, that the owned fetch did not run and why: the program,
/// where it was found, and what would allow it.
#[cfg(unix)]
#[test]
fn an_outside_cargo_that_fails_reads_failed_and_the_sheet_says_the_owned_fetch_did_not_run() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let tools = outside.join("tools");
    std::fs::create_dir_all(&tools).expect("tools");
    let ran = outside.join("OUTSIDE-CARGO-RAN.txt");
    let program = tools.join("cargo");
    std::fs::write(
        &program,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 0.0.0-outside'; exit 0; fi\n\
             echo \"$*\" >> '{}'\n\
             echo 'error: no matching package named `serde` found' >&2\nexit 101\n",
            ran.display()
        ),
    )
    .expect("program");
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths([
                tools.clone(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ])
            .expect("path"),
            temp.path().join("home"),
        ),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let view = wait_idle(&state, &id);
    let last = &view["checks"][0]["last"];
    assert_eq!(last["outcome"], "failed", "{view}");
    assert_eq!(
        std::fs::read_to_string(&ran).unwrap_or_default(),
        "test\n",
        "the outside cargo was asked to fetch, or did not run the check"
    );
    let why = last["fetch_skipped"]
        .as_str()
        .unwrap_or_else(|| panic!("the sheet does not say the owned fetch was skipped: {view}"));
    assert!(why.contains("cargo"), "{why}");
    assert!(why.contains(&program.display().to_string()), "{why}");
    assert!(why.contains("would allow"), "{why}");
    assert!(nothing_reads_passed(&state, temp.path(), &id).is_empty());
}

/* ── ADV-run-security-3: the cargo rustup's proxy hands off to ────────── */

/// The folder holding this machine's rustup and the `cargo` proxy beside it,
/// found on this test's own search path. It is only read and run: every rustup
/// these tests start is given a rustup home and a cargo home inside the test's
/// temporary folder ([`rustup_env`]), so no toolchain of the machine's is
/// looked up and nothing of rustup's is written. A machine without rustup
/// cannot run these tests, and they say so rather than pass.
#[cfg(unix)]
fn the_machines_rustup_proxies() -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let path = std::env::var_os("PATH").unwrap_or_default();
    let rustup = std::env::split_paths(&path)
        .map(|dir| dir.join("rustup"))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| {
            panic!(
                "could not run: no rustup on this test's search path, and these tests are about \
                 the cargo rustup hands off to"
            )
        });
    let folder = rustup.parent().expect("rustup's folder").to_path_buf();
    let same = |a: &Path, b: &Path| match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
        _ => false,
    };
    assert!(
        same(&folder.join("cargo"), &rustup),
        "could not run: the cargo beside rustup at {} is not rustup's proxy",
        folder.display()
    );
    folder
}

/// A rustup home and a cargo home of this test's own, empty, and no toolchain
/// inherited: `cargo test` runs this test through rustup, which names its own
/// toolchain in `RUSTUP_TOOLCHAIN`, and a run must not borrow it. Nothing
/// rustup starts here may install anything or reach a server.
#[cfg(unix)]
fn rustup_env(temp: &Path) -> Vec<(std::ffi::OsString, Option<std::ffi::OsString>)> {
    let rustup_home = temp.join("rustup-home");
    let cargo_home = temp.join("cargo-home");
    std::fs::create_dir_all(&rustup_home).expect("rustup home");
    std::fs::create_dir_all(&cargo_home).expect("cargo home");
    vec![
        ("RUSTUP_HOME".into(), Some(rustup_home.into_os_string())),
        ("CARGO_HOME".into(), Some(cargo_home.into_os_string())),
        ("RUSTUP_TOOLCHAIN".into(), None),
        ("RUSTUP_AUTO_INSTALL".into(), Some("0".into())),
        (
            "RUSTUP_DIST_SERVER".into(),
            Some("file:///nonexistent-dist".into()),
        ),
        (
            "RUSTUP_UPDATE_ROOT".into(),
            Some("file:///nonexistent-update".into()),
        ),
    ]
}

/// A state whose HQ search path is rustup's proxies and the system folders,
/// starting the real allow-list (`cargo test`) with `env` applied.
#[cfg(unix)]
fn rustup_state(
    temp: &Path,
    proxies: &Path,
    env: Vec<(std::ffi::OsString, Option<std::ffi::OsString>)>,
) -> Arc<State> {
    Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths([
                proxies.to_path_buf(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ])
            .expect("path"),
            temp.join("home"),
        )
        .with_env(env),
        ..Arc::try_unwrap(state_with(temp, None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    })
}

/// Allow, press Run with nothing picked, and hold what follows to the rule:
/// nothing reads a pass; the planted cargo never ran; the run was refused
/// before anything started, naming cargo and the program rustup would have
/// handed it to (a path ending in `handed`), not the proxy HQ found; nothing
/// is recorded or remembered; and the sheet does not say a run from HQ's own
/// search path would start cargo.
#[cfg(unix)]
fn run_through_rustup_is_refused(
    form: &str,
    state: &Arc<State>,
    temp: &Path,
    id: &str,
    marker: &Path,
    handed: &str,
) {
    grant(state, id);
    let r = post(
        state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(state),
    );
    wait_idle(state, id);
    let passed = nothing_reads_passed(state, temp, id);
    assert!(
        passed.is_empty(),
        "{form}: a cargo in the project, reached through rustup's proxy, was recorded as a pass: \
         {passed:?} — start answered {r}"
    );
    assert!(
        !marker.exists(),
        "{form}: the planted cargo ran: {}",
        std::fs::read_to_string(marker).unwrap_or_default()
    );
    assert_eq!(status(&r), 403, "{form}: {r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "program_in_project", "{form}: {refusal}");
    assert_eq!(refusal["program"], "cargo", "{form}: {refusal}");
    assert!(
        refusal["found_at"]
            .as_str()
            .is_some_and(|at| at.ends_with(handed)),
        "{form}: the refusal names the program rustup would have started (…{handed}): {refusal}"
    );
    assert!(
        executions(temp, id).is_empty(),
        "{form}: a refused run left an execution record"
    );
    let view = response_json(&get(state, &format!("/api/changes/{id}/run"), ""));
    assert!(
        view["remembered_environment"].is_null(),
        "{form}: a refused environment was remembered for the next press: {view}"
    );
    let hq = view["environments"]
        .as_array()
        .expect("environments")
        .iter()
        .find(|e| e["id"] == "hq")
        .cloned()
        .expect("HQ's own environment is always listed");
    assert_eq!(
        hq["has"],
        serde_json::json!([]),
        "{form}: the sheet says a run from HQ's search path would start cargo: {view}"
    );
}

/// **ADV-run-security-3 (D-038 pre-gate, the F2 variant V2b).** The `cargo`
/// HQ finds is rustup's proxy, outside the project — and the proxy is not the
/// program that runs. It reads the project's own toolchain file and hands off
/// to the toolchain that file names, and a `path` toolchain can be a folder the
/// repository wrote. Every form the repository can write is refused before
/// anything starts, and nothing reads a pass: `rust-toolchain.toml` naming a
/// folder in the project (the pre-gate's reproduction); the older
/// `rust-toolchain` file naming it as a bare path; and a toolchain file naming
/// a folder OUTSIDE the project that is a link back into it.
#[cfg(unix)]
#[test]
fn a_toolchain_file_handing_cargo_to_the_project_is_never_recorded_passed() {
    let proxies = the_machines_rustup_proxies();
    for form in ["rust-toolchain.toml", "rust-toolchain", "linked"] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let marker = outside.join("LYING-BINARY-RAN.txt");
        let toolchain = root.join(".fake-toolchain");
        let planted = plant_lying_cargo(&toolchain.join("bin"), &marker);
        let (named, handed) = if form == "linked" {
            let link = outside.join("linked-toolchain");
            std::os::unix::fs::symlink(&toolchain, &link).expect("link back into the project");
            (link, "linked-toolchain/bin/cargo")
        } else {
            (toolchain.clone(), ".fake-toolchain/bin/cargo")
        };
        assert_eq!(
            named.join("bin/cargo").canonicalize().expect("resolves"),
            planted,
            "{form}: the toolchain named must lead to the planted cargo, or this proves nothing"
        );
        if form == "rust-toolchain" {
            std::fs::write(
                root.join("rust-toolchain"),
                format!("{}\n", named.display()),
            )
            .expect("toolchain file");
        } else {
            std::fs::write(
                root.join("rust-toolchain.toml"),
                format!("[toolchain]\npath = \"{}\"\n", named.display()),
            )
            .expect("toolchain file");
        }
        let state = rustup_state(temp.path(), &proxies, rustup_env(temp.path()));
        run_through_rustup_is_refused(form, &state, temp.path(), &id, &marker, handed);
    }
}

/// **ADV-run-security-3, the routes that are not in the project's folder.**
/// rustup also takes its toolchain from `RUSTUP_TOOLCHAIN` in the environment
/// HQ was started with, from a directory override in its own settings, and
/// from a toolchain file in a folder ABOVE the project (the repository's own
/// when the recorded folder is a part of it). Any of them can name a toolchain
/// that is the project's — directly, or through a toolchain linked into it.
/// The guard asks rustup which cargo a run would start rather than reading any
/// one file, so each is refused the same way.
#[cfg(unix)]
#[test]
fn a_rustup_override_handing_cargo_to_the_project_is_refused_too() {
    let proxies = the_machines_rustup_proxies();
    for form in ["RUSTUP_TOOLCHAIN", "directory override", "a folder above"] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let marker = outside.join("LYING-BINARY-RAN.txt");
        let toolchain = root.join(".fake-toolchain");
        plant_lying_cargo(&toolchain.join("bin"), &marker);
        let mut env = rustup_env(temp.path());
        let handed = if form == "RUSTUP_TOOLCHAIN" {
            env.push((
                "RUSTUP_TOOLCHAIN".into(),
                Some(toolchain.clone().into_os_string()),
            ));
            ".fake-toolchain/bin/cargo"
        } else if form == "a folder above" {
            // The project's parent is this test's own temporary folder.
            std::fs::write(
                root.parent()
                    .expect("the folder above")
                    .join("rust-toolchain.toml"),
                format!("[toolchain]\npath = \"{}\"\n", toolchain.display()),
            )
            .expect("toolchain file");
            ".fake-toolchain/bin/cargo"
        } else {
            let rustup_home = temp.path().join("rustup-home");
            std::fs::create_dir_all(rustup_home.join("toolchains")).expect("toolchains");
            std::os::unix::fs::symlink(&toolchain, rustup_home.join("toolchains/linked"))
                .expect("a toolchain linked into the project");
            std::fs::write(
                rustup_home.join("settings.toml"),
                format!(
                    "version = \"12\"\n\n[overrides]\n\"{}\" = \"linked\"\n",
                    root.display()
                ),
            )
            .expect("settings");
            "toolchains/linked/bin/cargo"
        };
        let state = rustup_state(temp.path(), &proxies, env);
        run_through_rustup_is_refused(form, &state, temp.path(), &id, &marker, handed);
    }
}

/// **ADV-run-security-3, what still runs.** A project that pins its toolchain
/// to one outside the project is honoured, not overridden: rustup is asked,
/// names the pinned toolchain's cargo, and that cargo runs and reads passed.
/// Both ways a toolchain file pins: a `path` to a folder outside the project,
/// and a toolchain by name that is installed in rustup's own home.
#[cfg(unix)]
#[test]
fn a_toolchain_pinned_outside_the_project_still_runs() {
    use std::os::unix::fs::PermissionsExt;
    let proxies = the_machines_rustup_proxies();
    for form in ["path", "name"] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let ran = outside.join("PINNED-CARGO-RAN.txt");
        let env = rustup_env(temp.path());
        let (pinned, file) = if form == "path" {
            let pinned = outside.join("pinned-toolchain");
            let file = format!("[toolchain]\npath = \"{}\"\n", pinned.display());
            (pinned, file)
        } else {
            let pinned = temp.path().join("rustup-home/toolchains/pinned");
            (pinned, "[toolchain]\nchannel = \"pinned\"\n".to_string())
        };
        std::fs::create_dir_all(pinned.join("bin")).expect("pinned toolchain");
        let cargo = pinned.join("bin/cargo");
        std::fs::write(
            &cargo,
            format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 0.0.0-pinned'; exit 0; fi\n\
                 echo \"ran with: $*\" >> '{}'\n\
                 echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n",
                ran.display()
            ),
        )
        .expect("pinned cargo");
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        std::fs::write(root.join("rust-toolchain.toml"), file).expect("toolchain file");
        let state = rustup_state(temp.path(), &proxies, env);
        grant(&state, &id);
        let r = post(
            &state,
            &format!("/api/changes/{id}/run"),
            r#"{"check":"cargo-test"}"#,
            &writer(&state),
        );
        assert_eq!(
            status(&r),
            202,
            "{form}: a pinned toolchain outside the project was refused: {r}"
        );
        let view = wait_idle(&state, &id);
        assert_eq!(
            view["checks"][0]["last"]["outcome"], "passed",
            "{form}: {view}"
        );
        assert_eq!(
            std::fs::read_to_string(&ran).unwrap_or_default(),
            "ran with: test\n",
            "{form}: the run did not go to the pinned toolchain's cargo"
        );
    }
}

/// **ADV-run-security-3, when rustup cannot say.** A toolchain file naming a
/// toolchain that is not installed on this machine: rustup names no cargo it
/// would start without installing one, and asking it never installs. Nothing
/// runs, nothing is installed, nothing reads a pass, and the run reads "tool
/// not found", as any check whose program is not here does.
#[cfg(unix)]
#[test]
fn a_toolchain_rustup_cannot_name_here_runs_nothing_and_installs_nothing() {
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    std::fs::write(
        root.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"not-installed-here\"\n",
    )
    .expect("toolchain file");
    let state = rustup_state(temp.path(), &proxies, rustup_env(temp.path()));
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    wait_idle(&state, &id);
    assert_eq!(response_json(&r)["outcome"], "tool_not_found", "{r}");
    let passed = nothing_reads_passed(&state, temp.path(), &id);
    assert!(passed.is_empty(), "{passed:?}");
    assert!(
        !temp.path().join("rustup-home/toolchains").exists(),
        "asking rustup installed a toolchain"
    );
}

/* ── ADV-run-security-5: what rustup reads, and how long it is waited for ── */

/// Poll `ready` every 25 ms until it holds or `bound` passes. True when it held.
fn wait_until(bound: Duration, mut ready: impl FnMut() -> bool) -> bool {
    let deadline = std::time::Instant::now() + bound;
    loop {
        if ready() {
            return true;
        }
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `request` against the route on a thread of its own, waited for at most
/// `bound`. When it has not answered by then, whatever is blocked reading the
/// FIFO at `fifo` (if any) is given a harmless line so the thread can end, and
/// the test fails: a guard that waits on a file the repository wrote has
/// already lost.
#[cfg(unix)]
fn answered_within(
    bound: Duration,
    fifo: Option<&Path>,
    request: impl FnOnce() -> String + Send + 'static,
) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        let _ = tx.send(request());
    });
    match rx.recv_timeout(bound) {
        Ok(response) => {
            worker.join().expect("the request thread");
            response
        }
        Err(_) => {
            let released = fifo.is_some_and(release_fifo);
            panic!(
                "the route did not answer within {bound:?}: a guard is waiting on something the \
                 repository controls{}",
                if released {
                    " (a reader was blocked on the FIFO, and was released)"
                } else {
                    ""
                }
            );
        }
    }
}

/// Does anything have `fifo` open for reading — a rustup blocked on it, say?
/// A writer that does not wait is refused (`ENXIO`) exactly when nothing does.
#[cfg(unix)]
fn fifo_has_a_reader(fifo: &Path) -> bool {
    use rustix::fs::{open, Mode, OFlags};
    match open(
        fifo,
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(_) => true,
        Err(e) => {
            assert_eq!(
                e,
                rustix::io::Errno::NXIO,
                "the FIFO could not be probed: {e}"
            );
            false
        }
    }
}

/// Give whatever is blocked reading `fifo` a harmless toolchain line and end
/// its read. True when there was a reader to release.
#[cfg(unix)]
fn release_fifo(fifo: &Path) -> bool {
    use rustix::fs::{open, Mode, OFlags};
    use std::io::Write as _;
    match open(
        fifo,
        OFlags::WRONLY | OFlags::NONBLOCK | OFlags::CLOEXEC,
        Mode::empty(),
    ) {
        Ok(fd) => {
            let _ = std::fs::File::from(fd).write_all(b"[toolchain]\nchannel = \"released\"\n");
            true
        }
        Err(_) => false,
    }
}

/// A FIFO at `at`, made by the system's `mkfifo`, inside the test's folder.
#[cfg(unix)]
fn make_fifo(at: &Path) {
    let made = std::process::Command::new("mkfifo")
        .arg(at)
        .status()
        .expect("mkfifo runs")
        .success();
    assert!(made, "mkfifo {}", at.display());
}

/// A `rustup` of this test's own in `dir`, with `cargo` a link to it, so the
/// guard recognises `cargo` as rustup's proxy: as `rustup` it runs `as_rustup`,
/// and as `cargo` it hands on to the machine's own proxy in `proxies`.
#[cfg(unix)]
fn a_rustup_of_our_own(dir: &Path, proxies: &Path, as_rustup: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    std::fs::create_dir_all(dir).expect("the rustup's folder");
    let rustup = dir.join("rustup");
    std::fs::write(
        &rustup,
        format!(
            "#!/bin/sh\ncase \"${{0##*/}}\" in\n  rustup)\n{as_rustup}\n    ;;\n  *) exec '{}' \"$@\" ;;\nesac\n",
            proxies.join("cargo").display()
        ),
    )
    .expect("the rustup script");
    std::fs::set_permissions(&rustup, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    std::os::unix::fs::symlink(&rustup, dir.join("cargo")).expect("cargo, a link to it");
    dir.to_path_buf()
}

/// A state whose HQ search path is `bin` and the system folders, starting the
/// real allow-list (`cargo test`), with rustup given a home in the test's folder.
#[cfg(unix)]
fn state_on(temp: &Path, bin: &Path) -> Arc<State> {
    rustup_state(temp, bin, rustup_env(temp))
}

/// **ADV-run-security-5 (D-038 r1, verifier reason 1).** The guard of
/// ADV-run-security-3 asks rustup which cargo a run would start, and rustup
/// answers by READING the project's toolchain file. A repository that makes
/// that file a FIFO made rustup wait for a writer that never came: the run
/// sheet and Run never answered, and every view left another blocked rustup
/// behind. So every toolchain file rustup would read — `rust-toolchain` and
/// `rust-toolchain.toml`, in the project and every folder above it — is looked
/// at first without opening it, and anything there that is not a plain file of
/// a toolchain file's size refuses the run before rustup is asked and before
/// anything runs. Five forms: a FIFO in the project; a FIFO in the folder
/// above; a link; a folder; a file far larger than any toolchain file. Each
/// answers, refuses, asks rustup nothing, leaves nothing reading the FIFO, and
/// nothing reads a pass.
#[cfg(unix)]
#[test]
fn a_toolchain_file_that_is_not_a_plain_file_refuses_the_run_before_rustup_is_asked() {
    let proxies = the_machines_rustup_proxies();
    for form in [
        "a FIFO",
        "a FIFO in the folder above",
        "a link",
        "a folder",
        "too large",
    ] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let asked = outside.join("RUSTUP-WAS-ASKED.txt");
        let bin = a_rustup_of_our_own(
            &outside.join("rustup-bin"),
            &proxies,
            &format!(
                "    echo \"asked: $*\" >> '{}'\n    exec '{}' \"$@\"",
                asked.display(),
                proxies.join("rustup").display()
            ),
        );
        // A toolchain outside the project whose cargo says it passed: what a
        // guard that did not look at the file would have run.
        let ran = outside.join("PINNED-CARGO-RAN.txt");
        let pinned = outside.join("pinned-toolchain");
        plant_lying_cargo(&pinned.join("bin"), &ran);
        let pin = format!("[toolchain]\npath = \"{}\"\n", pinned.display());
        let file = if form == "a FIFO in the folder above" {
            root.parent()
                .expect("the folder above")
                .join("rust-toolchain.toml")
        } else {
            root.join("rust-toolchain.toml")
        };
        match form {
            "a FIFO" | "a FIFO in the folder above" => make_fifo(&file),
            "a link" => {
                let target = outside.join("toolchain-file.toml");
                std::fs::write(&target, &pin).expect("the linked file");
                std::os::unix::fs::symlink(&target, &file).expect("the link");
            }
            "a folder" => std::fs::create_dir(&file).expect("the folder"),
            _ => {
                let mut text = pin.clone();
                while text.len() <= 256 * 1024 {
                    text.push_str("# padding a toolchain file never needs\n");
                }
                std::fs::write(&file, text).expect("the large file");
            }
        }
        let fifo = form.starts_with("a FIFO").then(|| file.clone());
        let state = state_on(temp.path(), &bin);
        grant(&state, &id);

        let sheet = format!("/api/changes/{id}/run");
        let view = {
            let state = Arc::clone(&state);
            let sheet = sheet.clone();
            answered_within(Duration::from_secs(20), fifo.as_deref(), move || {
                get(&state, &sheet, "")
            })
        };
        let view = response_json(&view);
        let hq = view["environments"]
            .as_array()
            .expect("environments")
            .iter()
            .find(|e| e["id"] == "hq")
            .cloned()
            .expect("HQ's own environment is always listed");
        assert_eq!(
            hq["has"],
            serde_json::json!([]),
            "{form}: the sheet offers cargo past a toolchain file that is not a plain file: {view}"
        );

        let r = {
            let state = Arc::clone(&state);
            let run = sheet.clone();
            answered_within(Duration::from_secs(20), fifo.as_deref(), move || {
                post(&state, &run, r#"{"check":"cargo-test"}"#, &writer(&state))
            })
        };
        wait_idle(&state, &id);
        assert_eq!(status(&r), 403, "{form}: {r}");
        let refusal = response_json(&r);
        let expected = if form == "a FIFO in the folder above" {
            "not_runnable_from_here"
        } else {
            "program_in_project"
        };
        assert_eq!(refusal["error"], expected, "{form}: {refusal}");
        assert_eq!(refusal["program"], "cargo", "{form}: {refusal}");
        assert!(
            refusal["found_at"]
                .as_str()
                .is_some_and(|at| at.ends_with("rust-toolchain.toml")),
            "{form}: the refusal names the toolchain file: {refusal}"
        );
        assert!(
            !asked.exists(),
            "{form}: rustup was asked about a toolchain file that is not a plain file: {}",
            std::fs::read_to_string(&asked).unwrap_or_default()
        );
        if let Some(fifo) = &fifo {
            assert!(
                !fifo_has_a_reader(fifo),
                "{form}: something is still blocked reading the FIFO"
            );
        }
        assert!(!ran.exists(), "{form}: a cargo ran");
        assert!(
            executions(temp.path(), &id).is_empty(),
            "{form}: a refused run left an execution record"
        );
        let passed = nothing_reads_passed(&state, temp.path(), &id);
        assert!(passed.is_empty(), "{form}: {passed:?}");
    }
}

/// Every pid a script wrote into `file`, one per line.
fn pids_in(file: &Path) -> Vec<i32> {
    std::fs::read_to_string(file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect()
}

/// Is a process with this pid still in the process table?
#[cfg(unix)]
fn alive(pid: i32) -> bool {
    rustix::process::Pid::from_raw(pid)
        .is_some_and(|pid| rustix::process::test_kill_process(pid).is_ok())
}

/// A rustup that never answers: it writes its own pid and its child's, and
/// waits a minute on the child.
#[cfg(unix)]
fn a_rustup_that_never_answers(temp: &Path, proxies: &Path, pids: &Path) -> PathBuf {
    a_rustup_of_our_own(
        &temp.join("slow-rustup"),
        proxies,
        &format!(
            "    echo $$ >> '{p}'\n    sleep 60 &\n    echo $! >> '{p}'\n    wait",
            p = pids.display()
        ),
    )
}

/// **ADV-run-security-5, the deadline.** However the question to rustup is
/// kept from answering — a file the first look cannot see, a slow machine, a
/// rustup that hangs — the run sheet and Run must still answer, fail closed,
/// and leave nothing behind. rustup is asked in a process group of its own and
/// given a few seconds; past them the whole group is killed and reaped, and
/// the run is refused before anything starts. Asserted by pid: the rustup and
/// the child it started are both gone.
#[cfg(unix)]
#[test]
fn a_rustup_that_does_not_answer_in_time_is_stopped_and_nothing_runs() {
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let pids = outside.join("RUSTUP-PIDS.txt");
    let bin = a_rustup_that_never_answers(&outside, &proxies, &pids);
    let state = state_on(temp.path(), &bin);
    grant(&state, &id);

    let started = std::time::Instant::now();
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    let took = started.elapsed();
    assert!(
        took < Duration::from_secs(20),
        "Run waited {took:?} on a rustup that never answers: {r}"
    );
    assert_eq!(status(&r), 403, "{r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "not_runnable_from_here", "{refusal}");
    assert_eq!(refusal["why"], "rustup_did_not_answer", "{refusal}");
    assert_eq!(refusal["program"], "cargo", "{refusal}");
    let asked = pids_in(&pids);
    assert_eq!(asked.len(), 2, "the rustup and its child: {asked:?}");
    assert!(
        wait_until(Duration::from_secs(10), || !asked.iter().any(|p| alive(*p))),
        "a process of the question outlived its deadline: {:?}",
        asked.iter().filter(|p| alive(**p)).collect::<Vec<_>>()
    );
    assert!(
        executions(temp.path(), &id).is_empty(),
        "a refused run left an execution record"
    );

    // The sheet asks too, and answers the same way: no cargo offered, and
    // nothing of its question left.
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    let hq = view["environments"]
        .as_array()
        .expect("environments")
        .iter()
        .find(|e| e["id"] == "hq")
        .cloned()
        .expect("HQ's own environment is always listed");
    assert_eq!(hq["has"], serde_json::json!([]), "{view}");
    let asked = pids_in(&pids);
    assert!(asked.len() >= 4, "the sheet asked rustup too: {asked:?}");
    assert!(
        wait_until(Duration::from_secs(10), || !asked.iter().any(|p| alive(*p))),
        "a process of the sheet's question outlived its deadline: {:?}",
        asked.iter().filter(|p| alive(**p)).collect::<Vec<_>>()
    );
    let passed = nothing_reads_passed(&state, temp.path(), &id);
    assert!(passed.is_empty(), "{passed:?}");
}

/// **ADV-run-security-5, HQ stopping.** A question still waiting on rustup
/// when HQ stops must not outlive HQ: the verifier found one left behind with
/// parent 1. HQ's stop ([`RunRoute::stop_questions`], called as the server
/// loop ends) kills every question still going with its whole group, and
/// rustup is not asked again. Here rustup is given two minutes, so only the
/// stop can end the question within the test's bound; asserted by pid.
#[cfg(unix)]
#[test]
fn hq_stopping_ends_a_question_to_rustup_that_is_still_going() {
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let pids = outside.join("RUSTUP-PIDS.txt");
    let bin = a_rustup_that_never_answers(&outside, &proxies, &pids);
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths([
                bin.clone(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ])
            .expect("path"),
            temp.path().join("home"),
        )
        .with_env(rustup_env(temp.path()))
        .with_rustup_deadline(Duration::from_secs(120)),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);

    let sheet = format!("/api/changes/{id}/run");
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let state = Arc::clone(&state);
        let sheet = sheet.clone();
        std::thread::spawn(move || {
            let _ = tx.send(get(&state, &sheet, ""));
        });
    }
    assert!(
        wait_until(Duration::from_secs(20), || pids_in(&pids).len() >= 2),
        "the sheet never asked rustup: {:?}",
        pids_in(&pids)
    );
    let asked = pids_in(&pids);
    assert!(
        asked.iter().all(|p| alive(*p)),
        "the question ended before HQ stopped: {asked:?}"
    );

    state.run_route.stop_questions();
    assert!(
        wait_until(Duration::from_secs(10), || !asked.iter().any(|p| alive(*p))),
        "a question to rustup outlived HQ's stop: {:?}",
        asked.iter().filter(|p| alive(**p)).collect::<Vec<_>>()
    );
    let view = response_json(
        &rx.recv_timeout(Duration::from_secs(20))
            .expect("the sheet answered once its question was ended"),
    );
    let hq = view["environments"]
        .as_array()
        .expect("environments")
        .iter()
        .find(|e| e["id"] == "hq")
        .cloned()
        .expect("HQ's own environment is always listed");
    assert_eq!(hq["has"], serde_json::json!([]), "{view}");

    // After the stop, rustup is not asked again.
    let view = response_json(&get(&state, &sheet, ""));
    assert_eq!(
        pids_in(&pids),
        asked,
        "rustup was asked after HQ stopped: {view}"
    );
}

/* ── ADV-run-security-6: what rustup reads, read as data ───────────────── */

/// The target triple rustup calls this machine by, which it adds to a bare
/// channel name (`stable` names `stable-<host>`).
fn host_triple() -> &'static str {
    match (std::env::consts::ARCH, std::env::consts::OS) {
        ("aarch64", "macos") => "aarch64-apple-darwin",
        ("x86_64", "macos") => "x86_64-apple-darwin",
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu",
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu",
        (arch, os) => panic!("could not run: no rustup host triple is known here for {arch} {os}"),
    }
}

/// A cargo home of the test's own whose `rustup` and `cargo` are two separate
/// byte copies of the machine's rustup — what rustup installs when it cannot
/// link its proxies — so `cargo` is rustup's proxy, and no file of the same
/// identity is anywhere near it.
#[cfg(unix)]
fn a_cargo_home_of_byte_copies(temp: &Path, proxies: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let bin = temp.join("cargo-home/bin");
    std::fs::create_dir_all(&bin).expect("the cargo home");
    for name in ["rustup", "cargo"] {
        std::fs::copy(proxies.join("rustup"), bin.join(name)).expect("a byte copy of rustup");
    }
    let (a, b) = (
        std::fs::metadata(bin.join("rustup")).expect("rustup"),
        std::fs::metadata(bin.join("cargo")).expect("cargo"),
    );
    assert!(
        (a.dev(), a.ino()) != (b.dev(), b.ino())
            && std::fs::read(bin.join("rustup")).expect("rustup")
                == std::fs::read(bin.join("cargo")).expect("cargo"),
        "the proxy must be a byte copy, not a link, or this proves nothing"
    );
    bin
}

/// **ADV-run-security-6 (D-038 r1, the residual r1 disclosed and the verifier
/// measured).** The guard recognises rustup's proxy by file identity, so a
/// `cargo` that is a byte COPY of rustup — which rustup itself installs when
/// it cannot link — was taken for the program itself, rustup was never asked,
/// and the project's planted toolchain ran and read passed. What rustup reads
/// is now read as data too, whether or not the proxy is recognised: the
/// toolchain files in the project and every folder above it (plain files only,
/// read to a bound), `RUSTUP_TOOLCHAIN` in HQ's own environment, and the
/// directory overrides and default toolchain in rustup's `settings.toml`. A
/// path there that is in the project or links into it — or a toolchain name
/// whose installed folder does — refuses the run. Nine forms, each through a
/// byte-copied proxy.
#[cfg(unix)]
#[test]
fn a_proxy_that_is_a_byte_copy_of_rustup_is_held_to_the_same_rule() {
    let proxies = the_machines_rustup_proxies();
    let stable = format!("stable-{}", host_triple());
    for form in [
        "rust-toolchain.toml",
        "rust-toolchain",
        "linked",
        "RUSTUP_TOOLCHAIN",
        "directory override",
        "a directory override above",
        "a folder above",
        "a channel rustup expands",
        "the default toolchain",
    ] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let marker = outside.join("LYING-BINARY-RAN.txt");
        let toolchain = root.join(".fake-toolchain");
        plant_lying_cargo(&toolchain.join("bin"), &marker);
        let bin = a_cargo_home_of_byte_copies(temp.path(), &proxies);
        let mut env = rustup_env(temp.path());
        let rustup_home = temp.path().join("rustup-home");
        let link_toolchain = |name: &str| {
            std::fs::create_dir_all(rustup_home.join("toolchains")).expect("toolchains");
            std::os::unix::fs::symlink(&toolchain, rustup_home.join("toolchains").join(name))
                .expect("a toolchain linked into the project");
        };
        let toml_path = |at: &Path| format!("[toolchain]\npath = \"{}\"\n", at.display());
        let handed = match form {
            "rust-toolchain.toml" => {
                std::fs::write(root.join("rust-toolchain.toml"), toml_path(&toolchain))
                    .expect("toolchain file");
                ".fake-toolchain/bin/cargo".to_string()
            }
            "rust-toolchain" => {
                std::fs::write(
                    root.join("rust-toolchain"),
                    format!("{}\n", toolchain.display()),
                )
                .expect("toolchain file");
                ".fake-toolchain/bin/cargo".to_string()
            }
            "linked" => {
                let link = outside.join("linked-toolchain");
                std::os::unix::fs::symlink(&toolchain, &link).expect("link back into the project");
                std::fs::write(root.join("rust-toolchain.toml"), toml_path(&link))
                    .expect("toolchain file");
                "linked-toolchain/bin/cargo".to_string()
            }
            "RUSTUP_TOOLCHAIN" => {
                env.push((
                    "RUSTUP_TOOLCHAIN".into(),
                    Some(toolchain.clone().into_os_string()),
                ));
                ".fake-toolchain/bin/cargo".to_string()
            }
            "directory override" | "a directory override above" => {
                link_toolchain("linked");
                let folder = if form == "directory override" {
                    root.clone()
                } else {
                    root.parent().expect("the folder above").to_path_buf()
                };
                std::fs::write(
                    rustup_home.join("settings.toml"),
                    format!(
                        "version = \"12\"\n\n[overrides]\n\"{}\" = \"linked\"\n",
                        folder.display()
                    ),
                )
                .expect("settings");
                "toolchains/linked/bin/cargo".to_string()
            }
            "a folder above" => {
                std::fs::write(
                    root.parent()
                        .expect("the folder above")
                        .join("rust-toolchain.toml"),
                    toml_path(&toolchain),
                )
                .expect("toolchain file");
                ".fake-toolchain/bin/cargo".to_string()
            }
            "a channel rustup expands" => {
                link_toolchain(&stable);
                std::fs::write(
                    root.join("rust-toolchain.toml"),
                    "[toolchain]\nchannel = \"stable\"\n",
                )
                .expect("toolchain file");
                format!("toolchains/{stable}/bin/cargo")
            }
            _ => {
                link_toolchain("linked");
                std::fs::write(
                    rustup_home.join("settings.toml"),
                    "version = \"12\"\ndefault_toolchain = \"linked\"\n",
                )
                .expect("settings");
                "toolchains/linked/bin/cargo".to_string()
            }
        };
        let state = rustup_state(temp.path(), &bin, env);
        run_through_rustup_is_refused(form, &state, temp.path(), &id, &marker, &handed);
    }
}

/// **ADV-run-security-6, the question still holds.** With what rustup reads
/// now checked as data first, every form the repository can write is refused
/// before rustup is asked. The question remains, for whatever rustup does that
/// the data does not show: a rustup (outside the project) whose answer is a
/// program in the project is refused the same way, naming that program.
#[cfg(unix)]
#[test]
fn whatever_rustup_answers_is_held_to_the_rule_too() {
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let marker = outside.join("LYING-BINARY-RAN.txt");
    let planted = plant_lying_cargo(&root.join(".fake-toolchain/bin"), &marker);
    let bin = a_rustup_of_our_own(
        &outside.join("rustup-bin"),
        &proxies,
        &format!("    echo '{}'", planted.display()),
    );
    let state = state_on(temp.path(), &bin);
    run_through_rustup_is_refused(
        "an answer in the project",
        &state,
        temp.path(),
        &id,
        &marker,
        ".fake-toolchain/bin/cargo",
    );
}

/// **ADV-run-security-6, rustup's own settings.** rustup reads its
/// `settings.toml` for every question and every run; read as data here too,
/// it is read the same careful way — never waited on — and one that is not a
/// plain file refuses the run before rustup is asked, naming it, with nothing
/// left reading. (Before this, rustup was asked, gave no answer, and the run
/// read as tool not found.)
#[cfg(unix)]
#[test]
fn rustups_settings_that_are_not_a_plain_file_refuse_the_run_before_it_is_asked() {
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let env = rustup_env(temp.path());
    let settings = temp.path().join("rustup-home/settings.toml");
    make_fifo(&settings);
    let state = rustup_state(temp.path(), &proxies, env);
    grant(&state, &id);
    let r = {
        let state = Arc::clone(&state);
        let run = format!("/api/changes/{id}/run");
        answered_within(Duration::from_secs(20), Some(&settings), move || {
            post(&state, &run, r#"{"check":"cargo-test"}"#, &writer(&state))
        })
    };
    assert_eq!(status(&r), 403, "{r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "not_runnable_from_here", "{refusal}");
    assert_eq!(refusal["why"], "rustup_settings_not_plain", "{refusal}");
    assert!(
        refusal["found_at"]
            .as_str()
            .is_some_and(|at| at.ends_with("rustup-home/settings.toml")),
        "{refusal}"
    );
    assert!(
        !fifo_has_a_reader(&settings),
        "something is still blocked reading rustup's settings"
    );
    assert!(executions(temp.path(), &id).is_empty());
    assert!(!outside.join("LYING-BINARY-RAN.txt").exists());
}

/* ── ADV-run-security-7: the toolchain checked is the toolchain that runs ── */

/// **ADV-run-security-7 (D-038 r1, residual (1): the time of the check against
/// the time of use).** The guard asks rustup which cargo a run would start;
/// the proxy decides again, a moment later, when the run starts. A live process
/// that rewrites the project's toolchain file in between could hand the run to
/// a toolchain the guard never saw. So the run is pinned to the toolchain the
/// guard checked: its child gets `RUSTUP_TOOLCHAIN` naming it, which rustup
/// puts before every toolchain file. Here the rustup HQ finds answers with the
/// machine's rustup and then — as a racing process would — rewrites the
/// project's toolchain file to name a toolchain the repository planted. The
/// run still starts the checked toolchain's cargo, and the planted one never
/// runs. Both ways a toolchain can be pinned: by a path outside the project,
/// and by the name of one installed in rustup's home.
#[cfg(unix)]
#[test]
fn the_toolchain_the_guard_checked_is_the_toolchain_the_run_starts() {
    use std::os::unix::fs::PermissionsExt;
    let proxies = the_machines_rustup_proxies();
    for form in ["path", "name"] {
        let temp = fixture_dir();
        let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
        let outside = outside_the_project(temp.path(), &root);
        let marker = outside.join("LYING-BINARY-RAN.txt");
        let planted = root.join(".fake-toolchain");
        plant_lying_cargo(&planted.join("bin"), &marker);
        let ran = outside.join("PINNED-CARGO-RAN.txt");
        let (pinned, file) = if form == "path" {
            let pinned = outside.join("pinned-toolchain");
            let file = format!("[toolchain]\npath = \"{}\"\n", pinned.display());
            (pinned, file)
        } else {
            let pinned = temp.path().join("rustup-home/toolchains/pinned");
            (pinned, "[toolchain]\nchannel = \"pinned\"\n".to_string())
        };
        std::fs::create_dir_all(pinned.join("bin")).expect("pinned toolchain");
        let cargo = pinned.join("bin/cargo");
        std::fs::write(
            &cargo,
            format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 0.0.0-pinned'; exit 0; fi\n\
                 echo \"ran with: $*\" >> '{}'\n\
                 echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n",
                ran.display()
            ),
        )
        .expect("pinned cargo");
        std::fs::set_permissions(&cargo, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        let toolchain_file = root.join("rust-toolchain.toml");
        std::fs::write(&toolchain_file, file).expect("toolchain file");
        let racing = outside.join("racing-toolchain.toml");
        std::fs::write(
            &racing,
            format!("[toolchain]\npath = \"{}\"\n", planted.display()),
        )
        .expect("the racing file");
        let bin = a_rustup_of_our_own(
            &outside.join("racing-rustup"),
            &proxies,
            &format!(
                "    '{}' \"$@\"\n    answered=$?\n    cp '{}' '{}'\n    exit $answered",
                proxies.join("rustup").display(),
                racing.display(),
                toolchain_file.display()
            ),
        );
        let state = state_on(temp.path(), &bin);
        grant(&state, &id);
        let r = post(
            &state,
            &format!("/api/changes/{id}/run"),
            r#"{"check":"cargo-test"}"#,
            &writer(&state),
        );
        assert_eq!(status(&r), 202, "{form}: {r}");
        let view = wait_idle(&state, &id);
        assert!(
            std::fs::read_to_string(&toolchain_file)
                .expect("the toolchain file")
                .contains(".fake-toolchain"),
            "{form}: the race did not happen, so this proves nothing"
        );
        assert!(
            !marker.exists(),
            "{form}: the toolchain file rewritten after the check handed the run to the planted cargo: {}",
            std::fs::read_to_string(&marker).unwrap_or_default()
        );
        assert_eq!(
            std::fs::read_to_string(&ran).unwrap_or_default(),
            "ran with: test\n",
            "{form}: the run did not go to the toolchain the guard checked: {view}"
        );
        assert_eq!(
            view["checks"][0]["last"]["outcome"], "passed",
            "{form}: {view}"
        );
    }
}

/// **ADV-run-security-7, an answer the guard cannot pin.** The pin is the
/// toolchain folder above `bin/cargo` in rustup's answer. An answer outside
/// the project that is not a program in a toolchain's `bin` folder cannot be
/// pinned, so the run it would start is not started: the guard says it cannot
/// tell, and nothing runs.
#[cfg(unix)]
#[test]
fn an_answer_the_guard_cannot_pin_to_a_toolchain_starts_nothing() {
    use std::os::unix::fs::PermissionsExt;
    let proxies = the_machines_rustup_proxies();
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let ran = outside.join("UNPINNED-CARGO-RAN.txt");
    let unpinned = outside.join("not-a-toolchain/cargo");
    std::fs::create_dir_all(unpinned.parent().expect("its folder")).expect("folder");
    std::fs::write(
        &unpinned,
        format!("#!/bin/sh\necho \"ran with: $*\" >> '{}'\n", ran.display()),
    )
    .expect("the program");
    std::fs::set_permissions(&unpinned, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let bin = a_rustup_of_our_own(
        &outside.join("rustup-bin"),
        &proxies,
        &format!("    echo '{}'", unpinned.display()),
    );
    let state = state_on(temp.path(), &bin);
    grant(&state, &id);
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    wait_idle(&state, &id);
    assert_eq!(status(&r), 403, "{r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "not_runnable_from_here", "{refusal}");
    assert_eq!(refusal["why"], "rustup_answer_not_a_toolchain", "{refusal}");
    assert!(
        executions(temp.path(), &id).is_empty(),
        "a refused run left an execution record"
    );
    assert!(!ran.exists(), "a cargo ran");
}

/* ── D-049: the refusal for a program inside the project, in words ─────── */

/// The sentence a person reads for a `program_in_project` refusal: the copy
/// file's own string (`ui/src/workspace/hq-copy.js`,
/// `run.refusals.program_in_project`), filled with the program the route
/// served, exactly as `run-route.js`'s `refusalText` fills it. Read from the
/// copy file, not written here, so this test cannot hold a sentence the
/// screen does not say.
fn program_in_project_sentence(program: &str) -> String {
    let copy = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/workspace/hq-copy.js"),
    )
    .expect("the copy file");
    let lines: Vec<&str> = copy
        .lines()
        .filter_map(|l| l.trim_start().strip_prefix("program_in_project: "))
        .collect();
    assert_eq!(
        lines.len(),
        1,
        "the copy file holds exactly one program_in_project sentence"
    );
    let literal = lines[0].trim_end().trim_end_matches(',');
    let template: String = if literal.starts_with('"') {
        serde_json::from_str(literal).expect("a double-quoted copy string")
    } else {
        literal
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .expect("a quoted copy string")
            .replace("\\'", "'")
    };
    template.replace("{tool}", program)
}

/// **D-049 (founder ruling 3).** Refusing a program that lives inside the
/// project is accepted for launch, and the refusal a person reads names the
/// program, says it was refused because it lives inside the project, and says
/// what would allow it. The route serves the reason and the program; the
/// sheet says the copy file's sentence filled with that program. This holds
/// both, word for word — and then does what the sentence says: installs the
/// program outside the project folder and picks that place. The run goes,
/// so the sentence tells the truth about what would allow it. The planted
/// program, the installed one and the home folder are all inside this test's
/// temporary folder.
#[cfg(unix)]
#[test]
fn the_program_in_project_refusal_names_the_program_says_why_and_says_what_would_allow_it() {
    use std::os::unix::fs::PermissionsExt;
    let temp = fixture_dir();
    let (id, root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let outside = outside_the_project(temp.path(), &root);
    let marker = outside.join("LYING-BINARY-RAN.txt");
    let planted_dir = root.join("node_modules/.bin");
    plant_lying_cargo(&planted_dir, &marker);
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).expect("home");
    // HQ was started with the project's own folder first on its search path.
    let state = Arc::new(State {
        run_route: RunRoute::with_machine(
            None,
            std::env::join_paths([
                planted_dir.clone(),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
            ])
            .expect("path"),
            home.clone(),
        ),
        ..Arc::try_unwrap(state_with(temp.path(), None))
            .unwrap_or_else(|_| panic!("the state is not shared yet"))
    });
    grant(&state, &id);

    // 1. The served refusal: the reason, and the program by name.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 403, "{r}");
    let refusal = response_json(&r);
    assert_eq!(refusal["error"], "program_in_project", "{refusal}");
    let program = refusal["program"]
        .as_str()
        .expect("the refusal names the program");
    assert_eq!(program, "cargo", "{refusal}");
    assert!(!marker.exists(), "the planted program ran");
    assert!(
        executions(temp.path(), &id).is_empty(),
        "a refused run left a record"
    );

    // 2. The sentence the sheet says for it, word for word.
    assert_eq!(
        program_in_project_sentence(program),
        "Nothing ran: the cargo this check would use is inside this project, where the \
         project's own files can put anything, so ThruLine refused to start it. To run this \
         check, install cargo outside the project folder, then press Look somewhere else and \
         pick that place."
    );

    // 3. What it says would allow it does: cargo installed outside the
    //    project folder, in the home folder, is offered as a place to run...
    let installed_dir = home.join(".local/bin");
    std::fs::create_dir_all(&installed_dir).expect("an outside folder");
    let installed = installed_dir.join("cargo");
    let ran = outside.join("INSTALLED-CARGO-RAN.txt");
    std::fs::write(
        &installed,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'cargo 0.0.0-outside'; exit 0; fi\n\
             echo \"ran with: $*\" >> '{}'\n\
             echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s'\n",
            ran.display()
        ),
    )
    .expect("the installed program");
    std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    let place = view["environments"]
        .as_array()
        .expect("environments")
        .iter()
        .find(|e| e["id"] == "home-local")
        .cloned()
        .expect("the folder cargo was installed in is offered as a place");
    assert_eq!(place["has"], serde_json::json!(["cargo"]), "{view}");

    // ...and picking that place runs the check there, and the project's own
    // copy never runs.
    let r = post(
        &state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test","environment":"home-local"}"#,
        &writer(&state),
    );
    assert_eq!(status(&r), 202, "{r}");
    let view = wait_idle(&state, &id);
    assert_eq!(view["checks"][0]["last"]["outcome"], "passed", "{view}");
    assert_eq!(
        std::fs::read_to_string(&ran).unwrap_or_default(),
        "ran with: test\n",
        "the cargo installed outside the project did not run"
    );
    assert!(!marker.exists(), "the planted program ran");
}

/* ── C30 R-RUN: what a running check has printed, and for how long ─────── */

/// Prints, then waits for the test to open a gate, twice — so the sheet is
/// read while the run is going, in an order the test sets and never by
/// timing. A gate never opened ends the run after about 20 s. Its last line is
/// pytest's own summary.
static GATED: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &[
        "-c",
        "echo 'first line'; echo 'to stderr' 1>&2; \
         n=0; while [ ! -e \"$RRUN_GATE/one\" ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done; \
         echo 'second line'; echo \"home $HOME/rrun-folder/file.txt\"; \
         echo 'token ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e'; \
         n=0; while [ ! -e \"$RRUN_GATE/two\" ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done; \
         echo '========== 5 passed in 0.12s =========='",
    ],
}];

/// Far more than the sheet may serve: 500 short lines, 40 wide ones, one of
/// 5,000 characters, then `last line` — and then it waits at the gate, so the
/// sheet is read while it runs.
static FLOOD: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &[
        "-c",
        "i=1; while [ $i -le 500 ]; do echo \"short $i\"; i=$((i+1)); done; \
         w=$(printf '%900s' '' | tr ' ' w); \
         i=1; while [ $i -le 40 ]; do echo \"wide $i $w\"; i=$((i+1)); done; \
         printf '%5000s\\n' '' | tr ' ' x; echo 'last line'; \
         n=0; while [ ! -e \"$RRUN_GATE/one\" ] && [ $n -lt 400 ]; do sleep 0.05; n=$((n+1)); done",
    ],
}];

/// pytest's own failing summary, and pytest's exit status for it.
static PYTEST_FAILS: &[RunnableCheck] = &[RunnableCheck {
    id: "cargo-test",
    program: "sh",
    args: &[
        "-c",
        "echo 'FAILED tests/test_api.py::test_total - assert 90 == 95'; \
         echo '=========== 1 failed, 4 passed in 0.31s ==========='; exit 1",
    ],
}];

/// A route starting `catalogue`, whose runs see `RRUN_GATE` naming `gate`.
fn gated_state(dir: &Path, catalogue: &'static [RunnableCheck], gate: &Path) -> Arc<State> {
    let base = test_state(dir, MockEngine::available());
    let state = Arc::try_unwrap(base).unwrap_or_else(|_| panic!("the state is not shared yet"));
    Arc::new(State {
        run_route: RunRoute::with_catalogue(catalogue).with_env(vec![(
            "RRUN_GATE".into(),
            Some(gate.as_os_str().to_os_string()),
        )]),
        ..state
    })
}

fn open_gate(gate: &Path, name: &str) {
    std::fs::write(gate.join(name), "").expect("open the gate");
}

/// The sheet, read until `ready` holds of it — bounded, and failing with the
/// last sheet it read.
fn sheet_until(
    state: &Arc<State>,
    id: &str,
    what: &str,
    ready: impl Fn(&serde_json::Value) -> bool,
) -> serde_json::Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    loop {
        let view = response_json(&get(state, &format!("/api/changes/{id}/run"), ""));
        if ready(&view) {
            return view;
        }
        assert!(std::time::Instant::now() < deadline, "{what}: {view}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// The lines the sheet serves of the running job's output.
fn printed(view: &serde_json::Value) -> Vec<String> {
    view["job"]["output"]["lines"]
        .as_array()
        .map(|lines| {
            lines
                .iter()
                .filter_map(|l| l.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn start_one(state: &Arc<State>, id: &str) {
    let r = post(
        state,
        &format!("/api/changes/{id}/run"),
        r#"{"check":"cargo-test"}"#,
        &writer(state),
    );
    assert_eq!(status(&r), 202, "{r}");
}

/// Every table's row count, read without writing.
fn row_counts(db: &Path) -> Vec<(String, i64)> {
    let conn =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .expect("read-only");
    let mut names = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .expect("tables");
    let names: Vec<String> = names
        .query_map([], |r| r.get::<_, String>(0))
        .expect("names")
        .map(|r| r.expect("a name"))
        .collect();
    names
        .into_iter()
        .map(|n| {
            let count: i64 = conn
                .query_row(&format!("SELECT COUNT(*) FROM \"{n}\""), [], |r| r.get(0))
                .expect("count");
            (n, count)
        })
        .collect()
}

/// **While a check runs, the sheet serves what it has printed so far, and it
/// grows** — both streams, each line masked and with the home folder as `~`,
/// with the masking said beside it. Once the run ends, none of it is served:
/// the job and its output go together, and the outcome is the record's
/// (here pytest's own count, through the wrapper's tally).
#[cfg(unix)]
#[test]
fn while_a_check_runs_the_sheet_serves_what_it_has_printed_and_it_grows() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let gate = temp.path().join("gate");
    std::fs::create_dir_all(&gate).expect("gate");
    let state = gated_state(temp.path(), GATED, &gate);
    grant(&state, &id);
    start_one(&state, &id);

    let first = sheet_until(
        &state,
        &id,
        "the first lines never reached the sheet",
        |v| {
            let lines = printed(v);
            lines.iter().any(|l| l == "first line") && lines.iter().any(|l| l == "to stderr")
        },
    );
    assert_eq!(first["schema_version"], 3, "{first}");
    assert_eq!(first["job"]["for_this_change"], true);
    assert_eq!(first["job"]["phase"], "running");
    assert_eq!(first["job"]["output"]["cut"], false, "{first}");
    let before = printed(&first);
    assert_eq!(before.len(), 2, "{first}");
    assert!(
        !before.iter().any(|l| l.contains("second line")),
        "{before:?}"
    );

    open_gate(&gate, "one");
    let second = sheet_until(&state, &id, "the output did not grow", |v| {
        printed(v).iter().any(|l| l.starts_with("token "))
    });
    let after = printed(&second);
    assert!(after.len() > before.len(), "{after:?}");
    assert_eq!(
        &after[..before.len()],
        &before[..],
        "lines moved: {after:?}"
    );
    assert!(after.contains(&"second line".to_string()), "{after:?}");

    // The home folder is `~`, and the token is masked — with the record of
    // that masking served beside the lines.
    assert!(
        after.contains(&"home ~/rrun-folder/file.txt".to_string()),
        "{after:?}"
    );
    if let Some(home) = std::env::var_os("HOME").filter(|h| h.len() > 1) {
        let home = home.to_string_lossy().to_string();
        assert!(
            !after.iter().any(|l| l.contains(&home)),
            "a home-folder path was served: {after:?}"
        );
    }
    assert!(
        !second.to_string().contains("ghp_TLCANARY"),
        "a credential was served: {second}"
    );
    let token_line = after
        .iter()
        .find(|l| l.starts_with("token "))
        .expect("the token line");
    assert!(token_line.contains("[masked"), "{token_line}");
    let masking = &second["job"]["output"]["masking"];
    assert!(masking["masked"].as_u64().unwrap_or(0) >= 1, "{masking}");
    assert!(
        masking["families"]
            .as_array()
            .is_some_and(|f| f.iter().any(|x| x == "github")),
        "{masking}"
    );
    assert_eq!(masking["complete"], true, "{masking}");

    open_gate(&gate, "two");
    let end = wait_idle(&state, &id);
    assert!(end["job"].is_null(), "{end}");
    // The check's own command (Technical face) quotes the script's words; the
    // printed lines must be nowhere else.
    let mut rest = end.clone();
    rest["checks"][0]["command"] = serde_json::Value::Null;
    let served = rest.to_string();
    for line in ["first line", "second line", "to stderr", "rrun-folder"] {
        assert!(
            !served.contains(line),
            "{line:?} outlived its run on the sheet: {end}"
        );
    }
    // The outcome is the record's: pytest's own count, read by the wrapper.
    let last = &end["checks"][0]["last"];
    assert_eq!(last["outcome"], "passed", "{last}");
    assert_eq!(last["tests_passed"], 5, "{last}");
    assert_eq!(last["tests_failed"], 0, "{last}");
}

/// **The elapsed time is the run's own**: from the start the wrapper recorded,
/// counting while the record says it runs — on the job and on the running
/// record alike — and stopped at the recorded end once it has ended.
#[cfg(unix)]
#[test]
fn the_sheet_serves_the_elapsed_time_from_the_runs_own_recorded_start() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let gate = temp.path().join("gate");
    std::fs::create_dir_all(&gate).expect("gate");
    let state = gated_state(temp.path(), GATED, &gate);
    grant(&state, &id);
    start_one(&state, &id);

    let a = sheet_until(&state, &id, "the run was never recorded", |v| {
        v["job"]["phase"] == "running" && !printed(v).is_empty()
    });
    let record = executions(temp.path(), &id)
        .into_iter()
        .next()
        .expect("the run's record");
    assert_eq!(
        a["job"]["recorded_started_at"].as_str(),
        record.started_at.as_deref(),
        "the start served is not the one the record holds: {a}"
    );
    let first = a["job"]["elapsed_s"].as_i64().expect("an elapsed time");
    assert!((0..=5).contains(&first), "{a}");
    let running = &a["checks"][0]["last"];
    assert_eq!(running["outcome"], "running", "{a}");
    let on_record = running["elapsed_s"].as_i64().expect("an elapsed time");
    assert!((on_record - first).abs() <= 1, "{a}");

    std::thread::sleep(Duration::from_millis(2200));
    let b = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    let later = b["job"]["elapsed_s"].as_i64().expect("an elapsed time");
    assert!(later >= first + 2, "{first} then {later}: {b}");

    open_gate(&gate, "one");
    open_gate(&gate, "two");
    let end = wait_idle(&state, &id);
    let last = &end["checks"][0]["last"];
    let stopped = last["elapsed_s"].as_i64().expect("the run's length");
    let started = last["started_at"].as_str().expect("start");
    let ended = last["recorded_at"].as_str().expect("end");
    let length = (thruline_core::hq::parse_ms(ended).expect("end")
        - thruline_core::hq::parse_ms(started).expect("start"))
        / 1000;
    assert_eq!(stopped, length, "{last}");
    std::thread::sleep(Duration::from_millis(1100));
    let again = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
    assert_eq!(
        again["checks"][0]["last"]["elapsed_s"], stopped,
        "an ended run's time kept counting: {again}"
    );
}

/// **What is served is bounded**: at most `LIVE_LINES` lines and `LIVE_BYTES`
/// bytes, no line longer than `LIVE_LINE_BYTES`, the newest kept, and `cut`
/// saying something printed is not there.
#[cfg(unix)]
#[test]
fn what_the_sheet_serves_of_a_running_check_is_bounded_and_keeps_the_newest() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let gate = temp.path().join("gate");
    std::fs::create_dir_all(&gate).expect("gate");
    let state = gated_state(temp.path(), FLOOD, &gate);
    grant(&state, &id);
    start_one(&state, &id);

    let view = sheet_until(&state, &id, "the flood never reached its end", |v| {
        printed(v).last().is_some_and(|l| l == "last line")
    });
    let lines = printed(&view);
    assert!(lines.len() <= crate::check::LIVE_LINES, "{}", lines.len());
    let bytes: usize = lines.iter().map(String::len).sum();
    assert!(bytes <= crate::check::LIVE_BYTES, "{bytes} bytes served");
    assert!(
        lines
            .iter()
            .all(|l| l.len() <= crate::check::LIVE_LINE_BYTES),
        "a line longer than the bound was served"
    );
    assert_eq!(
        view["job"]["output"]["cut"], true,
        "nothing said it was cut"
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("short ")),
        "the oldest lines were kept over the newest"
    );
    let long = &lines[lines.len() - 2];
    assert!(long.starts_with("xxxx"), "{long}");
    assert_eq!(long.len(), crate::check::LIVE_LINE_BYTES);
    assert!(lines.iter().any(|l| l.starts_with("wide 40 ")));

    open_gate(&gate, "one");
    wait_idle(&state, &id);
}

/// **Reading the sheet while a check runs writes nothing**: every table's row
/// count is the same before and after five reads.
#[cfg(unix)]
#[test]
fn reading_the_sheet_while_a_check_runs_writes_nothing() {
    let temp = fixture_dir();
    let (id, _root) = a_change_with_profile(temp.path(), &["cargo-test"]);
    let gate = temp.path().join("gate");
    std::fs::create_dir_all(&gate).expect("gate");
    let state = gated_state(temp.path(), GATED, &gate);
    grant(&state, &id);
    start_one(&state, &id);
    sheet_until(&state, &id, "the run never printed", |v| {
        !printed(v).is_empty()
    });

    let db = temp.path().join("ledger.db");
    let before = row_counts(&db);
    for _ in 0..5 {
        let view = response_json(&get(&state, &format!("/api/changes/{id}/run"), ""));
        assert!(view["job"]["output"].is_object(), "{view}");
    }
    assert_eq!(row_counts(&db), before, "reading the sheet wrote a row");

    open_gate(&gate, "one");
    open_gate(&gate, "two");
    wait_idle(&state, &id);
}

/// pytest's failing summary reaches the sheet's fail form with pytest's own
/// counts — the pass form is the GATED run above.
#[test]
fn a_pytest_failure_reads_failed_with_pytests_own_counts() {
    let (last, n) = run_and_read(PYTEST_FAILS);
    assert_eq!(n, 1);
    assert_eq!(last["outcome"], "failed", "{last}");
    assert_eq!(last["exit_code"], 1);
    assert_eq!(last["tests_failed"], 1, "{last}");
    assert_eq!(last["tests_passed"], 4, "{last}");
    assert_eq!(last["suites"], 1, "{last}");
}

/// **A secret that straddles the served cut is masked, never half shown.** The
/// tail keeps more of a line than the sheet serves, and the sheet masks that text
/// before it cuts each line to `LIVE_LINE_BYTES`.
#[test]
fn a_secret_across_the_served_cut_is_masked_not_half_shown() {
    let live = crate::check::LiveOutput::new();
    let token = format!("ghp_{}", "a".repeat(36));
    // The token starts a few bytes before the served bound and runs past it.
    let mut line = "z ".repeat((crate::check::LIVE_LINE_BYTES - 12) / 2);
    line.push_str(&token);
    line.push_str(" and the rest of the line\n");
    live.feed_for_test(line.as_bytes());
    let served = run_route::output_json(&live);
    let lines = served["lines"].as_array().expect("lines");
    assert_eq!(lines.len(), 1, "{served}");
    let text = lines[0].as_str().expect("a line");
    assert!(!text.contains("ghp_"), "half a token was served: {text}");
    assert!(
        text.len() <= crate::check::LIVE_LINE_BYTES,
        "{}",
        text.len()
    );
    assert_eq!(served["cut"], true, "{served}");
    assert!(
        served["masking"]["masked"].as_u64().unwrap_or(0) >= 1,
        "the masking was not counted: {}",
        served["masking"]
    );
}
