//! Every `--json` shape the binary can emit is versioned, and a new one cannot
//! appear unversioned without failing here.
//!
//! # Why this census walks the binary and not the source
//!
//! This repository's signature defect is a guard that builds the paths it
//! checks instead of calling the function it guards. A source-grep census for
//! `--json` would be exactly that: it would enumerate what the *text* looks
//! like, and a `clap::Args` struct that reaches the CLI through a `#[command(
//! flatten)]` in another file would walk straight past it. Worse, a grep whose
//! pattern stops matching passes forever.
//!
//! So both halves of the comparison come from the **shipping binary**:
//!
//! * the left side is `thruline … --help`, walked recursively, which is clap's
//!   own account of which subcommands offer `--json`;
//! * the right side is `thruline json-shapes`, which prints the registry the
//!   emitters actually call — `json_out::SHAPES`, the same slice whose entries
//!   every `json_out::print` takes by reference.
//!
//! A `--json` surface that nobody registered appears on the left and not the
//! right. A registry entry for a surface that no longer exists appears on the
//! right and not the left. Both are failures, and neither can be produced by a
//! pattern that quietly stops matching, because a pattern that stops matching
//! empties the left side and the right side is still full.
//!
//! # Witnessed
//!
//! Adding a `--json` flag to a command and not registering it turns
//! `census_every_json_surface_is_registered` red naming that command; removing
//! the flag returns it to green. Deleting one entry from `SHAPES` turns it red
//! from the other direction. Both were observed before this file was committed.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_thruline")
}

/// `thruline <path> --help`, as one string (clap writes help to stdout).
fn help_for(path: &[String]) -> String {
    let out = Command::new(bin())
        .args(path)
        .arg("--help")
        .output()
        .unwrap_or_else(|e| panic!("running `thruline {} --help`: {e}", path.join(" ")));
    let mut s = String::from_utf8_lossy(&out.stdout).into_owned();
    s.push_str(&String::from_utf8_lossy(&out.stderr));
    s
}

/// The `Commands:` block lists one subcommand per line, indented, name first.
fn subcommands_in(help: &str) -> Vec<String> {
    let Some(start) = help.find("\nCommands:\n") else {
        return Vec::new();
    };
    let body = &help[start + "\nCommands:\n".len()..];
    let end = body.find("\n\n").unwrap_or(body.len());
    let mut names = Vec::new();
    for line in body[..end].lines() {
        if !line.starts_with("  ") {
            continue;
        }
        let trimmed = line.trim_start();
        let name: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-')
            .collect();
        // A continuation line of a long description starts with a word too, but
        // clap indents those further than the name column; requiring the rest of
        // the line to begin with whitespace after the name keeps them out.
        if name.is_empty() || name == "help" {
            continue;
        }
        if !trimmed[name.len()..].starts_with(char::is_whitespace) && !name.is_empty() {
            continue;
        }
        if trimmed.len() > name.len() && !names.contains(&name) {
            names.push(name);
        }
    }
    names
}

/// Every subcommand path the binary exposes that accepts `--json`.
fn json_surfaces_from_help() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    let mut seen: BTreeSet<Vec<String>> = BTreeSet::new();
    let mut queue: Vec<Vec<String>> = vec![Vec::new()];
    while let Some(path) = queue.pop() {
        if !seen.insert(path.clone()) || path.len() > 4 {
            continue;
        }
        let help = help_for(&path);
        let offers_json = help
            .lines()
            .any(|l| l.trim_start().starts_with("--json") || l.contains(", --json"));
        // `decide` advertises the inherited output flag, but requires a verb.
        // It emits no document by itself; its seven emitting verbs are walked.
        if offers_json && !path.is_empty() && !(path.len() == 1 && path[0] == "decide") {
            found.insert(path.join(" "));
        }
        for sub in subcommands_in(&help) {
            let mut next = path.clone();
            next.push(sub);
            queue.push(next);
        }
    }
    found
}

#[test]
fn decide_root_requires_a_verb_and_emits_no_json_document() {
    let output = Command::new(bin())
        .args(["decide", "--json"])
        .output()
        .expect("run decide root");
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
}

#[derive(Debug, Clone)]
struct Entry {
    id: String,
    command: String,
    surface: String,
    version: u64,
    envelope: String,
    note: String,
}

