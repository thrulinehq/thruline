//! A preparation is an observation, not permission to run project programs.
//! Q20b found the planted cargo ran with `--version` before the guarded POST.
//! These tests take that same CLI door with synthetic projects and tool homes.

#![cfg(unix)]

use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::Command;

const PROJECT: &str = "prepare-tool-project";
const CHANGE: &str = "prepare-tool-change";

struct Fixture {
    _dir: tempfile::TempDir,
    home: PathBuf,
    root: PathBuf,
    db: PathBuf,
    marker: PathBuf,
    bin: PathBuf,
    rustup_home: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("synthetic fixture");
        let home = dir.path().canonicalize().expect("fixture path");
        let root = home.join("project");
        let bin = home.join("bin");
        let rustup_home = home.join("rustup");
        for folder in [&root, &bin, &rustup_home, &home.join("cargo-home")] {
            std::fs::create_dir_all(folder).expect("fixture folder");
        }
        std::fs::write(root.join("README.md"), "synthetic project\n").expect("project file");
        for args in [
            vec!["init", "-q"],
            vec!["add", "."],
            vec!["commit", "-qm", "synthetic project"],
        ] {
            let out = Command::new("/usr/bin/git")
                .args(args)
                .current_dir(&root)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_AUTHOR_NAME", "synthetic")
                .env("GIT_AUTHOR_EMAIL", "synthetic@example.invalid")
                .env("GIT_COMMITTER_NAME", "synthetic")
                .env("GIT_COMMITTER_EMAIL", "synthetic@example.invalid")
                .output()
                .expect("fixture git");
            assert!(out.status.success(), "{out:?}");
        }
        let db = home.join("ledger.db");
        let ledger = thruline_core::ledger::Ledger::open(&db).expect("fixture ledger");
        ledger
            .conn()
            .execute(
                "INSERT INTO projects(id, root_path, source, display_name, created_at)
             VALUES(?1, ?2, 'claude_code', 'Synthetic', '2026-01-01T00:00:00Z')",
                rusqlite::params![PROJECT, root.display().to_string()],
            )
            .expect("fixture project");
        thruline_core::change::insert_change(
            &ledger,
            &thruline_core::change::NewChange {
                id: CHANGE.into(),
                project_id: PROJECT.into(),
                opened_at: "2026-01-01T00:00:00Z".into(),
                request: thruline_core::change::ChangeRequest {
                    title: "Synthetic tool observation".into(),
                    purpose: String::new(),
                    prompt_event_id: None,
                    requester: "sam".into(),
                    source: thruline_core::change::RequestSource::Person,
                },
            },
        )
        .expect("fixture change");
        let version = thruline_core::change::append_scope(
            &ledger,
            CHANGE,
            "2026-01-01T00:00:01Z",
            &thruline_core::change::AcceptedScope {
                criteria: vec!["The synthetic check passes.".into()],
                check_profile: vec!["cargo-test".into()],
                ..Default::default()
            },
        )
        .expect("fixture scope");
        thruline_core::change::accept_scope(
            &ledger,
            CHANGE,
            version,
            &thruline_core::change::ScopeAcceptance {
                actor: "sam".into(),
                accepted_at: "2026-01-01T00:00:02Z".into(),
                rationale: String::new(),
            },
        )
        .expect("fixture acceptance");
        drop(ledger);
        let marker = home.join("marker");
        Self {
            _dir: dir,
            home,
            root,
            db,
            marker,
            bin,
            rustup_home,
        }
    }

    fn plant(&self, folder: &Path) {
        std::fs::create_dir_all(folder).expect("synthetic tool folder");
        let program = folder.join("cargo");
        std::fs::write(
            &program,
            format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nprintf '%s\\n' 'cargo synthetic-version'\n",
            self.marker.display(),
        ),
        )
        .expect("synthetic cargo");
        std::fs::set_permissions(program, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    fn proxies(&self) {
        let path = std::env::var_os("PATH").expect("test search path");
        let source = std::env::split_paths(&path)
            .map(|dir| dir.join("rustup"))
            .find(|p| p.is_file())
            .expect("could not run: this regression requires an installed rustup");
        std::fs::copy(source, self.bin.join("rustup")).expect("copy rustup into fixture");
        symlink("rustup", self.bin.join("cargo")).expect("owned cargo proxy");
    }

    fn prepare(&self, first: &Path, toolchain: Option<&Path>, from: &Path) -> serde_json::Value {
        let path = std::env::join_paths([first, Path::new("/usr/bin"), Path::new("/bin")])
            .expect("fixture search path");
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_thruline"));
        cmd.current_dir(from)
            .env("HOME", &self.home)
            .env("THRULINE_HOME", self.home.join(".thruline"))
            .env("PATH", path)
            .env("CARGO_HOME", self.home.join("cargo-home"))
            .env("RUSTUP_HOME", &self.rustup_home)
            .env_remove("RUSTUP_TOOLCHAIN")
            .env("RUSTUP_AUTO_INSTALL", "0")
            .env("RUSTUP_DIST_SERVER", "file:///nonexistent-dist")
            .env("RUSTUP_UPDATE_ROOT", "file:///nonexistent-update")
            .arg("--db")
            .arg(&self.db)
            .args(["change", "prepare", CHANGE, "--root"])
            .arg(&self.root)
            .arg("--json");
        if let Some(toolchain) = toolchain {
            cmd.env("RUSTUP_TOOLCHAIN", toolchain);
        }
        let out = cmd.output().expect("prepare CLI");
        assert!(out.status.success(), "{out:?}");
        let value: serde_json::Value = serde_json::from_slice(&out.stdout).expect("prepare JSON");
        let tools = value["prepare"]["inputs"]["tools"]
            .as_array()
            .expect("recorded tools");
        assert_eq!(tools.len(), 1, "{value}");
        assert_eq!(tools[0]["name"], "cargo", "{value}");
        tools[0].clone()
    }

    fn assert_not_started(&self, tool: &serde_json::Value) {
        assert!(
            !self.marker.exists(),
            "prepare ran the planted cargo: {}",
            std::fs::read_to_string(&self.marker).unwrap_or_default()
        );
        assert_eq!(tool["version"]["state"], "unknown", "{tool}");
        assert!(tool["version"].get("value").is_none(), "{tool}");
    }
}

