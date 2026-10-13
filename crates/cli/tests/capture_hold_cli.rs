//! C30 R-LEDGER — `thruline capture pause|resume|exclude|include|holds`: the
//! terminal door to a person's pause and path exclusions.
//!
//! Each verb calls the same core function its route calls
//! (`crates/cli/tests/hq_capture_hold_route.rs` asks the route), so what is
//! asked here is the door itself: the project named by its id or by its folder,
//! the rows left in the record, the words of a refusal (the route's own), the
//! exit status (1 for "nothing was saved"), that no folder or home path is
//! printed, and — the point of the card — that every verb that writes says
//! "Nothing honours a pause or an exclusion yet".
//!
//! `capture status` is the Assured Capture command's and keeps its meaning;
//! this verb is `capture holds`.
//!
//! A real store under a temporary HOME with a synthetic recording; no real store
//! is opened.

use std::path::PathBuf;
use std::process::{Command, Output, Stdio};

use serde_json::Value;

struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("h/.thruline")).expect("home");
        std::fs::create_dir_all(dir.path().join("repo/.git")).expect("repo");
        let m = Machine { dir };
        m.record();
        m.ingest();
        m
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("h")
    }
    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }
    fn repo(&self) -> String {
        self.dir.path().join("repo").display().to_string()
    }
    fn project_dir(&self) -> PathBuf {
        let mangled: String = self
            .repo()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect();
        self.home().join(".claude/projects").join(mangled)
    }
    fn cmd(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.args(args)
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env("USER", "robin")
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
            .env_remove("CLAUDE_CODE_SESSION_ID")
            .env_remove("THRULINE_SESSION_ID")
            .current_dir(self.dir.path().join("repo"));
        cmd
    }
    fn record(&self) {
        let cwd = self.repo();
        std::fs::create_dir_all(self.project_dir()).expect("dir");
        let line = |n: u32, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind, "uuid": format!("s1-{n}"), "parentUuid": null,
                "isSidechain": false, "sessionId": "s1",
                "timestamp": format!("2026-10-01T09:0{n}:00.000Z"), "cwd": cwd, "message": message,
            })
            .to_string()
        };
        let lines = [
            line(
                1,
                "user",
                serde_json::json!({"role": "user", "content": "Fix the parser."}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role": "assistant", "content": [
                {"type": "text", "text": "Done. I updated `src/a.rs`."}]}),
            ),
        ];
        std::fs::write(self.project_dir().join("s1.jsonl"), lines.join("\n") + "\n")
            .expect("recording");
    }
    fn ingest(&self) {
        let status = self
            .cmd(&["ingest", "--apply", "--source", "claude-code"])
            .arg(self.project_dir())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("ingest");
        assert!(status.success());
    }
    /// The HQ project the recording made: its id and root.
    fn project(&self) -> (String, String) {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        let projects = thruline_core::hq::hq_projects(&ledger).expect("projects");
        let p = projects.first().expect("one project");
        (p.id.clone(), p.root.clone())
    }
    fn run(&self, args: &[&str]) -> (Option<i32>, String) {
        let out: Output = self.cmd(args).output().expect("run");
        (
            out.status.code(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
        )
    }
    #[allow(clippy::type_complexity)]
    fn table(&self) -> Vec<(String, Option<String>, String, Option<String>, String)> {
        let ledger = thruline_core::ledger::Ledger::open(self.db()).expect("open");
        let mut stmt = ledger
            .conn()
            .prepare("SELECT act, pattern, actor, ends, id FROM capture_holds ORDER BY seq")
            .expect("prepare");
        let rows = stmt
            .query_map([], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            })
            .expect("query")
            .collect::<Result<Vec<_>, _>>()
            .expect("rows");
        rows
    }
}

const HONOURED: &str = "While paused, ThruLine reads nothing from this folder";
const EXCLUSIONS_HONOURED: &str =
    "A path exclusion leaves matching entries out when ThruLine shows or exports a view";

