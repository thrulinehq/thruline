//! Opt-in, foreground Codex log observation. Reuses the bounded importer;
//! imported narration never becomes independently verified execution.
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde_json::{json, Value};

const MAX_LOG: u64 = 16 * 1024 * 1024;
const MAX_HEADER: u64 = 1024 * 1024;
type DiscoveryCache = BTreeMap<PathBuf, (u64, Option<SystemTime>, Option<String>)>;
const MAX_ENTRIES: usize = 20_000;
const MAX_SESSIONS: usize = 128;

pub struct Observer {
    state: Mutex<Value>,
    paused: AtomicBool,
    pub active: AtomicBool,
    /// C34 pause gate (5 of 6): the ledger whose `capture_holds` this observer
    /// follows, or `None` when it follows only the in-memory switch.
    record: Option<PathBuf>,
}

/// What the pause record says about the folder this observer watches, read
/// afresh every cycle — so a pause recorded from a terminal, which cannot reach
/// this process's switch, is still followed.
enum Record {
    /// Nothing to follow: no ledger was given, or none exists yet.
    Absent,
    /// The record was read.
    Read(thruline_core::capture_pause::PauseGate),
    /// The record could not be read. The observer reads nothing until it can:
    /// a read of a paused folder cannot be undone, a delayed one can.
    Unreadable(String),
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn digest(bytes: &[u8]) -> String {
    thruline_core::chain::sha256_hex(bytes)
}

impl Observer {
    /// Start the observer. `record` is a ledger path whose pause record it
    /// follows (C34 pause gate 5 of 6): from its first cycle, before any read,
    /// and on every cycle after, whatever act recorded the pause and from
    /// wherever. `None` follows only the in-memory switch.
    pub fn start(
        root: PathBuf,
        repo: PathBuf,
        db: PathBuf,
        exe: PathBuf,
        stop: Arc<AtomicBool>,
        record: Option<PathBuf>,
    ) -> Result<Arc<Self>> {
        let directory = db.with_extension("observation");
        std::fs::create_dir_all(&directory)?;
        let observer = Arc::new(Self {
            state: Mutex::new(
                json!({"enabled":true,"state":"starting","mode":"automatic_codex_log_observation","source_root":root,"project":repo,"events_written":0,"sessions":[],"message":"Starting local log observation.","boundary":"Automatic observation of Codex's own recorded messages. This does not independently prove commands ran or files changed. Partial history and unsupported record types remain explicit."}),
            ),
            paused: AtomicBool::new(false),
            active: AtomicBool::new(true),
            record,
        });
        let owned = Arc::clone(&observer);
        std::thread::spawn(move || {
            if let Err(error) = owned.run(&root, &repo, &db, &exe, &directory, &stop) {
                owned.update("disconnected", &format!("Observation stopped: {error:#}. Restart HQ after resolving the source or storage problem."));
            }
            owned.active.store(false, Ordering::SeqCst);
        });
        Ok(observer)
    }
    pub fn read(&self) -> Value {
        self.state.lock().map(|s|s.clone()).unwrap_or_else(|_|json!({"enabled":true,"state":"disconnected","message":"Observation status could not be read. Restart HQ."}))
    }
    pub fn pause(&self, paused: bool) {
        self.paused.store(paused, Ordering::SeqCst);
        self.update(if paused {"paused"} else {"starting"}, if paused {"Pause requested. A current bounded read may finish. The coding agent continues independently; resume to read the backlog."} else {"Resuming observation and checking recorded backlog."});
    }
    /// Whether the pause record holds the folder this observer watches (or
    /// cannot be read): the in-memory switch must not resume what the record
    /// says is paused.
    pub fn held_by_record(&self) -> bool {
        let repo = self.read()["project"]
            .as_str()
            .map(str::to_string)
            .unwrap_or_default();
        match self.read_record() {
            Record::Absent => false,
            Record::Read(gate) => gate.paused_now(&repo).is_some(),
            Record::Unreadable(_) => true,
        }
    }
    fn read_record(&self) -> Record {
        let Some(path) = &self.record else {
            return Record::Absent;
        };
        if !path.is_file() {
            return Record::Absent;
        }
        match thruline_core::ledger::Ledger::open(path)
            .and_then(|l| thruline_core::capture_pause::PauseGate::read(&l))
        {
            Ok(gate) => Record::Read(gate),
            Err(e) => Record::Unreadable(format!("{e}")),
        }
    }
    fn update(&self, state: &str, message: &str) {
        if let Ok(mut value) = self.state.lock() {
            value["state"] = json!(state);
            value["message"] = json!(message);
            value["checked_at"] = json!(now());
        }
    }
    fn run(
        &self,
        root: &Path,
        repo: &Path,
        db: &Path,
        exe: &Path,
        directory: &Path,
        stop: &AtomicBool,
    ) -> Result<()> {
        let checkpoint = directory.join("sources.json");
        let mut sources: BTreeMap<String, Value> = if checkpoint.exists() {
            serde_json::from_slice(&std::fs::read(&checkpoint)?)
                .context("reading persisted observation cursor; no cursor was discarded")?
        } else {
            BTreeMap::new()
        };
        let mut previous_error = String::new();
        let mut discovery_cache = DiscoveryCache::new();
        while !stop.load(Ordering::SeqCst) {
            // C34 pause gate (5 of 6): the record is read before anything is,
            // on the first cycle and on every one after. A pause in force for
            // the folder watched — or a record that cannot be read — stops this
            // cycle's reads; the in-memory switch below stays as it was.
            let record = self.read_record();
            let held = match &record {
                Record::Absent => None,
                Record::Read(gate) => gate
                    .paused_now(&repo.to_string_lossy())
                    .map(|_| "Paused in ThruLine's record for this project. Nothing is read until it is resumed there. The coding agent continues independently.".to_string()),
                Record::Unreadable(why) => Some(format!("Paused: the pause record could not be read ({why}). Nothing is read until it can be.")),
            };
            if let Some(message) = &held {
                self.update("paused", message);
            } else if !self.paused.load(Ordering::SeqCst) {
                let result = (|| -> Result<()> {
                    let files = discover(root, repo, &mut discovery_cache)?;
                    storage_budget(directory)?;
                    for name in sources.keys() {
                        anyhow::ensure!(files.iter().any(|(p,_)|p.to_string_lossy()==name.as_str()),"Previously observed log is no longer available in the configured source: {name}. Restore it to resume; stored history remains inspectable.");
                    }
                    let mut latest = files.last().map(|(_, id)| id.clone());
                    let mut waiting = 0;
                    let mut started_while_paused = 0u64;
                    for (path, session) in &files {
                        if stop.load(Ordering::SeqCst) || self.paused.load(Ordering::SeqCst) {
                            break;
                        }
                        let key = path.to_string_lossy().to_string();
                        // The session-start rule: a Codex session that started
                        // inside a pause is never read, after the pause as
                        // before. (Paused-now is handled above the loop.)
                        if let Record::Read(gate) = &record {
                            if gate.judge_file(path) != thruline_core::capture_pause::Verdict::Read
                            {
                                started_while_paused += 1;
                                continue;
                            }
                        }
                        let bytes = read_log(path)?;
                        if let Some(before) = sources.get(&key) {
                            let size = before["complete_bytes"]
                                .as_u64()
                                .context("invalid persisted observation length")?
                                as usize;
                            anyhow::ensure!(bytes.len()>=size && digest(&bytes[..size])==before["sha256"].as_str().unwrap_or(""),"Previously observed log changed or shrank: {}. Existing events are preserved; inspect the source before restarting.",path.display());
                            if bytes.len() as u64
                                == before["file_bytes"].as_u64().unwrap_or(u64::MAX)
                            {
                                continue;
                            }
                        }
                        let complete = bytes
                            .iter()
                            .rposition(|b| *b == b'\n')
                            .map(|p| p + 1)
                            .unwrap_or(0);
                        if complete == 0 {
                            waiting += 1;
                            continue;
                        }
                        self.update(
                            "reading",
                            "Reading newly recorded Codex messages; verification remains separate.",
                        );
                        let mut command = std::process::Command::new(exe);
                        command
                            .args(["import-codex", "--json", "--session-file"])
                            .arg(path)
                            .arg("--repo")
                            .arg(repo)
                            .arg("--exec-db")
                            .arg(db)
                            .current_dir(repo);
                        let report = thruline_capture::bounded::run_controlled(
                            command,
                            Duration::from_secs(15),
                            thruline_capture::redaction::Redactor::from_repo(repo)?,
                            &directory.join("outputs"),
                            stop,
                            None,
                        )?;
                        let stdout = report
                            .stdout
                            .as_ref()
                            .and_then(|s| s.captured.stored_path.as_ref());
                        let value: Value = stdout
                            .and_then(|p| std::fs::read(p).ok())
                            .and_then(|b| serde_json::from_slice(&b).ok())
                            .unwrap_or(Value::Null);
                        let record = json!({"at":now(),"source":path,"session":session,"exit":report.status.code(),"cancelled":report.cancelled,"timed_out":report.timed_out,"stdout":stdout,"stderr":report.stderr.as_ref().and_then(|s|s.captured.stored_path.as_ref()),"observation":value});
                        append(directory, &record)?;
                        anyhow::ensure!(!report.cancelled&&!report.timed_out&&report.status.success()&&import_completed(&value)&&report.stdout.as_ref().is_some_and(|s|s.complete),"A bounded observation attempt did not complete. Its output and any captured events are preserved in {}. Inspect the attempt; automatic retry will occur.",directory.display());
                        let mut source = value["source_read"].clone();
                        anyhow::ensure!(
                            source.is_object(),
                            "Importer did not return a source identity"
                        );
                        // Do not advance a cursor if the source changed while the child read it.
                        // Already appended observations remain historical, with this failed attempt.
                        let after = read_log(path)?;
                        if let Some(before) = sources.get(&key) {
                            validate_prefix(&after, before)?;
                        }
                        validate_prefix(&after, &source)?;
                        source["file_bytes"] = json!(
                            source["complete_bytes"].as_u64().unwrap_or(0)
                                + source["trailing_bytes_waiting"].as_u64().unwrap_or(0)
                        );
                        waiting += source["trailing_bytes_waiting"]
                            .as_u64()
                            .unwrap_or(0)
                            .min(1);
                        sources.insert(key, source);
                        let temporary = directory.join("sources.pending.json");
                        std::fs::write(&temporary, serde_json::to_vec_pretty(&sources)?)?;
                        std::fs::rename(temporary, &checkpoint)?;
                        if let Ok(mut status) = self.state.lock() {
                            status["events_written"] = json!(
                                status["events_written"].as_u64().unwrap_or(0)
                                    + value["events_written"].as_u64().unwrap_or(0)
                            );
                            status["last_observed_at"] = json!(now());
                            status["last_attempt"] = record;
                        }
                        latest = Some(session.clone());
                    }
                    if !self.paused.load(Ordering::SeqCst) {
                        self.update("watching",if files.is_empty(){"Watching Codex logs for this project. No matching session has been recorded yet."}else{"Watching Codex logs automatically. New recorded messages appear without manual import."});
                        if let Ok(mut status) = self.state.lock() {
                            status["sessions"] =
                                json!(files.iter().map(|(_, id)| id).collect::<Vec<_>>());
                            status["partial_lines_waiting"] = json!(waiting);
                            if started_while_paused > 0 {
                                status["started_while_paused"] = json!(started_while_paused);
                            }
                            if let Some(id) = latest {
                                status["latest_session"] = json!(id);
                            }
                        }
                    }
                    Ok(())
                })();
                if let Err(error) = result {
                    let text = format!("{error:#}");
                    self.update("disconnected", &text);
                    if text != previous_error {
                        append(
                            directory,
                            &json!({"at":now(),"state":"disconnected","error":text}),
                        )?;
                        previous_error = text;
                    }
                } else {
                    previous_error.clear();
                }
            }
            // Explicit foreground observer: interruptible 2-second cadence.
            for _ in 0..20 {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        self.update("stopped","HQ stopped observing. The coding agent was not stopped. Restart HQ to resume from persisted evidence.");
        append(directory, &json!({"at":now(),"state":"stopped"}))?;
        Ok(())
    }
}
fn validate_prefix(bytes: &[u8], source: &Value) -> Result<()> {
    let size = source["complete_bytes"]
        .as_u64()
        .context("Missing observed source length")? as usize;
    anyhow::ensure!(size<=bytes.len() && digest(&bytes[..size])==source["sha256"].as_str().unwrap_or(""),"Observed source changed during reading. Captured observations are retained, but the cursor was not advanced and observation is incomplete.");
    Ok(())
}
fn storage_budget(directory: &Path) -> Result<()> {
    let mut total = 0u64;
    let mut stack = vec![directory.to_path_buf()];
    let mut count = 0;
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            count += 1;
            anyhow::ensure!(
                count <= 20000,
                "Observation storage reached 20,000 files; archive it before resuming"
            );
            let meta = entry.metadata()?;
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                total = total.saturating_add(meta.len());
            }
        }
    }
    anyhow::ensure!(
        total < 512 * 1024 * 1024,
        "Observation storage reached 512 MiB; archive it before resuming. No history was removed."
    );
    Ok(())
}
fn append(directory: &Path, value: &Value) -> Result<()> {
    let path = directory.join("attempts.jsonl");
    anyhow::ensure!(
        std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0) < 64 * 1024 * 1024,
        "Observation journal reached 64 MiB; preserve/archive it before continuing"
    );
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}
/// A command envelope is not a captured tool outcome. Validate its identity
/// before interpreting the importer's completion flag; arbitrary JSON that
/// happens to contain a true boolean cannot advance the observation cursor.
fn import_completed(value: &Value) -> bool {
    #[derive(serde::Deserialize)]
    struct ImportStatus {
        shape: String,
        schema_version: u64,
        ok: bool,
    }
    serde_json::from_value::<ImportStatus>(value.clone()).is_ok_and(|status| {
        status.shape == crate::json_out::IMPORT_CODEX.id
            && status.schema_version == crate::json_out::IMPORT_CODEX.version
            && status.ok
    })
}

