//! C29: the guard every `#[ignore]` probe calls before it opens a store named
//! by the environment. It refuses any path inside the live store
//! (`$HOME/.thruline`, through symlinks, and a non-scratch `$THRULINE_HOME`),
//! and lets a clone anywhere else through.

#[path = "support/live_store.rs"]
mod live_store;

use std::path::{Path, PathBuf};

use live_store::{live_store_refusal, refuse_live_store, resolved, scratch_roots};

/// A fake HOME with a live store in it, and a scratch folder beside it.
struct Machine {
    dir: tempfile::TempDir,
}

impl Machine {
    fn new() -> Machine {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(dir.path().join("home/.thruline")).expect("store");
        std::fs::write(dir.path().join("home/.thruline/ledger.db"), b"").expect("ledger");
        std::fs::create_dir_all(dir.path().join("scratch")).expect("scratch");
        Machine { dir }
    }
    fn home(&self) -> PathBuf {
        self.dir.path().join("home")
    }
    fn at(&self, rel: &str) -> PathBuf {
        self.dir.path().join(rel)
    }
    /// Refusal with no scratch roots: every THRULINE_HOME counts as live.
    fn refuses(&self, path: &Path, thruline_home: Option<&Path>) -> bool {
        live_store_refusal(path, Some(&self.home()), thruline_home, &[]).is_some()
    }
}

#[test]
fn the_live_ledger_and_anything_beside_it_is_refused() {
    let m = Machine::new();
    assert!(m.refuses(&m.at("home/.thruline/ledger.db"), None));
    // Not there yet (a -wal, a new mirror): judged by where it would be made.
    assert!(m.refuses(&m.at("home/.thruline/ledger.db-wal"), None));
    assert!(m.refuses(&m.at("home/.thruline/mirrors/x/y.jsonl"), None));
    assert!(m.refuses(&m.at("home/.thruline"), None));
    // Spelled the long way round.
    assert!(m.refuses(&m.at("scratch/../home/.thruline/ledger.db"), None));
    let why = live_store_refusal(
        &m.at("home/.thruline/ledger.db"),
        Some(&m.home()),
        None,
        &[],
    )
    .expect("refused");
    assert!(why.contains("cp -c"), "{why}");
}

#[test]
fn a_symlink_into_the_live_store_is_refused() {
    let m = Machine::new();
    std::os::unix::fs::symlink(m.at("home/.thruline"), m.at("scratch/alias")).expect("link");
    assert!(m.refuses(&m.at("scratch/alias/ledger.db"), None));
    std::os::unix::fs::symlink(m.at("home/.thruline/ledger.db"), m.at("scratch/l.db"))
        .expect("link");
    assert!(m.refuses(&m.at("scratch/l.db"), None));
}

#[test]
fn a_clone_outside_the_live_store_is_let_through() {
    let m = Machine::new();
    std::fs::write(m.at("scratch/ledger.db"), b"").expect("clone");
    assert!(!m.refuses(&m.at("scratch/ledger.db"), None));
    assert!(!m.refuses(&m.at("scratch/new/ledger.db"), None));
    // A sibling whose name only starts the same way is not inside it.
    assert!(!m.refuses(&m.at("home/.thruline-bench/ledger.db"), None));
}

#[test]
fn a_thruline_home_that_is_not_scratch_is_live_too() {
    let m = Machine::new();
    std::fs::create_dir_all(m.at("elsewhere")).expect("dir");
    let th = m.at("elsewhere");
    assert!(m.refuses(&m.at("elsewhere/ledger.db"), Some(&th)));
    // The same folder, when it lies under a scratch root, is a scratch store.
    let scratch = [resolved(m.dir.path())];
    assert!(live_store_refusal(
        &m.at("elsewhere/ledger.db"),
        Some(&m.home()),
        Some(&th),
        &scratch
    )
    .is_none());
    // ... but $HOME/.thruline stays live wherever it is.
    assert!(live_store_refusal(
        &m.at("home/.thruline/ledger.db"),
        Some(&m.home()),
        Some(&th),
        &scratch
    )
    .is_some());
}

#[test]
fn the_os_temp_folder_is_a_scratch_root() {
    let roots = scratch_roots();
    assert!(
        roots.contains(&resolved(&std::env::temp_dir())),
        "{roots:?}"
    );
}

/// The wrapper reads the real environment: the real live ledger is refused
/// before anything opens it (the guard only resolves the path).
#[test]
#[should_panic(expected = "is inside the live store")]
fn the_wrapper_refuses_the_real_live_ledger() {
    let home = std::env::var_os("HOME").map(PathBuf::from).expect("HOME");
    refuse_live_store(&home.join(".thruline/ledger.db"));
}
