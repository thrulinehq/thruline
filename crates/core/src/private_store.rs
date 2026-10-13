//! Owner-only permissions for the ThruLine home and the stores inside it.
//!
//! `~/.thruline` holds unredacted transcript for every project the machine
//! has ever captured. The engine's privacy guarantee is "nothing leaves the
//! machine"; it says nothing about the other accounts **on** the machine, and
//! until this module existed the process umask decided. Measured on the
//! author's own install: `protected/` and `mirrors/` were `0700` — because
//! [`crate::mirror`] and [`crate::capture::secure_dir`] set them explicitly —
//! while the home directory itself, `ledger.db` and `exec.db` were
//! world-readable. This applies the same approach those two already take to
//! the home directory and the databases, at open/create time, so a fresh
//! install is private by construction rather than by a chmod someone
//! remembers to run.
//!
//! Three properties, all required:
//!
//! * **Idempotent, and narrowing only.** A call reads the current mode and
//!   clears the bits outside the target, so running it again is the same as
//!   running it once and a store that predates this module is repaired on its
//!   next open. It can only ever REMOVE access: assigning the target mode
//!   outright would grant permission as readily as it removes it, and silently
//!   handing write access back to a store an operator had made read-only is a
//!   worse failure than the one this module exists to fix.
//! * **Advisory.** A chmod that fails is reported through `tracing` and the
//!   command carries on. Refusing to open a ledger because a permission bit
//!   would not set trades a privacy improvement for an outage, and the
//!   failure mode this guards against is passive (another local account
//!   reading the file), not active.
//! * **Unix-gated.** On other platforms every function here is a no-op,
//!   stated rather than assumed. Windows ACLs are a different model and
//!   nothing in this tree has measured them.
//!
//! # What this does not do
//!
//! It does not redact — see `SECURITY.md` and [`crate::redact`] for where
//! redaction actually runs. It does not reach the home's other contents
//! (`narrate.db`, `sensor-spill.jsonl`, `spool/`, `vault/`, `export/`).
//! Those are covered *transitively*: a `0700` home cannot be traversed by
//! another local account, so their own modes stop being reachable. Their
//! modes themselves are whatever their writers chose, and files already on
//! disk keep the mode they were created with — only the databases named to
//! [`harden_store`] are re-asserted.

use std::path::{Path, PathBuf};

/// Owner-only directory: `rwx` for the owner, nothing for anyone else.
///
/// Ungated so the non-Unix build still type-checks the call sites; the mode
/// is simply never applied there.
const DIR_MODE: u32 = 0o700;

/// Owner-only file: `rw` for the owner, nothing for anyone else.
const FILE_MODE: u32 = 0o600;

/// `$THRULINE_HOME`, else `~/.thruline`.
///
/// The same resolution `thruline_ledger::Ledger::default_path` and the
/// capture spool use. `None` when neither variable is set, which is a real
/// state on a stripped environment and not an error here — a caller that
/// cannot locate the home simply has no home to harden.
pub fn thruline_home() -> Option<PathBuf> {
    std::env::var_os("THRULINE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".thruline")))
}

/// Create `path` and any missing parents owner-only.
///
/// A drop-in for `fs::create_dir_all` that closes the window in which a
/// newly created directory is world-readable: the mode is passed to the
/// creating syscall rather than applied afterwards. Directories that
/// already exist keep their mode — [`harden_dir`] is the deliberate,
/// separately scoped repair for those.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(DIR_MODE)
            .create(path)?;
    }
    #[cfg(not(unix))]
    std::fs::create_dir_all(path)?;
    Ok(())
}

/// Re-assert owner-only permissions on a directory ThruLine owns.
///
/// Best-effort: a failure is logged, never returned.
pub fn harden_dir(path: &Path) {
    set_private_mode(path, DIR_MODE, "directory");
}

/// Re-assert owner-only permissions on a SQLite store, its WAL sidecars,
/// and the ThruLine home when that is where the store lives.
///
/// The sidecars matter as much as the database: `-wal` carries committed
/// rows verbatim until the next checkpoint. SQLite copies the database
/// file's mode onto a WAL or shared-memory file it creates, so tightening
/// the database *before* the `journal_mode` pragma runs is what makes new
/// sidecars private; the explicit pass here covers sidecars that already
/// exist from an earlier, looser open.
///
/// The parent directory is tightened **only** when it is this process's
/// ThruLine home. `--db` can name any path — a repository checkout, a
/// shared scratch directory, a test's temporary directory — and chmodding
/// whatever directory happens to contain a database is a side effect nobody
/// asked for.
pub fn harden_store(path: &Path) {
    set_private_mode(path, FILE_MODE, "database");
    for suffix in ["-wal", "-shm"] {
        let mut sidecar = path.as_os_str().to_os_string();
        sidecar.push(suffix);
        set_private_mode(Path::new(&sidecar), FILE_MODE, "database sidecar");
    }
    if let (Some(parent), Some(home)) = (path.parent(), thruline_home()) {
        if parent == home {
            harden_dir(parent);
        }
    }
}

