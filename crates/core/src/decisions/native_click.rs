//! A browser request starts a native review; only AppKit click plus fresh OS presence can authorize it.
use super::*;
use serde::{Deserialize, Serialize};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) const FORMAT: &str = "thruline_native_click_confirmation_v1";
const KIND: &str = "native_review_click";
#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NativeEnvelope {
    pub schema_version: u32,
    pub kind: String,
    pub review_sha256: String,
    #[serde(deserialize_with = "required_supersession")]
    pub supersession: Option<Value>,
}
fn required_supersession<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Value>, D::Error> {
    Option::<Value>::deserialize(deserializer)
}
fn full_sha(s: &str) -> bool {
    s.len() == 64
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(super) fn parse_envelope(bytes: &str) -> Result<Option<NativeEnvelope>> {
    let value: Value = serde_json::from_str(bytes)?;
    if value["kind"].as_str() != Some(KIND) {
        return Ok(None);
    }
    let envelope: NativeEnvelope = serde_json::from_str(bytes)?;
    if envelope.schema_version != 1 || !full_sha(&envelope.review_sha256) {
        return Err(CoreError::Integrity("native envelope is invalid".into()));
    }
    Ok(Some(envelope))
}
pub(super) fn bind_confirmation(
    confirmation: &mut Value,
    review_sha: &str,
    nonce: &str,
) -> Result<()> {
    if !full_sha(review_sha) || nonce.is_empty() {
        return Err(CoreError::Integrity(
            "native review binding is invalid".into(),
        ));
    }
    let object = confirmation
        .as_object_mut()
        .ok_or_else(|| CoreError::Integrity("native action tuple is not an object".into()))?;
    object.insert(
        "thruline_native_review".into(),
        json!({"schema_version":1,"review_sha256":review_sha,"nonce":nonce}),
    );
    Ok(())
}
#[derive(Debug, Serialize)]
pub struct NativeClickAvailability {
    pub schema_version: u32,
    pub available: bool,
    pub reason: Option<String>,
}
pub fn native_click_availability(ledger: &Ledger) -> NativeClickAvailability {
    #[cfg(target_os = "macos")]
    let result = if BUSY.load(Ordering::Acquire) {
        Err(CoreError::Config("another native review is active".into()))
    } else {
        preflight(ledger).map(|_| ())
    };
    #[cfg(not(target_os = "macos"))]
    let result: Result<()> = {
        let _ = ledger;
        Err(CoreError::Config("native review requires macOS".into()))
    };
    NativeClickAvailability {
        schema_version: 1,
        available: result.is_ok(),
        reason: result.err().map(|e| e.to_string()),
    }
}
pub fn confirm_native_click(
    ledger: &mut Ledger,
    action: PersonActKind,
    act_row_id: &str,
    selection: Option<SupersessionInput>,
    expected_review_sha256: &str,
) -> Result<String> {
    #[cfg(target_os = "macos")]
    {
        confirm_macos(
            ledger,
            action,
            act_row_id,
            selection,
            expected_review_sha256,
        )
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (
            ledger,
            action,
            act_row_id,
            selection,
            expected_review_sha256,
        );
        Err(CoreError::Config("native review requires macOS".into()))
    }
}
#[cfg(target_os = "macos")]
static BUSY: AtomicBool = AtomicBool::new(false);
#[cfg(target_os = "macos")]
fn native_agent_context() -> bool {
    if [
        "CODEX_THREAD_ID",
        "CODEX_RUN_ID",
        "CODEX_SANDBOX",
        "CLAUDECODE",
        "CLAUDE_CODE_ENTRYPOINT",
        "OPENAI_AGENT",
        "THRULINE_AGENT_CONTEXT",
        "THRULINE_MODEL_CONTEXT",
        "GITHUB_ACTIONS",
        "CI",
    ]
    .iter()
    .any(|k| std::env::var_os(k).is_some())
    {
        return true;
    }
    let until = Instant::now() + Duration::from_secs(2);
    let mut pid = std::process::id();
    for _ in 0..12 {
        let Some(left) = until.checked_duration_since(Instant::now()) else {
            return true;
        };
        let result = bounded_output(
            Command::new("/bin/ps").args(["-p", &pid.to_string(), "-o", "ppid=", "-o", "comm="]),
            left.min(Duration::from_millis(200)),
        );
        let Ok(output) = result else {
            return true;
        };
        if !output.status.success() {
            return true;
        }
        let row = String::from_utf8_lossy(&output.stdout);
        let mut parts = row.split_whitespace();
        let Some(parent) = parts.next().and_then(|v| v.parse::<u32>().ok()) else {
            return true;
        };
        let command = parts.collect::<Vec<_>>().join(" ").to_ascii_lowercase();
        if ["codex", "claude", "mcp", "agent", "chatgpt"]
            .iter()
            .any(|v| command.contains(v))
        {
            return true;
        }
        if parent == 0 || parent == pid {
            return false;
        }
        pid = parent;
    }
    false
}
#[cfg(target_os = "macos")]
fn preflight(ledger: &Ledger) -> Result<(u32, PathBuf, fs::Metadata)> {
    use std::os::unix::fs::MetadataExt;
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        return Err(CoreError::Config(
            "person confirmation requires an interactive terminal".into(),
        ));
    }
    if native_agent_context() {
        return Err(CoreError::Config(
            "an agent or automated context cannot create a person confirmation".into(),
        ));
    }
    let uid = terminal_account_uid()?;
    let _tty = OpenOptions::new()
        .read(true)
        .open("/dev/tty")
        .map_err(|_| CoreError::Config("person confirmation requires /dev/tty".into()))?;
    let db = database_path(ledger.conn())?.ok_or_else(|| {
        CoreError::Config("native confirmation needs a file-backed ledger".into())
    })?;
    let meta = fs::symlink_metadata(&db).map_err(|e| CoreError::io(&db, e))?;
    if meta.file_type().is_symlink() || !meta.is_file() || meta.uid() != uid {
        return Err(CoreError::Config(
            "ledger and terminal account owners differ or ledger is a symlink".into(),
        ));
    }
    if !Path::new("/usr/bin/swift").is_file() {
        return Err(CoreError::Config(
            "native review runtime is unavailable".into(),
        ));
    }
    Ok((uid, db, meta))
}
#[cfg(target_os = "macos")]
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct NativeResponse {
    schema_version: u32,
    transport: String,
    result: String,
    nonce: String,
    review_sha256: String,
    tuple_sha256: String,
    account_uid: u32,
}
#[cfg(target_os = "macos")]
fn validate_native_result(bytes: &[u8], expected: &Value) -> Result<()> {
    if bytes.len() > 4096
        || serde_json::from_slice::<NativeResponse>(bytes)
            .ok()
            .and_then(|r| serde_json::to_value(r).ok())
            .as_ref()
            != Some(expected)
    {
        return Err(CoreError::Config(
            "native result did not match the exact review challenge".into(),
        ));
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn native_presence(
    review: &Value,
    expected: &Value,
    reason: &str,
    until: Instant,
) -> Result<Vec<u8>> {
    #[cfg(test)]
    if let Some(result) = tests::native_fixture_result(expected) {
        return result;
    }
    use std::os::unix::fs::MetadataExt;
    let mut payload = tempfile::NamedTempFile::new()
        .map_err(|e| CoreError::Config(format!("native review file could not start: {e}")))?;
    let mut result = tempfile::NamedTempFile::new()
        .map_err(|e| CoreError::Config(format!("native result file could not start: {e}")))?;
    let payload_value =
        json!({"schema_version":1,"review":review,"expected":expected,"reason":reason});
    let bytes = serde_json::to_vec(&payload_value)?;
    if bytes.len() > 2 * 1024 * 1024 {
        return Err(CoreError::Config(
            "native review exceeds its display bound".into(),
        ));
    }
    payload
        .write_all(&bytes)
        .and_then(|_| payload.as_file().sync_all())
        .map_err(|e| CoreError::io(payload.path(), e))?;
    let before = result
        .as_file()
        .metadata()
        .map_err(|e| CoreError::io(result.path(), e))?;
    let timeout = until
        .checked_duration_since(Instant::now())
        .ok_or_else(|| CoreError::Config("native review timeout".into()))?;
    let status = bounded_status(
        Command::new("/usr/bin/swift")
            .arg("-e")
            .arg(include_str!("native_review.swift"))
            .env("THRULINE_NATIVE_REVIEW_FILE", payload.path())
            .env("THRULINE_NATIVE_REVIEW_SHA256", sha256_hex(&bytes))
            .env("THRULINE_NATIVE_RESULT_FILE", result.path())
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null()),
        timeout,
    )?;
    if !status.success() {
        return Err(CoreError::Config(
            "native review was cancelled or macOS user presence was not completed".into(),
        ));
    }
    let after = fs::symlink_metadata(result.path()).map_err(|e| CoreError::io(result.path(), e))?;
    if !private_path(
        result.path(),
        expected["account_uid"]
            .as_u64()
            .and_then(|u| u32::try_from(u).ok())
            .ok_or_else(|| CoreError::Integrity("native challenge has no account".into()))?,
        false,
    ) || before.dev() != after.dev()
        || before.ino() != after.ino()
    {
        return Err(CoreError::Config("native result file changed".into()));
    }
    let mut bytes = Vec::new();
    result
        .as_file_mut()
        .take(4097)
        .read_to_end(&mut bytes)
        .map_err(|e| CoreError::io(result.path(), e))?;
    Ok(bytes)
}
#[cfg(target_os = "macos")]
fn confirm_macos(
    ledger: &mut Ledger,
    action: PersonActKind,
    act_row_id: &str,
    selection: Option<SupersessionInput>,
    expected_review_sha256: &str,
) -> Result<String> {
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    if BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Err(CoreError::Config("another native review is active".into()));
    }
    struct BusyGuard;
    impl Drop for BusyGuard {
        fn drop(&mut self) {
            BUSY.store(false, Ordering::Release);
        }
    }
    let _busy = BusyGuard;
    let (uid, db_path, db_meta) = preflight(ledger)?;
    let account = os_account_name()?;
    let target = target(ledger.conn(), action, act_row_id)?;
    let already_recorded: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decision_person_acts WHERE action=?1 AND act_row_id=?2)",
        params![action.as_str(), act_row_id],
        |r| r.get(0),
    )?;
    if already_recorded {
        return Err(CoreError::Config(
            "person action already has a receipt".into(),
        ));
    }
    let review = review_terminal_act(ledger, action, act_row_id, selection.as_ref())?;
    let review_sha = sha256_hex(&serde_json::to_vec(&review)?);
    if !full_sha(expected_review_sha256) || expected_review_sha256 != review_sha {
        return Err(CoreError::Integrity(
            "native review SHA differs from the caller review".into(),
        ));
    }
    let bridge_action = review["action"]
        .as_str()
        .ok_or_else(|| CoreError::Integrity("native review has no action".into()))?;
    let nonce = Ulid::generate().to_string();
    let session_id = format!("decision-native-{}", Ulid::generate());
    let mut confirmation = review["confirmation"].clone();
    bind_confirmation(&mut confirmation, &review_sha, &nonce)?;
    let tuple_sha = sha256_hex(&serde_json::to_vec(&confirmation)?);
    let envelope = NativeEnvelope {
        schema_version: 1,
        kind: KIND.into(),
        review_sha256: review_sha.clone(),
        supersession: review["supersession"]
            .as_object()
            .map(|_| review["supersession"].clone()),
    };
    let supersession_json = Some(serde_json::to_string(&envelope)?);
    let deadline = Instant::now() + Duration::from_secs(120);
    let expected = json!({"schema_version":1,"transport":"native_review_click","result":"confirmed","nonce":nonce,"review_sha256":review_sha,"tuple_sha256":tuple_sha,"account_uid":uid});
    let reason = format!(
        "Confirm ThruLine decision {bridge_action} nonce {nonce} tuple SHA-256 {tuple_sha}"
    );
    let result = native_presence(&review, &expected, &reason, deadline)?;
    validate_native_result(&result, &expected)?;
    if expired(Some(deadline)) {
        return Err(CoreError::Config("native review timeout".into()));
    }
    if native_agent_context() {
        return Err(CoreError::Config(
            "agent context appeared during native confirmation".into(),
        ));
    }
    if review_terminal_act(ledger, action, act_row_id, selection.as_ref())? != review {
        return Err(CoreError::Integrity(
            "reviewed decision changed during native confirmation".into(),
        ));
    }
    let current_db = fs::symlink_metadata(&db_path)
        .map_err(|_| CoreError::Config("ledger changed during native confirmation".into()))?;
    if current_db.file_type().is_symlink()
        || !current_db.is_file()
        || current_db.uid() != uid
        || current_db.uid() != db_meta.uid()
        || current_db.dev() != db_meta.dev()
        || current_db.ino() != db_meta.ino()
    {
        return Err(CoreError::Config(
            "ledger changed during native confirmation".into(),
        ));
    }
    if terminal_account_uid()? != uid {
        return Err(CoreError::Config(
            "terminal account changed during native confirmation".into(),
        ));
    }
    let already_recorded: bool = ledger.conn().query_row(
        "SELECT EXISTS(SELECT 1 FROM decision_person_acts WHERE action=?1 AND act_row_id=?2)",
        params![action.as_str(), act_row_id],
        |r| r.get(0),
    )?;
    if already_recorded || expired(Some(deadline)) {
        return Err(CoreError::Config(
            "native review expired or person action already has a receipt".into(),
        ));
    }
    let dir = db_path
        .parent()
        .unwrap_or(Path::new("/"))
        .join("decision-confirmations");
    crate::private_store::create_private_dir(&dir).map_err(|e| CoreError::io(&dir, e))?;
    if !private_path(&dir, uid, true) {
        return Err(CoreError::Config(
            "confirmation directory is not private to the account".into(),
        ));
    }
    let raw_path = dir.join(format!("{session_id}.jsonl"));
    let mut line = terminal_line(&TerminalLineFields {
        project_id: &target.project_id,
        session_id: &session_id,
        nonce: &nonce,
        action: bridge_action,
        act_row_id,
        account_uid: uid,
        account_name: &account,
        confirmation: &confirmation,
    });
    line["format"] = json!(FORMAT);
    let bytes = serde_json::to_vec(&line)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&raw_path)
        .map_err(|e| CoreError::io(&raw_path, e))?;
    file.write_all(&bytes)
        .and_then(|_| file.write_all(b"\n"))
        .and_then(|_| file.sync_all())
        .map_err(|e| CoreError::io(&raw_path, e))?;
    if !private_path(&raw_path, uid, false) {
        return Err(CoreError::Config(
            "confirmation log is not private to the account".into(),
        ));
    }
    let (line_count, raw_digest) = crate::ingest::whole_file_digest(&raw_path)?;
    if expired(Some(deadline)) {
        return Err(CoreError::Config("decision challenge expired".into()));
    }
    let now = crate::ingest::iso8601_utc_now();
    ledger.conn().execute(
        "INSERT INTO sessions(id,project_id,file_path,started_at,raw_line_count,raw_digest)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            session_id,
            target.project_id,
            raw_path.to_string_lossy(),
            now,
            line_count as i64,
            raw_digest
        ],
    )?;
    let mut event = NormalizedEvent::new(EventKind::UserMsg);
    event.role = Some("user".into());
    event.text = Some(confirmation.to_string());
    event.ts = Some(now.clone());
    event.line_session_id = Some(session_id.clone());
    let raw_sha = sha256_hex(&bytes);
    let placed = ledger.append_session_events(
        &target.project_id,
        &session_id,
        &[(
            RawLineRef {
                file: raw_path.to_string_lossy().into_owned(),
                line: 1,
                sha256: raw_sha.clone(),
            },
            event,
        )],
    )?;
    let event_id = placed
        .first()
        .ok_or_else(|| CoreError::Integrity("native event was not appended".into()))?
        .id
        .clone();
    install_schema_for_write(ledger)?;
    let receipt_id = Ulid::generate().to_string();
    let tx = ledger.conn().unchecked_transaction()?;
    tx.execute(
        "INSERT INTO decision_person_acts(id,action,act_row_id,decision_id,project_id,event_id,
         session_id,raw_file,raw_line,raw_sha256,chain_hash,tuple_sha256)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10,?11)",
        params![
            receipt_id,
            action.as_str(),
            act_row_id,
            target.decision_id,
            target.project_id,
            event_id,
            session_id,
            raw_path.to_string_lossy(),
            raw_sha,
            placed[0].chain_hash,
            tuple_sha
        ],
    )?;
    tx.execute(
        "INSERT INTO decision_bridge_proofs(event_id,receipt_id,project_id,session_id,action,
         act_row_id,nonce,tuple_sha256,account_uid,account_name,raw_sha256,supersession_json,
         confirmation_method,created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,'macos_local_authentication_tty',?13)",
        params![
            event_id,
            receipt_id,
            target.project_id,
            session_id,
            bridge_action,
            act_row_id,
            nonce,
            tuple_sha,
            uid,
            account,
            raw_sha,
            supersession_json,
            now
        ],
    )?;
    tx.commit()?;
    if !verified(ledger, action, act_row_id)? {
        return Err(CoreError::Integrity(
            "new native confirmation did not verify".into(),
        ));
    }
    Ok(receipt_id)
}

#[cfg(all(test, target_os = "macos"))]
#[path = "native_click_tests.rs"]
mod tests;