/// The registry, read out of the binary that ships it.
fn registry() -> Vec<Entry> {
    let out = Command::new(bin())
        .arg("json-shapes")
        .output()
        .expect("running `thruline json-shapes`");
    assert!(
        out.status.success(),
        "`thruline json-shapes` exited {:?}: {}",
        out.status.code(),
        String::from_utf8_lossy(&out.stderr)
    );
    let doc: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!(
            "`thruline json-shapes` did not emit JSON ({e}): {}",
            String::from_utf8_lossy(&out.stdout)
        )
    });
    assert!(
        doc.get("schema_version").and_then(Value::as_u64).is_some(),
        "the registry listing is itself a --json shape and must carry its own \
         top-level schema_version; got {doc}"
    );
    doc["shapes"]
        .as_array()
        .expect("`shapes` array")
        .iter()
        .map(|s| Entry {
            id: s["id"].as_str().expect("id").to_string(),
            command: s["command"].as_str().expect("command").to_string(),
            surface: s["surface"].as_str().expect("surface").to_string(),
            version: s["version"].as_u64().expect("version"),
            envelope: s["envelope"].as_str().expect("envelope").to_string(),
            note: s["note"].as_str().unwrap_or_default().to_string(),
        })
        .collect()
}

#[test]
fn census_every_json_surface_is_registered() {
    let from_help = json_surfaces_from_help();
    assert!(
        from_help.len() > 20,
        "the help walk found only {} `--json` surfaces, which means the walk \
         broke rather than the binary shrinking; a census whose left side is \
         empty passes forever: {from_help:?}",
        from_help.len()
    );

    let registered: BTreeSet<String> = registry()
        .iter()
        .filter(|e| e.surface == "--json")
        .map(|e| e.command.clone())
        .collect();

    let unregistered: Vec<&String> = from_help.difference(&registered).collect();
    assert!(
        unregistered.is_empty(),
        "these commands accept `--json` and emit an unversioned shape — add an \
         entry to `crates/cli/src/json_out.rs::SHAPES` and route the emitter \
         through `json_out::print`: {unregistered:?}"
    );

    let stale: Vec<&String> = registered.difference(&from_help).collect();
    assert!(
        stale.is_empty(),
        "these registry entries claim a `--json` surface the binary no longer \
         offers; four teams pin these, so a stale entry is a promise nobody \
         can call: {stale:?}"
    );
}

#[test]
fn registry_ids_and_versions_are_well_formed() {
    let entries = registry();
    let mut by_id: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &entries {
        *by_id.entry(e.id.as_str()).or_default() += 1;
        assert!(
            e.version >= 1,
            "{}: schema_version starts at 1, not {}",
            e.id,
            e.version
        );
        assert!(
            e.id.starts_with("thruline."),
            "{}: shape ids are namespaced `thruline.<path>` so a consumer that \
             logs one can tell whose it is",
            e.id
        );
    }
    let dupes: Vec<&&str> = by_id
        .iter()
        .filter(|(_, n)| **n > 1)
        .map(|(id, _)| id)
        .collect();
    assert!(dupes.is_empty(), "duplicate shape ids: {dupes:?}");
}

/// Shapes ThruLine does not own. Each is an external schema whose consumer is
/// not a ThruLine consumer, and injecting a field into one is a defect, not a
/// version. The list is exact: a new exemption fails here until a human writes
/// the reason.
const EXPECTED_EXTERNAL: &[&str] = &["thruline.receipt.dsse", "thruline.verify-receipt.check-run"];

#[test]
fn external_shapes_are_exactly_the_declared_ones_and_each_carries_a_reason() {
    let entries = registry();
    let external: BTreeSet<String> = entries
        .iter()
        .filter(|e| e.envelope == "external")
        .map(|e| e.id.clone())
        .collect();
    let expected: BTreeSet<String> = EXPECTED_EXTERNAL.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        external, expected,
        "the set of shapes exempt from carrying `schema_version` changed. An \
         exemption is a promise that someone else's schema owns the bytes; it \
         is never a shortcut past versioning."
    );
    for e in entries.iter().filter(|e| e.envelope == "external") {
        assert!(
            e.note.len() > 40,
            "{}: an external exemption must say whose schema it is and why a \
             ThruLine field cannot go in it; got {:?}",
            e.id,
            e.note
        );
    }
}

