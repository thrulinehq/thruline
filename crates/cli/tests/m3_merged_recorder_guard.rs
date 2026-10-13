//! M3.4's headline clause, over the view the harness actually reads.
//!
//! > *"the four-`Stop`-entries / two-recorders state is now impossible to write
//! > from any install path."*
//!
//! It was not. Both guards reasoned over **one file**:
//!
//! * `hook_install::refuse_double_recorder` read `before` from
//!   `std::fs::read_to_string(path)` — the single file it was about to write;
//! * `doctor::hook_packs_in` counted recorders **inside** its per-path loop.
//!
//! BUILD-PLAN B.1, quoting the vendor doc: *"Hook entries merge across settings
//! levels rather than replacing each other."* So `thruline init --yes` (user
//! settings) followed by `thruline hook-install --gate --scope project`
//! (project settings) put **two recorders on seven events** — `Stop` with four
//! entries — with both commands exiting 0 and `doctor` reporting `ok`. Neither
//! guard ever saw the merged view, so neither could see the state it existed to
//! prevent.
//!
//! Every test here drives the real binary against a private `HOME` and a
//! private working directory, so the paths under test are the *conventional*
//! ones the harness merges — which is the whole point. A test that passed
//! `--settings <tmpfile>` would exercise a file Claude Code never reads and
//! prove nothing about a merge.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::{tempdir, TempDir};

/// A machine: its own `HOME`, its own repository, its own ledger.
struct Machine {
    _temp: TempDir,
    home: PathBuf,
    repo: PathBuf,
    db: PathBuf,
}

fn machine() -> Machine {
    let temp = tempdir().expect("tempdir");
    let home = temp.path().join("home");
    let repo = temp.path().join("repo");
    std::fs::create_dir_all(&home).expect("home");
    std::fs::create_dir_all(&repo).expect("repo");
    for args in [
        vec!["init", "-q"],
        vec!["config", "user.email", "t@example.invalid"],
        vec!["config", "user.name", "t"],
    ] {
        let ok = Command::new("git")
            .args(&args)
            .current_dir(&repo)
            .output()
            .expect("git")
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }
    let db = temp.path().join("ledger.db");
    Machine {
        _temp: temp,
        home,
        repo,
        db,
    }
}

impl Machine {
    /// Run `thruline` as the harness's user would: this `HOME`, this repo as
    /// the working directory, and no `--settings` override — so the command
    /// resolves the same conventional paths Claude Code merges.
    fn run(&self, args: &[&str]) -> (String, bool) {
        let out = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .args(["--db", self.db.to_str().unwrap()])
            .args(args)
            .current_dir(&self.repo)
            .env("HOME", &self.home)
            .output()
            .expect("run thruline");
        (
            format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            ),
            out.status.success(),
        )
    }

    fn user_settings(&self) -> PathBuf {
        self.home.join(".claude").join("settings.json")
    }

    fn project_settings(&self) -> PathBuf {
        self.repo.join(".claude").join("settings.json")
    }

    fn project_local_settings(&self) -> PathBuf {
        self.repo.join(".claude").join("settings.local.json")
    }

    /// The **merged** recorder count per event: what the harness sees, summed
    /// across every settings level, which is the view both guards were blind to.
    fn merged_recorders(&self) -> BTreeMap<String, usize> {
        let mut out: BTreeMap<String, usize> = BTreeMap::new();
        for path in [
            self.user_settings(),
            self.project_settings(),
            self.project_local_settings(),
        ] {
            for (event, count) in recorders_in(&path) {
                *out.entry(event).or_default() += count;
            }
        }
        out
    }

    fn merged_stop_entries(&self) -> Vec<String> {
        [
            self.user_settings(),
            self.project_settings(),
            self.project_local_settings(),
        ]
        .iter()
        .flat_map(|p| commands_on(p, "Stop"))
        .collect()
    }
}

fn commands_on(path: &Path, event: &str) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return Vec::new();
    };
    value
        .get("hooks")
        .and_then(|h| h.get(event))
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default()
        .iter()
        .filter_map(|e| e.get("hooks").and_then(serde_json::Value::as_array))
        .flatten()
        .filter_map(|h| h.get("command").and_then(serde_json::Value::as_str))
        .filter(|c| c.contains("thruline"))
        .map(str::to_string)
        .collect()
}

/// `doctor::is_recorder`, restated here on purpose.
///
/// This file is the *acceptance* test: it must be able to fail when the
/// product's own reading of "recorder" narrows. Importing the predicate under
/// test would make the assertion true by construction.
fn is_recorder(command: &str) -> bool {
    [
        " hook claude-code",
        " hook codex",
        " hook gemini",
        " sensor append",
    ]
    .iter()
    .any(|marker| command.contains(marker))
}