/// Set `mode` on `path`, reporting rather than failing.
///
/// An absent path is silence, not a warning: `harden_store` names sidecars
/// that usually do not exist yet.
#[cfg(unix)]
fn set_private_mode(path: &Path, mode: u32, what: &str) {
    use std::os::unix::fs::PermissionsExt;

    // `set_permissions` resolves the path again and follows symlinks, so a
    // symlinked store would silently chmod its target. Report and skip.
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            tracing::warn!(
                path = %path.display(),
                "{what} is a symlink; leaving its permissions alone"
            );
        }
        Ok(metadata) => {
            // MASK DOWN, NEVER SET. `mode` is a ceiling: the new mode is the
            // old one with everything outside `mode` cleared, so this can only
            // ever remove access.
            //
            // Assigning `mode` outright would GRANT permission as readily as it
            // removes it. An operator who chmods a store to 0400 to keep it
            // read-only would have ThruLine hand itself write access back on
            // the next open, silently — and `m3_stop_gate`'s
            // `a_block_the_gate_cannot_write_down_is_not_issued_and_says_why`
            // caught exactly that: it makes `exec.db` read-only to prove the
            // gate refuses a block it cannot record, and a fixed 0600 undid the
            // premise and the refusal with it.
            let current = metadata.permissions().mode() & 0o7777;
            let restricted = current & mode;
            if restricted == current {
                return;
            }
            if let Err(error) =
                std::fs::set_permissions(path, std::fs::Permissions::from_mode(restricted))
            {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "could not restrict {what} permissions; it may be readable by other local accounts"
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(
                path = %path.display(),
                %error,
                "could not inspect {what} permissions"
            );
        }
    }
}

#[cfg(not(unix))]
fn set_private_mode(_path: &Path, _mode: u32, _what: &str) {}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// The regression `m3_stop_gate` caught: hardening must never GRANT.
    #[test]
    fn hardening_never_widens_permissions_it_only_narrows_them() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("tl-narrow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let db = dir.join("read-only.db");
        std::fs::write(&db, b"x").expect("write");

        // An operator deliberately makes the store read-only.
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o400)).expect("chmod");
        harden_store(&db);
        let after = std::fs::metadata(&db).expect("stat").permissions().mode() & 0o777;
        assert_eq!(
            after, 0o400,
            "hardening granted owner-write back on a deliberately read-only store"
        );

        // And it still does its actual job on a world-readable one.
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).expect("chmod");
        harden_store(&db);
        let after = std::fs::metadata(&db).expect("stat").permissions().mode() & 0o777;
        assert_eq!(
            after, 0o600,
            "hardening failed to remove group/other access"
        );

        std::fs::remove_dir_all(&dir).ok();
    }
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &Path) -> u32 {
        std::fs::symlink_metadata(path)
            .expect("stat")
            .permissions()
            .mode()
            & 0o777
    }

    #[test]
    fn a_newly_created_store_is_not_group_or_other_readable() {
        let home = tempfile::tempdir().expect("tempdir");
        let db = home.path().join("nested").join("ledger.db");

        let ledger = crate::ledger::Ledger::open(&db).expect("open");
        drop(ledger);

        assert!(db.exists(), "the store was not created");
        assert_eq!(
            mode_of(&db) & 0o077,
            0,
            "a new store is readable by group or other: {:o}",
            mode_of(&db)
        );
        assert_eq!(
            mode_of(db.parent().expect("parent")) & 0o077,
            0,
            "the directory created for a new store is traversable by group or other"
        );
        for suffix in ["-wal", "-shm"] {
            let mut sidecar = db.clone().into_os_string();
            sidecar.push(suffix);
            let sidecar = PathBuf::from(sidecar);
            if sidecar.exists() {
                assert_eq!(
                    mode_of(&sidecar) & 0o077,
                    0,
                    "a WAL sidecar is readable by group or other"
                );
            }
        }
    }

    #[test]
    fn hardening_an_existing_world_readable_store_is_idempotent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("exec.db");
        std::fs::write(&db, b"not really a database").expect("write");
        std::fs::set_permissions(&db, std::fs::Permissions::from_mode(0o644)).expect("chmod");

        harden_store(&db);
        assert_eq!(mode_of(&db), FILE_MODE);
        harden_store(&db);
        assert_eq!(mode_of(&db), FILE_MODE);
    }

    #[test]
    fn a_missing_store_is_silence_rather_than_a_failure() {
        let dir = tempfile::tempdir().expect("tempdir");
        // No panic, no error: the sidecars named by `harden_store` normally
        // do not exist, and neither does a store that failed to open.
        harden_store(&dir.path().join("absent.db"));
    }

    #[test]
    fn a_store_outside_the_thruline_home_leaves_its_directory_alone() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755))
            .expect("chmod");
        let db = dir.path().join("scratch.db");
        std::fs::write(&db, b"x").expect("write");

        harden_store(&db);

        assert_eq!(
            mode_of(dir.path()),
            0o755,
            "a directory that is not the ThruLine home was chmodded anyway"
        );
        assert_eq!(
            mode_of(&db),
            FILE_MODE,
            "the store itself was not tightened"
        );
    }
}