fn read_log(path: &Path) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    anyhow::ensure!(
        meta.is_file() && !meta.file_type().is_symlink(),
        "Log must be a regular file: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_LOG + 1)
        .read_to_end(&mut bytes)?;
    anyhow::ensure!(
        bytes.len() as u64 <= MAX_LOG,
        "Log exceeds 16 MiB automatic observation scope: {}",
        path.display()
    );
    Ok(bytes)
}
fn discover(
    root: &Path,
    repo: &Path,
    cache: &mut DiscoveryCache,
) -> Result<Vec<(PathBuf, String)>> {
    anyhow::ensure!(
        root.is_dir(),
        "Codex log directory is unavailable: {}. Restore it or restart with --observe-codex DIR.",
        root.display()
    );
    let expected = repo.canonicalize()?;
    let mut stack = vec![root.to_path_buf()];
    let mut seen = 0;
    let mut found = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            seen += 1;
            anyhow::ensure!(
                seen <= MAX_ENTRIES,
                "Codex discovery exceeded 20,000 entries; use a narrower log directory"
            );
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                continue;
            }
            if kind.is_dir() {
                stack.push(entry.path());
                continue;
            }
            if entry.path().extension().is_none_or(|e| e != "jsonl") {
                continue;
            }
            let path = entry.path();
            let metadata = entry.metadata()?;
            let stamp = (metadata.len(), metadata.modified().ok());
            let id = if let Some((length, modified, id)) = cache
                .get(&path)
                .filter(|(length, modified, _)| (*length, *modified) == stamp)
            {
                let _ = (length, modified);
                id.clone()
            } else {
                let mut header = String::new();
                BufReader::new(std::fs::File::open(&path)?.take(MAX_HEADER + 1))
                    .read_line(&mut header)?;
                anyhow::ensure!(header.len() as u64<=MAX_HEADER,"Codex session header exceeds 1 MiB: {}. Discovery is incomplete; use a narrower source directory.",path.display());
                let value:Value=serde_json::from_str(&header).with_context(||format!("Codex session header is incomplete or unreadable: {}. Waiting for a complete source header; this is not an empty account.",path.display()))?;
                anyhow::ensure!(
                    value["type"] == "session_meta",
                    "Unidentified Codex source header: {}. Discovery is incomplete.",
                    path.display()
                );
                let cwd = value["payload"]["cwd"]
                    .as_str()
                    .context("Codex session header does not identify a project")?;
                let id = if Path::new(cwd).canonicalize().ok().as_ref() == Some(&expected) {
                    Some(
                        value["payload"]["id"]
                            .as_str()
                            .context("Matching Codex session has no ID")?
                            .to_string(),
                    )
                } else {
                    None
                };
                // Cache unchanged headers, including unrelated projects, so each poll does not
                // parse the user's entire recorded history. Active files are examined again.
                if cache.len() >= MAX_ENTRIES {
                    cache.clear();
                }
                cache.insert(path.clone(), (stamp.0, stamp.1, id.clone()));
                id
            };
            if let Some(id) = id {
                found.push((path, id));
            }
            anyhow::ensure!(
                found.len() <= MAX_SESSIONS,
                "More than 128 matching sessions; use a narrower log directory"
            );
        }
    }
    found.sort();
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_versioned_importer_envelope_can_report_import_completion() {
        let complete = json!({"shape":"thruline.import-codex","schema_version":1,"ok":true});
        assert!(import_completed(&complete));
        for invalid in [
            json!({"ok":true,"ok_basis":"not_stated"}),
            json!({"shape":"thruline.doctor","schema_version":1,"ok":true}),
            json!({"shape":"thruline.import-codex","schema_version":2,"ok":true}),
            json!({"shape":"thruline.import-codex","schema_version":1,"ok":false}),
            json!({"shape":"thruline.import-codex","schema_version":1}),
            json!({"shape":"thruline.import-codex","schema_version":1,"ok":"true"}),
        ] {
            assert!(!import_completed(&invalid), "{invalid}");
        }
    }
    #[test]
    fn changed_or_truncated_source_cannot_advance_cursor() {
        let source = json!({"complete_bytes":4,"sha256":digest(b"old\n")});
        assert!(validate_prefix(b"old\nnew\n", &source).is_ok());
        assert!(validate_prefix(b"new\n", &source).is_err());
        assert!(validate_prefix(b"old", &source).is_err());
    }
    #[test]
    fn discovery_selects_exact_project_and_requires_available_source() {
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        let logs = temp.path().join("logs");
        std::fs::create_dir(&project).unwrap();
        std::fs::create_dir(&logs).unwrap();
        std::fs::write(
            logs.join("ours.jsonl"),
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":"ours","cwd":project,"base_instructions":"large legitimate header".repeat(4000)}})
            ),
        )
        .unwrap();
        std::fs::write(
            logs.join("other.jsonl"),
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":"other","cwd":temp.path()}})
            ),
        )
        .unwrap();
        let mut cache = DiscoveryCache::new();
        let entries = discover(&logs, &project, &mut cache).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1, "ours");
        assert_eq!(discover(&logs, &project, &mut cache).unwrap(), entries);
        std::fs::write(logs.join("partial.jsonl"), b"{\"type\":").unwrap();
        assert!(discover(&logs, &project, &mut cache)
            .unwrap_err()
            .to_string()
            .contains("incomplete"));
        std::fs::write(
            logs.join("partial.jsonl"),
            format!(
                "{}\n",
                json!({"type":"session_meta","payload":{"id":"later","cwd":project}})
            ),
        )
        .unwrap();
        assert_eq!(discover(&logs, &project, &mut cache).unwrap().len(), 2);
        std::fs::write(
            logs.join("oversize.jsonl"),
            vec![b' '; MAX_HEADER as usize + 1],
        )
        .unwrap();
        assert!(discover(&logs, &project, &mut cache)
            .unwrap_err()
            .to_string()
            .contains("exceeds 1 MiB"));
        assert!(discover(&temp.path().join("missing"), &project, &mut cache).is_err());
    }
}