/// `(argv, the surface clap names, shape id)` for every surface this test can
/// invoke with no fixture.
///
/// argv and surface are separate columns on purpose: `sync` takes a positional
/// verb, so it is invoked as `sync status` while clap still spells the surface
/// `sync`. Deriving one from the other by trimming words is how `shim status`
/// silently stopped being counted the first time this was written.
///
/// The uninvocable remainder is listed in `NOT_EXECUTED` with a reason, so a
/// shape that is never exercised is visible rather than absent.
fn invocable() -> Vec<(Vec<&'static str>, &'static str, &'static str)> {
    vec![
        (vec!["now"], "now", "thruline.now"),
        (
            vec!["shadow-sessions"],
            "shadow-sessions",
            "thruline.shadow-sessions",
        ),
        (
            vec!["entity", "list"],
            "entity list",
            "thruline.entity.list",
        ),
        (
            vec!["verify-claims"],
            "verify-claims",
            "thruline.verify-claims",
        ),
        (vec!["doctor"], "doctor", "thruline.doctor"),
        (
            vec!["shim", "status"],
            "shim status",
            "thruline.shim.status",
        ),
        (vec!["sync", "status"], "sync", "thruline.sync"),
        (vec!["policy"], "policy", "thruline.policy.check"),
        (vec!["bench"], "bench", "thruline.bench"),
        (vec!["evidence"], "evidence", "thruline.evidence.list"),
        (vec!["sessions"], "sessions", "thruline.sessions"),
        // T-DIST-4: needs no ledger and no licence; with none installed it
        // still answers, which is the point.
        (
            vec!["licence", "status"],
            "licence status",
            "thruline.licence.status",
        ),
    ]
}