#[test]
fn every_verb_that_writes_leaves_a_row_and_says_exclusions_are_honoured() {
    let m = Machine::new();
    let (id, root) = m.project();

    // By id.
    let (code, said) = m.run(&["capture", "pause", "--project", &id]);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("Recorded: paused."), "{said}");
    assert!(
        said.contains(HONOURED) && said.contains(EXCLUSIONS_HONOURED),
        "{said}"
    );
    assert!(said.contains(&id), "the project is named by id: {said}");

    // Pausing a paused project is the route's own refusal, in its words, and
    // exit 1 because nothing was saved.
    let (code, said) = m.run(&["capture", "pause", "--project", &id]);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(
        said.trim(),
        thruline_core::capture_hold::Refusal::AlreadyPaused.sentence()
    );
    assert_eq!(m.table().len(), 1);

    // By folder, with a trailing slash.
    let (code, said) = m.run(&["capture", "resume", "--project", &format!("{root}/")]);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("Recorded: resumed."), "{said}");
    assert!(
        said.contains(HONOURED) && said.contains(EXCLUSIONS_HONOURED),
        "{said}"
    );
    let (code, said) = m.run(&["capture", "resume", "--project", &root]);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(
        said.trim(),
        thruline_core::capture_hold::Refusal::NotPaused.sentence()
    );

    let (code, said) = m.run(&["capture", "exclude", "--project", &id, "  vendor/legacy "]);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("Recorded: vendor/legacy is excluded."),
        "{said}"
    );
    assert!(
        said.contains(HONOURED) && said.contains(EXCLUSIONS_HONOURED),
        "{said}"
    );
    let (code, said) = m.run(&["capture", "exclude", "--project", &id, "vendor/legacy"]);
    assert_eq!(code, Some(1), "{said}");
    let (code, said) = m.run(&["capture", "exclude", "--project", &id, "env"]);
    assert_eq!(code, Some(1), "env is on by default already: {said}");
    assert_eq!(
        said.trim(),
        thruline_core::capture_hold::Refusal::AlreadyExcluded.sentence()
    );

    // The first change to a default is two rows, the person on the second.
    let (code, said) = m.run(&["capture", "include", "--project", &id, "env"]);
    assert_eq!(code, Some(0), "{said}");
    assert!(
        said.contains("Recorded: env is no longer excluded."),
        "{said}"
    );
    let (code, said) = m.run(&["capture", "include", "--project", &id, "env"]);
    assert_eq!(code, Some(1), "{said}");

    let table = m.table();
    let shape: Vec<(&str, Option<&str>, &str)> = table
        .iter()
        .map(|(act, pattern, actor, _, _)| (act.as_str(), pattern.as_deref(), actor.as_str()))
        .collect();
    assert_eq!(
        shape,
        [
            ("paused", None, "robin"),
            ("resumed", None, "robin"),
            ("excluded", Some("vendor/legacy"), "robin"),
            ("excluded", Some("env"), "ThruLine default"),
            ("included", Some("env"), "robin"),
        ]
    );
    assert_eq!(
        table[1].3.as_deref(),
        Some(table[0].4.as_str()),
        "resume ends the pause"
    );
    assert_eq!(
        table[4].3.as_deref(),
        Some(table[3].4.as_str()),
        "the inclusion ends the default"
    );

    // The chain adds up.
    let (code, text) = m.run(&["verify"]);
    assert_eq!(code, Some(0), "{text}");
    assert!(
        text.contains("capture holds  chain re-checked, 5 links add up"),
        "{text}"
    );
}

