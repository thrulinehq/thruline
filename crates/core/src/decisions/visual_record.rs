//! Prepare only a disposable C1 snapshot with its exact version's native CLI.
//! Recorded event count, project heads and the selected clock witness must
//! survive. This code never upgrades the preserved source record.
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

#[path = "visual_upgrade_space.rs"]
mod upgrade_space;

const LOG_LIMIT: u64 = 65_536;
const OUTPUT_LIMIT: u64 = 1 << 20;
const UPGRADE_LIMIT: Duration = Duration::from_secs(30 * 60);

#[derive(Debug, PartialEq, Eq)]
struct Facts {
    event_count: Option<i64>,
    heads: Option<Vec<(String, String, i64)>>,
    witness: Option<ClockFacts>,
}

#[derive(Debug, PartialEq, Eq)]
struct ClockFacts {
    timestamp: Option<String>,
    sequence: i64,
    raw_line: Option<i64>,
    raw_sha256: Option<String>,
    chain_hash: Option<String>,
}

pub fn prepare_exact_snapshot(
    server: &Path,
    snapshot: &Path,
    work: &Path,
    witness_event_id: Option<&str>,
) -> Result<(), &'static str> {
    prepare_exact_snapshot_with_timeout(server, snapshot, work, witness_event_id, UPGRADE_LIMIT)
}

/// Callers can impose a stricter deadline; no caller can extend the bound.
pub fn prepare_exact_snapshot_with_timeout(
    server: &Path,
    snapshot: &Path,
    work: &Path,
    witness_event_id: Option<&str>,
    timeout: Duration,
) -> Result<(), &'static str> {
    let work = private_work(work)?;
    let meta = std::fs::symlink_metadata(snapshot).map_err(|_| "capture snapshot is missing")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if meta.nlink() != 1 {
            return Err("native upgrade requires an unshared disposable capture snapshot");
        }
    }
    let snapshot = snapshot
        .canonicalize()
        .map_err(|_| "capture snapshot is unavailable")?;
    if !meta.is_file()
        || meta.file_type().is_symlink()
        || snapshot.parent() != Some(work.as_path())
        || snapshot.file_name() != Some("ledger.db".as_ref())
    {
        return Err(
            "native upgrade requires a disposable capture snapshot inside its private work folder",
        );
    }
    let before = facts(&snapshot, witness_event_id)?;
    let probe_stamp = help_stamp(&snapshot)?;
    let deadline = Instant::now() + timeout.min(UPGRADE_LIMIT);
    let probe = native(server, &snapshot, &work, true, deadline)?;
    if probe_stamp != help_stamp(&snapshot)? || before != facts(&snapshot, witness_event_id)? {
        return Err("native capture help changed its snapshot schema, bytes or recorded facts");
    }
    if probe.code == Some(2)
        && (probe.stderr.contains("unrecognized subcommand 'upgrade'")
            || probe
                .stderr
                .contains("Found argument 'upgrade' which wasn't expected"))
    {
        return Ok(());
    }
    if probe.code != Some(0) || !probe.stdout.contains("upgrade") {
        return Err("exact native upgrade support could not be established");
    }
    if !enough_disk(&work, probe_stamp.main.bytes) {
        return Err(
            "native capture snapshot upgrade requires 15 GiB plus a full snapshot rewrite reserve",
        );
    }
    let result = native(server, &snapshot, &work, false, deadline)?;
    if result.code != Some(0) {
        return Err("exact native capture snapshot upgrade failed; private diagnostic retained");
    }
    if before != facts(&snapshot, witness_event_id)? {
        return Err(
            "native capture snapshot upgrade changed recorded event counts, heads or clock witness",
        );
    }
    Ok(())
}

#[derive(PartialEq, Eq)]
struct HelpStamp {
    main: super::visual_inputs::FileStamp,
    wal: Option<super::visual_inputs::FileStamp>,
    schema_version: i64,
    user_version: i64,
}
fn help_stamp(snapshot: &Path) -> Result<HelpStamp, &'static str> {
    let conn = Connection::open_with_flags(snapshot, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "capture snapshot help preflight is unavailable")?;
    conn.busy_timeout(Duration::from_millis(50))
        .map_err(|_| "capture snapshot help preflight is unavailable")?;
    let schema_version = conn
        .pragma_query_value(None, "schema_version", |row| row.get(0))
        .map_err(|_| "capture snapshot schema identity is unavailable")?;
    let user_version = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .map_err(|_| "capture snapshot schema identity is unavailable")?;
    drop(conn);
    let mut wal = snapshot.as_os_str().to_os_string();
    wal.push("-wal");
    Ok(HelpStamp {
        main: super::visual_inputs::file_stamp(snapshot)
            .ok_or("capture snapshot identity is unavailable")?,
        wal: super::visual_inputs::file_stamp(Path::new(&wal)),
        schema_version,
        user_version,
    })
}

