//! Serve-time path exclusions also apply to files written by `thruline export`.
//! Every child uses a temporary HOME and an explicit temporary ledger path.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;
use tempfile::TempDir;

const STAMP: &str = "2026-10-08T00:00:00Z";

struct World {
    temp: TempDir,
}

impl World {
    fn new() -> Self {
        let temp = tempfile::tempdir().expect("temporary world");
        std::fs::create_dir_all(temp.path().join("home/.thruline")).expect("temporary home");
        std::fs::create_dir_all(temp.path().join("repo/.git")).expect("temporary repo");
        Self { temp }
    }

    fn home(&self) -> PathBuf {
        self.temp.path().join("home")
    }

    fn repo(&self) -> PathBuf {
        self.temp.path().join("repo")
    }

    fn db(&self) -> PathBuf {
        self.home().join(".thruline/ledger.db")
    }

    fn transcript_dir(&self) -> PathBuf {
        let munged: String = self
            .repo()
            .display()
            .to_string()
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
            .collect();
        self.home().join(".claude/projects").join(munged)
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_thruline"))
            .args(["--db", self.db().to_str().expect("UTF-8 temp path")])
            .args(args)
            .env("HOME", self.home())
            .env("THRULINE_HOME", self.home().join(".thruline"))
            .env("USER", "synthetic-person")
            .env_remove("THRULINE_EXPORT_PROFILE")
            .current_dir(self.repo())
            .output()
            .expect("run thruline")
    }

    fn record_and_ingest(&self) -> String {
        let dir = self.transcript_dir();
        std::fs::create_dir_all(&dir).expect("transcript directory");
        let cwd = self.repo().display().to_string();
        let line = |n: u8, kind: &str, message: Value| {
            serde_json::json!({
                "type": kind,
                "uuid": format!("session-{n}"),
                "parentUuid": null,
                "isSidechain": false,
                "sessionId": "synthetic-session",
                "timestamp": format!("2026-10-08T09:0{n}:00.000Z"),
                "cwd": cwd,
                "message": message
            })
            .to_string()
        };
        let transcript = [
            line(
                1,
                "user",
                serde_json::json!({"role":"user","content":"Please edit src/a.rs."}),
            ),
            line(
                2,
                "assistant",
                serde_json::json!({"role":"assistant","content":[{
                    "type":"tool_use","id":"tool-1","name":"Edit",
                    "input":{"file_path":format!("{cwd}/src/a.rs"),
                             "old_string":"before","new_string":"PRIVATE_BLOCK_47"}
                }]}),
            ),
            line(
                3,
                "user",
                serde_json::json!({"role":"user","content":[{
                    "type":"tool_result","tool_use_id":"tool-1","content":"ok","is_error":false
                }]}),
            ),
            line(
                4,
                "assistant",
                serde_json::json!({"role":"assistant","content":[{
                    "type":"text","text":"Done. I updated src/a.rs."
                }]}),
            ),
        ];
        std::fs::write(
            dir.join("synthetic-session.jsonl"),
            transcript.join("\n") + "\n",
        )
        .expect("synthetic transcript");
        let out = self.run(&[
            "ingest",
            "--apply",
            "--source",
            "claude-code",
            dir.to_str().expect("UTF-8 temp path"),
        ]);
        assert!(
            out.status.success(),
            "ingest: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let ids = thruline_core::ledger::Ledger::open(self.db())
            .expect("ledger")
            .project_ids()
            .expect("project ids");
        assert_eq!(ids.len(), 1);
        ids[0].clone()
    }

    fn export(&self, dir: &Path) -> (Vec<u8>, Vec<u8>) {
        let out = self.run(&[
            "export",
            "--all",
            "--out",
            dir.to_str().expect("UTF-8 temp path"),
            "--generated-at",
            STAMP,
        ]);
        assert!(
            out.status.success(),
            "export: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let id = thruline_core::ledger::Ledger::open(self.db())
            .expect("ledger")
            .project_ids()
            .expect("project ids")
            .remove(0);
        let project = std::fs::read(dir.join("projects").join(format!(
            "{}.json",
            thruline_core::export::project_fixture_id(&id)
        )))
        .expect("project export");
        let index = std::fs::read(dir.join("index.json")).expect("index export");
        (project, index)
    }

    fn export_one(&self, id: &str, path: &Path) -> Vec<u8> {
        let project = format!("--project={id}");
        let out = self.run(&[
            "export",
            &project,
            "--out",
            path.to_str().expect("UTF-8 temp path"),
            "--generated-at",
            STAMP,
        ]);
        assert!(
            out.status.success(),
            "single-project export: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        std::fs::read(path).expect("single-project export file")
    }

    fn choice(&self, act: &str) {
        let project = format!("--project={}", self.repo().display());
        let out = self.run(&["capture", act, &project, "src/a.rs"]);
        assert!(
            out.status.success(),
            "{act} (exit {:?}): stdout={} stderr={}",
            out.status.code(),
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn standalone_export_leaves_out_an_excluded_path_and_restores_original_bytes_on_include() {
    let world = World::new();
    let id = world.record_and_ingest();
    let (before, before_index) = world.export(&world.temp.path().join("before"));
    let one_before = world.export_one(&id, &world.temp.path().join("one-before.json"));
    assert!(String::from_utf8_lossy(&before).contains("src/a.rs"));
    assert!(String::from_utf8_lossy(&one_before).contains("src/a.rs"));

    world.choice("exclude");
    let (hidden, _) = world.export(&world.temp.path().join("hidden"));
    let one_hidden = world.export_one(&id, &world.temp.path().join("one-hidden.json"));
    let view: Value = serde_json::from_slice(&hidden).expect("filtered export JSON");
    assert!(
        view["left_out_by_exclusion"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "the export counts replaced entries: {view}"
    );
    let shown = String::from_utf8_lossy(&hidden);
    assert!(shown.contains("left out by your exclusion (`src/a.rs`)"));
    assert!(
        !shown.contains(&format!("{}/src/a.rs", world.repo().display())),
        "the original absolute path must not remain in the file"
    );
    let one_view: Value = serde_json::from_slice(&one_hidden).expect("filtered single export JSON");
    assert!(
        one_view["left_out_by_exclusion"]
            .as_u64()
            .is_some_and(|n| n > 0),
        "the single export counts replaced entries: {one_view}"
    );
    let one_shown = String::from_utf8_lossy(&one_hidden);
    assert!(one_shown.contains("left out by your exclusion (`src/a.rs`)"));
    assert!(
        !one_shown.contains(&format!("{}/src/a.rs", world.repo().display())),
        "the original absolute path must not remain in the single export"
    );

    world.choice("include");
    let (restored, restored_index) = world.export(&world.temp.path().join("restored"));
    let one_restored = world.export_one(&id, &world.temp.path().join("one-restored.json"));
    assert_eq!(
        restored, before,
        "include restores the project export byte for byte"
    );
    assert_eq!(
        restored_index, before_index,
        "include restores the index byte for byte"
    );
    assert_eq!(
        one_restored, one_before,
        "include restores the single-project export byte for byte"
    );
}