#[test]
fn holds_says_what_stands_without_writing_and_says_exclusions_are_honoured() {
    let m = Machine::new();
    let (id, _) = m.project();

    let (code, said) = m.run(&["capture", "holds", "--project", &id]);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("paused       no"), "{said}");
    assert!(said.contains("on by default (not recorded)"), "{said}");
    assert!(
        said.contains("off by default (not recorded)"),
        "generated: {said}"
    );
    assert!(
        said.contains(HONOURED) && said.contains(EXCLUSIONS_HONOURED),
        "{said}"
    );
    assert_eq!(m.table().len(), 0, "reading wrote nothing");

    m.run(&["capture", "pause", "--project", &id]);
    m.run(&["capture", "include", "--project", &id, "secrets"]);
    m.run(&["capture", "exclude", "--project", &id, "docs/*.md"]);
    let (code, said) = m.run(&["capture", "holds", "--project", &id]);
    assert_eq!(code, Some(0), "{said}");
    assert!(said.contains("paused       yes, since "), "{said}");
    assert!(said.contains("by robin"), "{said}");
    let secrets = said
        .lines()
        .find(|l| l.contains("secrets"))
        .expect("a secrets line");
    assert!(secrets.contains("not excluded"), "{secrets}");
    assert!(
        !secrets.contains("by default"),
        "a recorded state is not 'by default': {secrets}"
    );
    let docs = said
        .lines()
        .find(|l| l.contains("docs/*.md"))
        .expect("a typed path line");
    assert!(
        docs.contains("excluded") && docs.contains("by robin"),
        "{docs}"
    );
    assert!(
        said.contains(HONOURED) && said.contains(EXCLUSIONS_HONOURED),
        "{said}"
    );
    assert_eq!(
        m.table().len(),
        4,
        "pause, the default written down and ended, the path"
    );
}

#[test]
fn a_project_nobody_has_is_said_so_and_a_bad_path_is_named_and_nothing_is_saved() {
    let m = Machine::new();
    let (id, _) = m.project();

    for args in [
        ["capture", "pause", "--project", "no-such-project"],
        ["capture", "resume", "--project", "/no/such/folder"],
        ["capture", "holds", "--project", "no-such-project"],
    ] {
        let (code, said) = m.run(&args);
        assert_eq!(code, Some(1), "{args:?}: {said}");
        assert!(said.contains("No project in this record matches"), "{said}");
        assert!(said.contains("Nothing was saved"), "{said}");
    }
    for bad in ["", "/etc/passwd", " ../outside", "src/?.rs", "a[b]"] {
        let (code, said) = m.run(&["capture", "exclude", "--project", &id, bad]);
        assert_eq!(code, Some(1), "{bad:?}: {said}");
        assert!(said.contains("That cannot be left out"), "{bad:?}: {said}");
        assert!(said.contains("Nothing was saved"), "{bad:?}: {said}");
    }
    let (code, said) = m.run(&["capture", "include", "--project", &id, "../outside"]);
    assert_eq!(code, Some(1), "{said}");
    assert_eq!(m.table().len(), 0, "every refusal appended nothing");
}

#[test]
fn no_verb_prints_the_projects_folder_or_a_home_path() {
    let m = Machine::new();
    let (id, root) = m.project();
    let home = m.home().display().to_string();
    for args in [
        vec!["capture", "pause", "--project", id.as_str()],
        vec!["capture", "holds", "--project", id.as_str()],
        vec!["capture", "exclude", "--project", id.as_str(), "vendor"],
        vec!["capture", "include", "--project", id.as_str(), "env"],
        vec!["capture", "resume", "--project", id.as_str()],
        vec!["capture", "pause", "--project", "nope"],
    ] {
        let (_, said) = m.run(&args);
        assert!(!said.contains(&root), "{args:?} printed the folder: {said}");
        assert!(
            !said.contains(&home),
            "{args:?} printed a home path: {said}"
        );
        assert!(!said.contains("~/"), "{args:?}: {said}");
    }
}

#[test]
fn status_keeps_its_meaning_and_the_new_verbs_are_listed_beside_it() {
    let m = Machine::new();
    let (code, help) = m.run(&["capture", "--help"]);
    assert_eq!(code, Some(0), "{help}");
    for verb in ["pause", "resume", "exclude", "include", "holds", "status"] {
        assert!(
            help.lines().any(|l| l.trim_start().starts_with(verb)),
            "`capture --help` lists {verb}: {help}"
        );
    }
    // `capture status` is still Assured Capture's: it asks for its own
    // configuration and does not take a project.
    let out = m.cmd(&["capture", "status"]).output().expect("status");
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(
        err.contains("--source") && err.contains("--capture-root"),
        "{err}"
    );
}