fn private_work(work: &Path) -> Result<PathBuf, &'static str> {
    let meta = std::fs::symlink_metadata(work).map_err(|_| "capture work folder is missing")?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return Err("capture work folder is invalid");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if meta.permissions().mode() & 0o077 != 0 {
            return Err("capture work folder must be private");
        }
    }
    work.canonicalize()
        .map_err(|_| "capture work folder is unavailable")
}

fn facts(path: &Path, witness_id: Option<&str>) -> Result<Facts, &'static str> {
    let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|_| "capture snapshot facts could not be read")?;
    conn.busy_timeout(Duration::from_millis(50))
        .map_err(|_| "capture snapshot facts could not be read")?;
    let exists = |name: &str| -> Result<bool, &'static str> {
        conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [name],
            |row| row.get(0),
        )
        .map_err(|_| "capture snapshot table census failed")
    };
    let event_count = if exists("events")? {
        Some(
            conn.query_row("SELECT COUNT(*) FROM events", [], |row| row.get(0))
                .map_err(|_| "capture snapshot event census failed")?,
        )
    } else {
        None
    };
    let heads = if exists("ledger_meta")? {
        let mut statement = conn
            .prepare("SELECT project_id,head_hash,event_count FROM ledger_meta ORDER BY project_id")
            .map_err(|_| "capture snapshot head census failed")?;
        let rows = statement
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|_| "capture snapshot head census failed")?;
        Some(
            rows.collect::<Result<Vec<_>, _>>()
                .map_err(|_| "capture snapshot head census failed")?,
        )
    } else {
        None
    };
    let witness = if let Some(id) = witness_id {
        let witness = conn
            .query_row(
                "SELECT ts,seq,raw_line,raw_sha256,chain_hash FROM events WHERE id=?1",
                [id],
                |row| {
                    Ok(ClockFacts {
                        timestamp: row.get(0)?,
                        sequence: row.get(1)?,
                        raw_line: row.get(2)?,
                        raw_sha256: row.get(3)?,
                        chain_hash: row.get(4)?,
                    })
                },
            )
            .optional()
            .map_err(|_| "capture snapshot clock witness could not be read")?;
        Some(witness.ok_or("capture snapshot clock witness is missing")?)
    } else {
        None
    };
    Ok(Facts {
        event_count,
        heads,
        witness,
    })
}