fn recorders_in(path: &Path) -> BTreeMap<String, usize> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return BTreeMap::new();
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return BTreeMap::new();
    };
    let Some(hooks) = value.get("hooks").and_then(serde_json::Value::as_object) else {
        return BTreeMap::new();
    };
    hooks
        .keys()
        .map(|event| {
            (
                event.clone(),
                commands_on(path, event)
                    .iter()
                    .filter(|c| is_recorder(c))
                    .count(),
            )
        })
        .filter(|(_, n)| *n > 0)
        .collect()
}

fn assert_at_most_one_recorder_per_event(m: &Machine, context: &str) {
    let merged = m.merged_recorders();
    let doubled: Vec<_> = merged.iter().filter(|(_, n)| **n > 1).collect();
    assert!(
        doubled.is_empty(),
        "{context}: the merged view carries more than one ThruLine RECORDER on {doubled:?}. \
         Two recorders write two records of one session and nothing afterwards says which \
         record a claim was checked against. Merged Stop entries: {:#?}",
        m.merged_stop_entries()
    );
}

/// A settings file with one hand-written ThruLine recorder on `Stop`.
///
/// Written by hand rather than by an install, because once the guard works the
/// installs refuse — and `doctor` still has to be able to see a machine that
/// reached this state before the guard existed.
fn seed_recorder(path: &Path, command: &str) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    std::fs::write(
        path,
        serde_json::to_string_pretty(&serde_json::json!({
            "hooks": { "Stop": [ { "hooks": [ { "type": "command", "command": command } ] } ] }
        }))
        .expect("json"),
    )
    .expect("write");
}

// ---------------------------------------------------------------------------
// The reproduction, in both orders.
// ---------------------------------------------------------------------------

/// The exact sequence from the defect report: user scope, then project scope.
#[test]
fn init_then_a_project_scoped_hook_install_cannot_reach_two_recorders() {
    let m = machine();
    let (out, ok) = m.run(&["init", "--yes"]);
    assert!(ok, "init failed: {out}");

    let (out, ok) = m.run(&["hook-install", "--gate", "--scope", "project"]);
    assert!(
        !ok,
        "`hook-install --gate --scope project` exited 0 on a machine `init` had already set \
         up. Claude Code MERGES hook entries across settings levels, so this installs a \
         second recorder on every capture event:\n{out}"
    );
    assert!(
        out.contains("RECORDER"),
        "the refusal must name what it refused and why: {out}"
    );
    assert_at_most_one_recorder_per_event(&m, "init then hook-install --scope project");
    assert!(
        !m.project_settings().exists() || recorders_in(&m.project_settings()).is_empty(),
        "a refused install must write nothing: {:?}",
        std::fs::read_to_string(m.project_settings())
    );
}

/// The reverse order, which the defect report measured as worse.
#[test]
fn a_project_scoped_hook_install_then_init_cannot_reach_two_recorders() {
    let m = machine();
    let (out, ok) = m.run(&["hook-install", "--gate", "--scope", "project"]);
    assert!(ok, "hook-install failed: {out}");

    let (out, ok) = m.run(&["init", "--yes"]);
    assert!(
        !ok,
        "`init --yes` exited 0 on a machine `hook-install --scope project` had already set \
         up, adding a second recorder to the merged view:\n{out}"
    );
    assert_at_most_one_recorder_per_event(&m, "hook-install --scope project then init");
}

/// `--gate` on both documented install paths: the `Stop` hook must not end up
/// speaking twice.
#[test]
fn the_gate_never_ends_up_installed_twice_on_stop() {
    for order in [["init", "hook-install"], ["hook-install", "init"]] {
        let m = machine();
        for command in order {
            let args: Vec<&str> = match command {
                "init" => vec!["init", "--yes", "--gate"],
                _ => vec!["hook-install", "--gate", "--scope", "project"],
            };
            let _ = m.run(&args);
        }
        let gates = m
            .merged_stop_entries()
            .iter()
            .filter(|c| c.contains("gate --hook stop"))
            .count();
        assert!(
            gates <= 1,
            "{order:?}: the Stop gate is installed {gates} times in the merged view, so it \
             speaks twice on every stop. Merged Stop: {:#?}",
            m.merged_stop_entries()
        );
        assert_at_most_one_recorder_per_event(&m, &format!("{order:?} with --gate on both"));
    }
}

// ---------------------------------------------------------------------------
// The blind spot: `.claude/settings.local.json`.
// ---------------------------------------------------------------------------