#[test]
fn prepare_does_not_run_a_project_tool_from_path() {
    let f = Fixture::new();
    let local = f.root.join("node_modules/.bin");
    f.plant(&local);
    f.assert_not_started(&f.prepare(&local, None, &f.root));
}

#[test]
fn prepare_does_not_run_a_project_tool_through_rustup_toolchain_file() {
    let f = Fixture::new();
    f.proxies();
    let toolchain = f.root.join("toolchain");
    f.plant(&toolchain.join("bin"));
    std::fs::write(
        f.root.join("rust-toolchain.toml"),
        format!(
            "[toolchain]\npath = {:?}\n",
            toolchain.display().to_string(),
        ),
    )
    .expect("toolchain file");
    f.assert_not_started(&f.prepare(&f.bin, None, &f.root));
}

#[test]
fn prepare_does_not_follow_a_symlink_toolchain_file() {
    let f = Fixture::new();
    f.proxies();
    let toolchain = f.root.join("toolchain");
    f.plant(&toolchain.join("bin"));
    std::fs::write(
        f.root.join("toolchain-data"),
        format!(
            "[toolchain]\npath = {:?}\n",
            toolchain.display().to_string()
        ),
    )
    .expect("toolchain data");
    symlink("toolchain-data", f.root.join("rust-toolchain.toml")).expect("toolchain link");
    f.assert_not_started(&f.prepare(&f.bin, None, &f.root));
}

#[test]
fn prepare_does_not_read_a_socket_toolchain_file() {
    let f = Fixture::new();
    f.proxies();
    // Bind under a short owned name, then move the inode to the toolchain
    // path: a long lane TMPDIR can exceed the Unix socket address limit.
    let socket = f.home.join("s");
    let _listener =
        std::os::unix::net::UnixListener::bind(&socket).expect("owned toolchain socket");
    std::fs::rename(socket, f.root.join("rust-toolchain.toml")).expect("toolchain socket");
    f.assert_not_started(&f.prepare(&f.bin, None, &f.root));
}