/// Surfaces the behavioural half cannot reach without a fixture, each named.
const NOT_EXECUTED: &[(&str, &str)] = &[
    (
        "verify",
        "requires an authored sealed-recording ledger; native versioned JSON and 0/1/2 source states are exercised in recorded_prefix_verify_adversaries.rs",
    ),
    (
        "decide list",
        "needs a project in a scratch ledger; the versioned list is exercised in crates/cli/tests/decide_cmd.rs",
    ),
    (
        "decide propose",
        "needs a recorded person event in a scratch ledger; exercised in crates/cli/tests/decide_cmd.rs",
    ),
    (
        "decide accept",
        "needs an exact recorded person action tuple; exercised in crates/cli/tests/decide_cmd.rs",
    ),
    (
        "decide retire",
        "needs a verified accepted decision and person retirement event; exercised in crates/cli/tests/decide_cmd.rs",
    ),
    (
        "decide defaults",
        "needs a project in a scratch ledger; exercised in crates/cli/tests/decision_k_first_use.rs",
    ),
    (
        "decide check",
        "needs an exact Git tree and a verified accepted decision; exercised in crates/cli/tests/decide_cmd.rs",
    ),
    (
        "decide gate",
        "needs a project policy and exact-version receipt; exercised in crates/cli/tests/decision_policy_gate.rs",
    ),
    (
        "decide confirm",
        "requires independent person presence; its JSON is tested through pure construction in decide_cmd::json_contract_tests, without invoking confirmation",
    ),
    (
        "decide inline",
        "requires a synthetic Git tree and proposal ledger; public single/member JSON is exercised in crates/cli/tests/decision_inline_cli.rs",
    ),
    (
        "decide prepare",
        "requires an existing proposal and appends only an inert action row; the closed JSON is exercised in crates/cli/tests/decision_json_cli.rs",
    ),
    (
        "decide repair-derived",
        "requires a project and writes its derived copy; a synthetic unaccepted project is exercised in crates/cli/tests/decision_json_cli.rs",
    ),
    (
        "decide rules",
        "requires tracked rules at an exact synthetic Git version; its JSON is exercised in crates/cli/tests/decision_json_cli.rs",
    ),
    (
        "decide revise-params",
        "requires an exact source-backed proposal and parameter document; its JSON and unchanged authority tables are exercised in crates/cli/tests/decision_revision_cli.rs",
    ),
    (
        "change waive",
        "setting an obligation aside needs an accepted scope whose profile \
         configures an exception path; both the recorded and the two refused \
         shapes are exercised against the binary in change_exceptions_cli.rs",
    ),
    (
        "change cancel-run",
        "stopping a real check needs a real process group; both the acted and \
         the refused shapes are exercised against the binary in \
         change_cancel_run_cli.rs",
    ),
    (
        "change finding",
        "changing where a review finding stands needs a Change holding a sealed \
         review; the acted shape (and its `shape` id) is exercised against the \
         binary in change_finding_cli.rs",
    ),
    (
        "acceptance resume",
        "l0_acceptance executes matched and mismatched handoff acknowledgement",
    ),
    (
        "acceptance receipt",
        "signing and explicit-key verification exercised in l0_acceptance.rs",
    ),
    (
        "acceptance run",
        "executes real subprocess checks in disposable repositories in l0_acceptance.rs",
    ),
    (
        "acceptance init",
        "executed with disposable repository fixtures in l0_acceptance.rs",
    ),
    (
        "acceptance accept",
        "executed with reviewed digest and disposable history in l0_acceptance.rs",
    ),
    (
        "acceptance status",
        "executed across process restarts in l0_acceptance.rs",
    ),
    (
        "acceptance handoff",
        "executed with a persisted contract in l0_acceptance.rs",
    ),
    (
        "claims",
        "needs a folded execution store with a session in it",
    ),
    (
        "explain",
        "needs a folded execution store with a session in it",
    ),
    (
        "promises",
        "needs a folded execution store with a session in it",
    ),
    (
        "receipt",
        "external DSSE envelope; exempt, and signing writes a key",
    ),
    ("verify-receipt", "needs a signed receipt file on disk"),
    ("reexec", "needs a session and a Docker daemon"),
    ("run", "spawns a child process and writes to the real spool"),
    ("fold", "writes to the real spool and execution store"),
    ("import-codex", "reads the user's ~/.codex rollouts"),
    ("import-legacy", "walks the 6.9 GB production ledger"),
    ("demo", "builds a git repo and runs a test runner"),
    (
        "hook-install",
        "writes to the user's ~/.claude/settings.json",
    ),
    (
        "shim install",
        "writes wrappers onto the user's PATH directory",
    ),
    (
        "shim uninstall",
        "removes wrappers from the user's PATH directory",
    ),
    ("why", "needs an attention item or a truth-loop record"),
    (
        "capture install",
        "needs a capture root, owner and source triple",
    ),
    (
        "capture status",
        "needs a capture root, owner and source triple",
    ),
    (
        "capture doctor",
        "needs a capture root, owner and source triple",
    ),
    // T-2-2. Each needs a ledger with a project, a session and a Change in it;
    // all four are executed against exactly such a fixture in
    // `crates/cli/tests/change_link_cli.rs`, under a temporary HOME.
    (
        "change suggest",
        "needs a ledger holding a project, a session and a Change; executed in change_link_cli.rs",
    ),
    (
        "change associate",
        "writes a person's decision; executed against a fixture ledger in change_link_cli.rs",
    ),
    (
        "change results",
        "needs a Change with linked sessions; executed in change_link_cli.rs",
    ),
    (
        "change links",
        "needs a session in a ledger; executed in change_link_cli.rs",
    ),
    // T-2-3. Both need a ledger holding a project, a session with an opening
    // request, and a Change with a confirmed link; both are executed against
    // exactly such a fixture in `crates/cli/tests/change_accept_cli.rs`.
    (
        "change prepare",
        "observes a real git worktree and probes a port; executed against a \
         fixture ledger in change_prepare_cli.rs",
    ),
    (
        "change propose",
        "needs a Change with a confirmed session and a request to read; executed in change_accept_cli.rs",
    ),
    (
        "change accept",
        "writes a scope version and its acceptance record; executed against a fixture ledger in change_accept_cli.rs",
    ),
    // T-2-4 fix round 1. `change new` writes the Change every other verb here
    // is about, so it needs a ledger with a project in it; its envelope,
    // version and both registers are asserted against exactly such a fixture
    // in `crates/cli/tests/change_new_cli.rs`.
    (
        "change new",
        "writes one Change row and needs a ledger holding a project; executed in change_new_cli.rs",
    ),
];