/// `.claude/settings.local.json` is read by Claude Code and merged like any
/// other level. It was absent from `settings_paths()`, so both guards were
/// blind to it — a recorder parked there was invisible to the install guard and
/// to `doctor` alike.
#[test]
fn a_recorder_in_project_local_settings_is_part_of_the_merged_view() {
    let m = machine();
    seed_recorder(
        &m.project_local_settings(),
        "/usr/local/bin/thruline hook claude-code",
    );

    let (out, ok) = m.run(&["hook-install", "--scope", "project"]);
    assert!(
        !ok,
        "a recorder in .claude/settings.local.json is merged by the harness, so installing \
         a second one into .claude/settings.json is the same collision:\n{out}"
    );
    assert_at_most_one_recorder_per_event(&m, "recorder seeded in settings.local.json");
}

// ---------------------------------------------------------------------------
// The detector.
// ---------------------------------------------------------------------------

/// `doctor` must report a merged collision that already exists on disk.
///
/// This is the state a machine reached *before* the guard, and the guard cannot
/// help it: the only thing that can is the detector saying so.
#[test]
fn doctor_reports_a_two_recorder_state_that_spans_two_settings_levels() {
    let m = machine();
    let exe = env!("CARGO_BIN_EXE_thruline");
    seed_recorder(&m.user_settings(), &format!("{exe} sensor append Stop"));
    seed_recorder(&m.project_settings(), &format!("{exe} hook claude-code"));

    let (out, ok) = m.run(&["doctor"]);
    assert!(
        !ok,
        "doctor exited 0 on a machine whose merged settings put two ThruLine recorders on \
         Stop:\n{out}"
    );
    assert!(
        out.contains("RECORDER"),
        "doctor must name the collision: {out}"
    );
    assert!(
        out.contains("Stop"),
        "doctor must name the event the collision is on: {out}"
    );
}

/// And it must report one that reaches into `.claude/settings.local.json`.
///
/// The test above passes for a `doctor` that reads only the user and project
/// files, because that is where it puts both recorders. This one does not: it
/// fails unless `doctor` is counting over the level list that includes the
/// gitignored project file. Written after the mutation witness for the previous
/// test came back GREEN with `doctor` restored to the two-path list — the
/// property was implemented and unwitnessed, which is the state this program's
/// rule exists to catch.
#[test]
fn doctor_reports_a_collision_that_reaches_into_project_local_settings() {
    let m = machine();
    let exe = env!("CARGO_BIN_EXE_thruline");
    seed_recorder(&m.user_settings(), &format!("{exe} sensor append Stop"));
    seed_recorder(
        &m.project_local_settings(),
        &format!("{exe} hook claude-code"),
    );

    let (out, ok) = m.run(&["doctor"]);
    assert!(
        !ok,
        "doctor exited 0 on a machine whose merged settings put a recorder in \
         ~/.claude/settings.json and another in .claude/settings.local.json — a file the \
         harness reads and `settings_paths()` did not list:\n{out}"
    );
    assert!(
        out.contains("RECORDER") && out.contains("project-local"),
        "doctor must name the level it found the second recorder in: {out}"
    );
}

/// One recorder, split across nothing — the honest single-pack machine — still
/// reads `ok`. Without this the test above passes for a detector that fails on
/// everything.
#[test]
fn doctor_stays_ok_on_a_single_recorder_across_levels() {
    let m = machine();
    let (out, ok) = m.run(&["init", "--yes"]);
    assert!(ok, "init failed: {out}");
    let (out, _) = m.run(&["doctor"]);
    assert!(
        !out.contains("more than one ThruLine RECORDER"),
        "a single-pack machine must not be reported as a collision: {out}"
    );
}

// ---------------------------------------------------------------------------
// The write that REDUCES a collision must still get through.
// ---------------------------------------------------------------------------

/// `--undo` is the way off a machine already in the two-recorder state, so the
/// guard must refuse writes that *create or keep* a collision and allow writes
/// that lower it. An unconditional refusal would strand exactly the user who
/// needs the fix.
#[test]
fn undo_still_works_on_a_machine_already_carrying_two_recorders() {
    let m = machine();
    let exe = env!("CARGO_BIN_EXE_thruline");
    // Both levels seeded by hand, into the state the guard now prevents.
    seed_recorder(&m.user_settings(), &format!("{exe} sensor append Stop"));
    seed_recorder(&m.project_settings(), &format!("{exe} hook claude-code"));
    assert_eq!(
        m.merged_recorders().get("Stop").copied(),
        Some(2),
        "the fixture must start in the state under test"
    );

    let (out, ok) = m.run(&["hook-install", "--undo", "--scope", "project"]);
    assert!(
        ok,
        "`hook-install --undo` must get through on a machine already carrying two \
         recorders — it is the write that fixes it:\n{out}"
    );
    assert_eq!(
        m.merged_recorders().get("Stop").copied(),
        Some(1),
        "undo removed the §4.1 capture pack and left the sensor pack"
    );
}