#[test]
fn prepare_does_not_read_a_fifo_toolchain_file() {
    use rustix::fs::{open, Mode, OFlags};
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::{Duration, Instant};

    let f = Fixture::new();
    f.proxies();
    let toolchain = f.root.join("toolchain");
    f.plant(&toolchain.join("bin"));
    let fifo = f.root.join("rust-toolchain.toml");
    assert!(Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo")
        .success());
    let text = format!(
        "[toolchain]\npath = {:?}\n",
        toolchain.display().to_string()
    );
    let done = AtomicBool::new(false);
    std::thread::scope(|scope| {
        // The unfixed probe reads this as rustup data. Supply its answer so
        // the red regression witnesses execution instead of hanging on open.
        // Nonblocking opens also let the writer end when the fixed guard
        // refuses the FIFO without ever opening it.
        let writer = scope.spawn(|| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::SeqCst) && Instant::now() < deadline {
                if let Ok(fd) = open(&fifo, OFlags::WRONLY | OFlags::NONBLOCK, Mode::empty()) {
                    let mut file = std::fs::File::from(fd);
                    let _ = file.write_all(text.as_bytes());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        });
        let observed = f.prepare(&f.bin, None, &f.root);
        done.store(true, Ordering::SeqCst);
        writer.join().expect("FIFO writer stopped");
        f.assert_not_started(&observed);
    });
}

#[test]
fn prepare_does_not_run_a_project_tool_through_environment_override() {
    let f = Fixture::new();
    f.proxies();
    let toolchain = f.root.join("toolchain");
    f.plant(&toolchain.join("bin"));
    f.assert_not_started(&f.prepare(&f.bin, Some(&toolchain), &f.root));
}

#[test]
fn prepare_does_not_run_a_project_tool_through_settings_override() {
    let f = Fixture::new();
    f.proxies();
    let toolchain = f.root.join("toolchain");
    f.plant(&toolchain.join("bin"));
    std::fs::create_dir_all(f.rustup_home.join("toolchains")).expect("toolchains");
    symlink(&toolchain, f.rustup_home.join("toolchains/synthetic")).expect("linked toolchain");
    std::fs::write(
        f.rustup_home.join("settings.toml"),
        format!(
            "version = \"12\"\n[overrides]\n{:?} = \"synthetic\"\n",
            f.root.display().to_string(),
        ),
    )
    .expect("directory override");
    f.assert_not_started(&f.prepare(&f.bin, None, &f.root));
}

#[test]
fn prepare_still_observes_an_outside_tool_and_records_absence_honestly() {
    let f = Fixture::new();
    let missing = f.prepare(&f.bin, None, &f.root);
    assert_eq!(missing["present"], false, "{missing}");
    f.assert_not_started(&missing);
    f.plant(&f.bin);
    let known = f.prepare(&f.bin, None, &f.root);
    assert_eq!(known["present"], true, "{known}");
    assert_eq!(known["version"]["state"], "known", "{known}");
    assert_eq!(
        known["version"]["value"], "cargo synthetic-version",
        "{known}"
    );
    assert_eq!(
        std::fs::read_to_string(&f.marker).expect("probe marker"),
        "--version\n"
    );
}

#[test]
fn prepare_probes_the_selected_root_even_when_invoked_elsewhere() {
    let f = Fixture::new();
    f.proxies();
    let toolchain = f.home.join("outside-toolchain");
    f.plant(&toolchain.join("bin"));
    std::fs::write(
        f.root.join("rust-toolchain.toml"),
        format!(
            "[toolchain]\npath = {:?}\n",
            toolchain.display().to_string(),
        ),
    )
    .expect("outside toolchain file");
    let known = f.prepare(&f.bin, None, &f.home);
    assert_eq!(
        known["version"]["value"], "cargo synthetic-version",
        "{known}"
    );
    assert_eq!(
        std::fs::read_to_string(&f.marker).expect("probe marker"),
        "--version\n"
    );
}