#[test]
fn every_json_surface_is_either_executed_here_or_named_as_not_executed() {
    let from_help = json_surfaces_from_help();
    let executed: BTreeSet<String> = invocable()
        .iter()
        .map(|(_, surface, _)| surface.to_string())
        .collect();
    assert!(
        from_help.len() > 20,
        "the help walk found only {} surfaces; an empty left side makes a \
         subset check pass forever",
        from_help.len()
    );
    let excused: BTreeSet<String> = NOT_EXECUTED.iter().map(|(c, _)| c.to_string()).collect();
    let unaccounted: Vec<&String> = from_help
        .iter()
        .filter(|c| !executed.contains(*c) && !excused.contains(*c))
        .collect();
    assert!(
        unaccounted.is_empty(),
        "these `--json` surfaces are neither exercised by this test nor listed \
         in NOT_EXECUTED with a reason: {unaccounted:?}"
    );
    for (cmd, reason) in NOT_EXECUTED {
        assert!(
            reason.len() > 15,
            "{cmd}: NOT_EXECUTED needs a reason, not a placeholder"
        );
    }
}

#[test]
fn executed_surfaces_emit_their_registered_version_at_the_top_level() {
    let reg: BTreeMap<String, u64> = registry()
        .into_iter()
        .map(|e| (e.id.clone(), e.version))
        .collect();
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");

    for (argv, _surface, id) in invocable() {
        let mut cmd = Command::new(bin());
        cmd.args(&argv).arg("--json").arg("--db").arg(&db);
        cmd.current_dir(dir.path());
        cmd.env("HOME", dir.path())
            .env("THRULINE_HOME", dir.path().join("state"));
        let out = cmd
            .output()
            .unwrap_or_else(|e| panic!("running `thruline {}`: {e}", argv.join(" ")));
        let stdout = String::from_utf8_lossy(&out.stdout);
        let doc: Value = serde_json::from_str(stdout.trim()).unwrap_or_else(|e| {
            panic!(
                "`thruline {} --json` did not print JSON on stdout ({e}). A \
                 `--json` surface that prints prose is unparseable for every \
                 consumer that pinned it. Got: {stdout:?} / stderr {:?}",
                argv.join(" "),
                String::from_utf8_lossy(&out.stderr)
            )
        });
        let obj = doc.as_object().unwrap_or_else(|| {
            panic!(
                "`thruline {} --json` emitted a bare {} at the top level. A \
                 top-level `schema_version` cannot be read out of one, which \
                 is why the collection shapes are wrapped.",
                argv.join(" "),
                if doc.is_array() { "array" } else { "scalar" }
            )
        });
        let got = obj
            .get("schema_version")
            .and_then(Value::as_u64)
            .unwrap_or_else(|| {
                panic!(
                    "`thruline {} --json` has no top-level integer \
                     `schema_version`; keys were {:?}",
                    argv.join(" "),
                    obj.keys().collect::<Vec<_>>()
                )
            });
        let want = reg
            .get(id)
            .unwrap_or_else(|| panic!("{id} is not in the registry"));
        assert_eq!(
            got,
            *want,
            "`thruline {} --json` emitted schema_version {got}, the registry \
             says {want}. A consumer pins the registry.",
            argv.join(" ")
        );
    }
}

/// The top-level field names of every shape this test can invoke, pinned.
///
/// # Why a key list and not an `insta` snapshot
///
/// What four external teams pin is the *field set*, not the byte rendering: a
/// value that varies per machine (a path, a timestamp, a repo id) makes a byte
/// snapshot fail for everyone but its author, and the usual repair is to accept
/// the new snapshot, which silently accepts real drift with it. The key list is
/// stable across machines and moves only when the shape moves — which is the
/// event a consumer needs to hear about. It also avoids adding `insta` to this
/// crate's dev-dependencies, and with it a `Cargo.lock` edit in a tree two
/// other coders are writing to.
const PINNED_TOP_LEVEL_FIELDS: &[(&str, &[&str])] = &[
    ("thruline.now", &["items", "schema_version", "shape"]),
    (
        "thruline.shadow-sessions",
        &["schema_version", "sessions", "shape"],
    ),
    (
        "thruline.entity.list",
        &["records", "schema_version", "shape"],
    ),
    (
        "thruline.verify-claims",
        &["claims", "schema_version", "shape"],
    ),
    (
        "thruline.doctor",
        &["findings", "ok", "schema_version", "shape"],
    ),
    (
        "thruline.shim.status",
        &[
            "captures_nothing",
            "forged",
            "installed",
            "intact",
            "missing",
            "modified",
            "on_path",
            "problem",
            "schema_version",
            "shadowed_by",
            "shape",
            "unexpected",
            "uncovered",
        ],
    ),
    (
        "thruline.sync",
        &["feature", "ok", "reason", "schema_version", "shape", "verb"],
    ),
    (
        "thruline.policy.check",
        &[
            "conclusion",
            "ok",
            "path",
            "reason",
            "schema_version",
            "shape",
        ],
    ),
    (
        "thruline.bench",
        &[
            "fixtures_dir",
            "ok",
            "reason",
            "schema_version",
            "shape",
            "sources",
        ],
    ),
    (
        "thruline.sessions",
        &[
            "limit",
            "limit_scope",
            "returned",
            "schema_version",
            "sessions",
            "shape",
            "store",
            "total",
        ],
    ),
    (
        "thruline.evidence.list",
        &[
            "bindings",
            "evaluated_at",
            "reason",
            "repository_identity",
            "schema_version",
            "shape",
        ],
    ),
    // T-DIST-4. The same document `GET /api/licence` serves. v4 removes
    // the retired trial field while preserving the paid-gate contract.
    (
        "thruline.licence.status",
        &[
            "enable",
            "entitlements",
            "expires_at",
            "expires_on",
            "headline",
            "paid",
            "paid_gate",
            "plan",
            "schema_version",
            "sentence",
            "shape",
            "status",
            "technical",
        ],
    ),
];