struct NativeResult {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}
struct OwnedChild {
    child: Child,
    active: bool,
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.active {
            #[cfg(unix)]
            if let Ok(group) = i32::try_from(self.child.id()) {
                let _ =
                    crate::bounded_child::signal_group(group, crate::bounded_child::kill_group_now);
            }
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

#[cfg(unix)]
fn exited_unreaped(child: &Child) -> Result<bool, &'static str> {
    use rustix::process::{waitid, Pid, WaitId, WaitIdOptions};
    let raw = i32::try_from(child.id()).map_err(|_| "native capture PID is invalid")?;
    let pid = Pid::from_raw(raw).ok_or("native capture PID is invalid")?;
    loop {
        match waitid(
            WaitId::Pid(pid),
            WaitIdOptions::EXITED | WaitIdOptions::NOWAIT | WaitIdOptions::NOHANG,
        ) {
            Ok(status) => return Ok(status.is_some()),
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return Err("native capture preparation failed"),
        }
    }
}

fn native(
    server: &Path,
    snapshot: &Path,
    work: &Path,
    help: bool,
    deadline: Instant,
) -> Result<NativeResult, &'static str> {
    if Instant::now() >= deadline {
        return Err("native capture snapshot preparation exceeded its deadline");
    }
    let mut stdout =
        tempfile::tempfile().map_err(|_| "native capture diagnostic is unavailable")?;
    let mut stderr =
        tempfile::tempfile().map_err(|_| "native capture diagnostic is unavailable")?;
    let home = work.join("home");
    std::fs::create_dir_all(&home).map_err(|_| "native capture home is unavailable")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o700))
            .map_err(|_| "native capture home is unavailable")?;
    }
    let mut command = Command::new(server);
    command.arg("--db").arg(snapshot).arg("upgrade");
    if help {
        command.arg("--help");
    }
    command
        .current_dir(work)
        .env("HOME", &home)
        .env("THRULINE_HOME", &home)
        .stdin(Stdio::null())
        .stdout(Stdio::from(
            stdout
                .try_clone()
                .map_err(|_| "native capture diagnostic is unavailable")?,
        ))
        .stderr(Stdio::from(
            stderr
                .try_clone()
                .map_err(|_| "native capture diagnostic is unavailable")?,
        ));
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = OwnedChild {
        child: command
            .spawn()
            .map_err(|_| "exact native CLI is unavailable")?,
        active: true,
    };
    let stem = if help {
        "native-upgrade-help"
    } else {
        "native-upgrade"
    };
    let code = loop {
        if stdout
            .metadata()
            .map_err(|_| "native capture diagnostic is unavailable")?
            .len()
            > OUTPUT_LIMIT
            || stderr
                .metadata()
                .map_err(|_| "native capture diagnostic is unavailable")?
                .len()
                > OUTPUT_LIMIT
        {
            save_log(&mut stderr, &work.join(format!("{stem}.stderr.log")))?;
            return Err("native capture diagnostic exceeded its output bound");
        }
        #[cfg(unix)]
        if exited_unreaped(&child.child)? {
            // Hold the exited leader's PID until its group has been stopped,
            // so a normally exited CLI cannot leave snapshot writers alive.
            if let Ok(group) = i32::try_from(child.child.id()) {
                let _ =
                    crate::bounded_child::signal_group(group, crate::bounded_child::kill_group_now);
            }
            let status = child
                .child
                .wait()
                .map_err(|_| "native capture preparation failed")?;
            child.active = false;
            break status.code();
        }
        #[cfg(not(unix))]
        if let Some(status) = child
            .child
            .try_wait()
            .map_err(|_| "native capture preparation failed")?
        {
            child.active = false;
            break status.code();
        }
        if Instant::now() >= deadline {
            save_log(&mut stderr, &work.join(format!("{stem}.stderr.log")))?;
            return Err("native capture snapshot preparation exceeded its deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout_body = save_log(&mut stdout, &work.join(format!("{stem}.stdout.log")))?;
    let stderr_body = save_log(&mut stderr, &work.join(format!("{stem}.stderr.log")))?;
    if stdout
        .metadata()
        .map_err(|_| "native capture diagnostic is unavailable")?
        .len()
        > OUTPUT_LIMIT
        || stderr
            .metadata()
            .map_err(|_| "native capture diagnostic is unavailable")?
            .len()
            > OUTPUT_LIMIT
    {
        return Err("native capture diagnostic exceeded its output bound");
    }
    Ok(NativeResult {
        code,
        stdout: stdout_body,
        stderr: stderr_body,
    })
}

fn save_log(input: &mut File, path: &Path) -> Result<String, &'static str> {
    input
        .seek(SeekFrom::Start(0))
        .map_err(|_| "native capture diagnostic is unavailable")?;
    let mut bytes = Vec::new();
    input
        .take(LOG_LIMIT)
        .read_to_end(&mut bytes)
        .map_err(|_| "native capture diagnostic is unavailable")?;
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .and_then(|mut file| file.write_all(&bytes))
        .map_err(|_| "native capture diagnostic could not be retained")?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn enough_disk(work: &Path, snapshot_len: u64) -> bool {
    let mut command = Command::new("df");
    command.args(["-Pk"]).arg(work);
    let Ok(crate::bounded_child::Outcome::Finished(output)) =
        crate::bounded_child::output_until(&mut command, Instant::now() + Duration::from_secs(2))
    else {
        return false;
    };
    output.status.success()
        && String::from_utf8_lossy(&output.stdout)
            .lines()
            .nth(1)
            .and_then(|line| line.split_whitespace().nth(3))
            .and_then(|value| value.parse::<u64>().ok())
            .and_then(|kib| kib.checked_mul(1024))
            .is_some_and(|free_bytes| {
                upgrade_space::native_upgrade_has_space(free_bytes, snapshot_len)
            })
}