#[test]
fn the_shapes_four_teams_will_pin_have_not_moved() {
    let pinned: BTreeMap<&str, &[&str]> = PINNED_TOP_LEVEL_FIELDS.iter().copied().collect();
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("ledger.db");
    for (argv, _surface, id) in invocable() {
        let out = Command::new(bin())
            .args(&argv)
            .arg("--json")
            .arg("--db")
            .arg(&db)
            .current_dir(dir.path())
            .env("HOME", dir.path())
            .env("THRULINE_HOME", dir.path().join("state"))
            .output()
            .unwrap_or_else(|e| panic!("running `thruline {}`: {e}", argv.join(" ")));
        let doc: Value = serde_json::from_slice(&out.stdout)
            .unwrap_or_else(|e| panic!("`thruline {} --json`: {e}", argv.join(" ")));
        let mut got: Vec<&str> = doc
            .as_object()
            .unwrap_or_else(|| panic!("{id} is not an object"))
            .keys()
            .map(String::as_str)
            .collect();
        got.sort_unstable();
        let mut want: Vec<&str> = pinned
            .get(id)
            .unwrap_or_else(|| panic!("{id} has no pinned field list"))
            .to_vec();
        want.sort_unstable();
        assert_eq!(
            got, want,
            "the top-level fields of {id} changed. Four teams pin this; a field \
             added or renamed here is a `schema_version` bump, not an edit."
        );
    }
}

#[test]
fn sessions_v2_reports_the_actual_global_or_per_project_limit() {
    let dir = tempfile::tempdir().expect("tempdir");
    let db = dir.path().join("exec.db");
    let ledger = thruline_ledger::Ledger::open(&db).expect("ledger");
    for project in ["first", "second"] {
        let root = dir.path().join(project).to_string_lossy().to_string();
        let id = ledger.repo(&root, Some(&root)).expect("repo");
        for suffix in ["a", "b"] {
            ledger
                .upsert_session(
                    &format!("{project}-{suffix}"),
                    &id,
                    "test",
                    None,
                    false,
                    None,
                )
                .expect("session");
        }
    }
    drop(ledger);
    for (per_project, scope, expected) in [(false, "store", 1), (true, "project", 2)] {
        let mut command = Command::new(bin());
        command
            .args(["sessions", "--json", "--limit", "1", "--exec-db"])
            .arg(&db);
        if per_project {
            command.arg("--per-project");
        }
        let output = command.current_dir(dir.path()).output().expect("sessions");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).expect("JSON");
        assert_eq!(value["schema_version"], 2);
        assert_eq!(value["shape"], "thruline.sessions");
        assert_eq!(value["limit_scope"], scope);
        assert_eq!(value["limit"], 1);
        assert_eq!(value["total"], 4);
        assert_eq!(value["returned"], expected);
        let rows = value["sessions"].as_array().expect("rows");
        assert_eq!(rows.len(), expected as usize);
        if per_project {
            let projects: BTreeSet<_> = rows
                .iter()
                .map(|row| row["repo_key"].as_str().expect("repo key"))
                .collect();
            assert_eq!(
                projects.len(),
                2,
                "a quiet project was hidden by the global limit"
            );
        }
    }
}
