//! HQ Next — `GET /api/changes/<id>/pr`: what the pull-request preview
//! (PR-Face, C30 R-PR) needs from this machine that no other route serves.
//!
//! It answers three questions about the folder ThruLine recorded for one
//! tracked Change (`change_decision::worktree_root`, the folder the decision
//! file and the code-moved line use), each from one source:
//!
//!   - **The branch** — `git symbolic-ref --quiet --short HEAD` and
//!     `git rev-parse --verify --quiet HEAD^{commit}` in that folder, both
//!     inside one 5 s deadline through `bounded_child::output_until`, with
//!     system and user git config refused. Read-only verbs only: nothing here
//!     stages, writes a tree or touches the index.
//!   - **The receipts** — the regular `*.json` files directly in
//!     `<folder>/.thruline/receipts/`, counted, and the newest by modified
//!     time: the file the ThruLine Action verifies when it is given none
//!     (`packaging/action/action.yml`, "Verify the receipt": `ls -t
//!     .thruline/receipts/*.json | head -1`). Nothing is opened, parsed or
//!     verified on the passive read. An explicit `?check=1` uses the same CLI
//!     with an isolated temporary object directory and index. It requires the
//!     launch token; no object or index is written into the repository.
//!   - **The check run's name and the badge's words** — the CLI's own
//!     constants (`thruline_attest::report::CHECK_NAME`, `badge_label`), so
//!     the page cannot draw a name or a word the generator does not produce.
//!
//! A fact that could not be read is `null` with a reason beside it, never a
//! guess (C30 G3). The route writes nothing, appends nothing and adds no DDL.
//! Every string read from the folder is masked by `redact::mask_recorded`, and
//! the merged record is served as `masking`.

use super::*;

use std::borrow::Cow;
use std::path::PathBuf;
use std::process::Command;
use std::time::Instant;

use thruline_attest::report::{badge_label, Conclusion, CHECK_NAME};
use thruline_core::ledger::Ledger;
use thruline_core::redact::{mask_recorded, MaskRecord};

/// The shape of this document. Raised when a field changes meaning.
pub(super) const SCHEMA_VERSION: u32 = 1;

/// How long the git reads may take, together.
const GIT_DEADLINE: Duration = Duration::from_secs(5);
const CHECK_DEADLINE: Duration = Duration::from_secs(25);

/// Where the Action looks for a receipt, relative to the repository.
const RECEIPTS_DIR: &str = ".thruline/receipts";

const PREFIX: &str = "/api/changes/";
const SUFFIX: &str = "/pr";

pub(super) fn owns(path: &str) -> bool {
    path.strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .is_some_and(|id| !id.is_empty() && !id.contains('/'))
}

fn json<T: Serialize>(stream: &mut TcpStream, status: u16, value: &T) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        &serde_json::to_vec(value)?,
    )
}

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    json(stream, status, &serde_json::json!({ "error": error }))
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    path: &str,
    query: &str,
    headers: &RequestHeaders,
    stream: &mut TcpStream,
) -> Result<()> {
    if method != "GET" {
        return refuse(stream, 405, "method_not_allowed");
    }
    let checking = query == "check=1";
    if !query.is_empty() && !checking {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    if checking {
        // A browser cannot send this token from another origin. A same-origin
        // GET may omit Origin, so its Fetch Metadata and token are both read.
        if headers.origin_count > 0 && !super::run_route::origin_is_ours(state, headers) {
            return refuse(stream, 403, "same_origin_required");
        }
        if headers
            .sec_fetch_site
            .as_deref()
            .is_some_and(|s| s != "same-origin" && s != "none")
        {
            return refuse(stream, 403, "same_origin_required");
        }
        if !super::run_route::token_is_ours(state, headers) {
            return refuse(stream, 403, "run_token_required");
        }
    }
    let id = path
        .strip_prefix(PREFIX)
        .and_then(|rest| rest.strip_suffix(SUFFIX))
        .and_then(percent_decode)
        .filter(|id| !id.is_empty() && !id.contains('/'));
    let Some(id) = id else {
        return refuse(stream, 400, "malformed_request");
    };
    let ledger = match Ledger::open(&state.db) {
        Ok(l) => l,
        Err(thruline_core::error::CoreError::StoreHeld { .. }) => {
            return super::store_held(state, stream)
        }
        Err(problem) => {
            return json(
                stream,
                503,
                &serde_json::json!({"error": "store_unavailable", "detail": problem.to_string()}),
            )
        }
    };
    let project_id: std::result::Result<String, _> = ledger.conn().query_row(
        "SELECT project_id FROM changes WHERE id=?1",
        [id.as_str()],
        |row| row.get(0),
    );
    let project_id = match project_id {
        Ok(project_id) => project_id,
        Err(rusqlite::Error::QueryReturnedNoRows) => return refuse(stream, 404, "no_such_change"),
        Err(e) => {
            return json(
                stream,
                500,
                &serde_json::json!({"error": "read_failed", "detail": e.to_string()}),
            )
        }
    };
    let policy = match super::change_exclusion_policy(&ledger, &project_id) {
        Ok(policy) => policy,
        Err(_) => return refuse(stream, 503, "exclusion_policy_unavailable"),
    };
    let root = match thruline_core::change_decision::worktree_root(&ledger, &id) {
        Ok(root) => root,
        Err(e) => {
            return json(
                stream,
                500,
                &serde_json::json!({"error": "read_failed", "detail": e.to_string()}),
            )
        }
    };
    let mut mask = Masker::default();
    let folder = folder(root.as_deref(), &mut mask);
    let (git, receipts) = match root.as_deref().filter(|r| r.is_dir()) {
        Some(root) => (git(root, &mut mask), receipts(root, &mut mask)),
        None => {
            let why = folder.reason.clone();
            (
                Git::unknown(why.clone()),
                Receipts::unknown(why.unwrap_or_default()),
            )
        }
    };
    let checked = if checking {
        let Some(root) = root.as_deref().filter(|r| r.is_dir()) else {
            return refuse(stream, 409, "folder_unavailable");
        };
        let Some(file) = receipts.newest_file.as_deref() else {
            return refuse(stream, 409, "no_receipt_to_check");
        };
        if policy
            .key_for_path(
                &format!(
                    "{RECEIPTS_DIR}/{}",
                    file.file_name().unwrap_or_default().to_string_lossy()
                ),
                false,
            )
            .is_some()
        {
            return refuse(stream, 403, "receipt_excluded");
        }
        let Some(fingerprint) = receipts.newest_fingerprint.as_deref() else {
            return refuse(stream, 409, "receipt_metadata_unavailable");
        };
        let Some(head) = git.head.as_deref() else {
            return refuse(stream, 409, "head_unavailable");
        };
        match check_receipt(state, root, file, head, fingerprint, &mut mask) {
            Ok(value) => Some(value),
            Err(_) => return refuse(stream, 503, "receipt_check_unavailable"),
        }
    } else {
        None
    };
    let doc = PrPreview {
        schema_version: SCHEMA_VERSION,
        read_at: thruline_core::ingest::iso8601_utc_now(),
        change_id: id,
        folder,
        git,
        receipts,
        check_run: CheckRun { name: CHECK_NAME },
        badge: Badge {
            states: [
                Conclusion::Success,
                Conclusion::Neutral,
                Conclusion::Failure,
            ]
            .into_iter()
            .map(|c| {
                let (label, colour) = badge_label(c);
                BadgeState {
                    conclusion: c.as_str(),
                    label,
                    colour,
                }
            })
            .collect(),
        },
        checked,
        masking: mask.record,
    };
    let raw = serde_json::to_vec(&doc)?;
    match present_preview(&raw, &policy) {
        Ok(served) => respond(stream, 200, "application/json; charset=utf-8", &served),
        Err(_) => refuse(stream, 503, "exclusion_policy_unavailable"),
    }
}

/// Receipt names are served separately from their directory. Join them only
/// for matching, then remove the temporary path so the contract stays intact.
fn present_preview<'a>(
    raw: &'a [u8],
    policy: &thruline_core::capture_exclusion::ExclusionPolicy,
) -> Result<Cow<'a, [u8]>> {
    if policy.is_empty() {
        return Ok(Cow::Borrowed(raw));
    }
    let mut doc: serde_json::Value = serde_json::from_slice(raw)?;
    if let Some(receipts) = doc["receipts"].as_object_mut() {
        if let Some(dir) = receipts.get("dir").and_then(serde_json::Value::as_str) {
            let path = match receipts.get("newest").and_then(serde_json::Value::as_str) {
                Some(name) => format!("{dir}/{name}"),
                None => dir.to_owned(),
            };
            receipts.insert("path".to_owned(), serde_json::Value::String(path));
        }
    }
    let left_out = crate::exclusion_serve::checked_apply(&mut doc, policy)?;
    if let Some(receipts) = doc["receipts"].as_object_mut() {
        receipts.remove("path");
        if ["dir", "newest"].into_iter().any(|field| {
            receipts
                .get(field)
                .and_then(serde_json::Value::as_str)
                .is_some_and(|s| s.starts_with("left out by your exclusion"))
        }) {
            receipts.insert("newest_fingerprint".to_owned(), serde_json::Value::Null);
        }
    }
    if left_out == 0 {
        Ok(Cow::Borrowed(raw))
    } else {
        Ok(Cow::Owned(serde_json::to_vec(&doc)?))
    }
}

#[cfg(test)]
mod exclusion_preview_tests {
    use super::*;
    use thruline_core::capture_hold::ExclusionView;

    fn policy_for(key: &str) -> thruline_core::capture_exclusion::ExclusionPolicy {
        thruline_core::capture_exclusion::ExclusionPolicy::from_exclusions(
            "/synthetic/project",
            &[ExclusionView {
                key: key.into(),
                covers: key.into(),
                note: None,
                built_in: false,
                on: true,
                source: "recorded",
                basis: None,
                since: None,
                by: None,
            }],
        )
        .expect("synthetic policy")
    }

    #[test]
    fn a_receipt_exclusion_hides_a_split_directory_and_name_without_changing_counts() {
        let raw = br#"{"schema_version":1,"receipts":{"dir":".thruline/receipts","newest":"private.json","count":2},"git":{"branch":"main"}}"#;
        assert!(
            matches!(present_preview(raw, &policy_for("other")), Ok(Cow::Borrowed(bytes)) if bytes == raw)
        );
        let served = present_preview(raw, &policy_for(".thruline/receipts/private.json"))
            .expect("excluded preview");
        let doc: serde_json::Value = serde_json::from_slice(&served).expect("served JSON");
        let mark = thruline_core::capture_exclusion::marker(".thruline/receipts/private.json");
        assert_eq!(doc["receipts"]["dir"], mark);
        assert_eq!(doc["receipts"]["newest"], mark);
        assert_eq!(doc["receipts"]["count"], 2);
        assert_eq!(doc["left_out_by_exclusion"], 1);
        assert!(!String::from_utf8_lossy(&served).contains("private.json\""));
    }
}

/// Masks every served string read from the folder, and keeps the merged record.
#[derive(Default)]
struct Masker {
    record: MaskRecord,
}

impl Masker {
    fn text(&mut self, raw: &str) -> String {
        let (out, rec) = mask_recorded(raw);
        self.record.merge(&rec);
        out
    }
}

#[derive(Serialize)]
struct PrPreview {
    schema_version: u32,
    /// When this was read, UTC.
    read_at: String,
    change_id: String,
    folder: Folder,
    git: Git,
    receipts: Receipts,
    check_run: CheckRun,
    badge: Badge,
    #[serde(skip_serializing_if = "Option::is_none")]
    checked: Option<Checked>,
    masking: MaskRecord,
}

#[derive(Serialize)]
struct Checked {
    receipt: String,
    head: String,
    fingerprint: String,
    check_run: serde_json::Value,
    badge: CheckedBadge,
}

#[derive(Serialize)]
struct CheckedBadge {
    aria_label: String,
    svg: String,
}

/// The same verifier the Action runs, but with every temporary Git object and
/// index redirected outside the recorded repository. The directory is removed
/// only after both bounded child processes have exited (or been stopped).
fn check_receipt(
    state: &State,
    root: &Path,
    file: &Path,
    head: &str,
    fingerprint: &str,
    mask: &mut Masker,
) -> anyhow::Result<Checked> {
    anyhow::ensure!(
        std::fs::symlink_metadata(file)?.file_type().is_file(),
        "receipt is no longer a regular file"
    );
    let root = root.canonicalize()?;
    let receipt_dir = root.join(RECEIPTS_DIR).canonicalize()?;
    anyhow::ensure!(
        receipt_dir.starts_with(&root),
        "receipt directory left the project"
    );
    anyhow::ensure!(
        receipt_dir == root.join(RECEIPTS_DIR),
        "receipt directory may not be reached through a symlink"
    );
    let receipt = file.canonicalize()?;
    anyhow::ensure!(
        receipt.starts_with(&receipt_dir),
        "receipt left its directory"
    );
    anyhow::ensure!(
        std::fs::metadata(&receipt)?.len() <= 16 * 1024 * 1024,
        "receipt exceeds 16 MiB"
    );
    anyhow::ensure!(
        receipt_fingerprint(&receipt).as_deref() == Some(fingerprint),
        "receipt changed before checking"
    );
    // `snapshot_tree` stages into a private index, but repository clean
    // filters (notably LFS) and fsmonitor hooks can run arbitrary helpers and
    // write back into the project. Refuse them before starting the verifier.
    ensure_no_active_git_helpers(&root)?;

    let objects = match ask(
        &root,
        &["rev-parse", "--git-path", "objects"],
        Instant::now() + GIT_DEADLINE,
    ) {
        Answer::Out(path) if !path.is_empty() => {
            let path = PathBuf::from(path);
            (if path.is_absolute() {
                path
            } else {
                root.join(path)
            })
            .canonicalize()?
        }
        _ => anyhow::bail!("git object directory unavailable"),
    };
    anyhow::ensure!(objects.is_dir(), "git object directory unavailable");
    let temp_base = [std::env::temp_dir(), PathBuf::from("/tmp")]
        .into_iter()
        .filter_map(|candidate| candidate.canonicalize().ok())
        .find(|candidate| !candidate.starts_with(&root))
        .ok_or_else(|| anyhow::anyhow!("no temporary directory outside the repository"))?;
    let scratch = tempfile::Builder::new()
        .prefix("thruline-pr-check-")
        .tempdir_in(temp_base)?;
    let isolated_objects = scratch.path().join("objects");
    std::fs::create_dir(&isolated_objects)?;
    // `--db` is the HQ ledger; the receipt verifier reads the separate
    // execution store, including a UI's selected verification project.
    let exec_db = super::projection_exec_db(state)?.canonicalize()?;
    anyhow::ensure!(
        !exec_db.starts_with(&root),
        "execution store is inside the checked project"
    );
    let executable = std::env::current_exe()?;
    let deadline = Instant::now() + CHECK_DEADLINE;
    let run = |format: &str| -> anyhow::Result<String> {
        let mut command = Command::new(&executable);
        clear_git_trace_env(&mut command);
        command
            .arg("verify-receipt")
            .arg(&receipt)
            .args([
                "--offline",
                "--format",
                format,
                "--head-sha",
                head,
                "--repo",
            ])
            .arg(&root)
            .arg("--exec-db")
            .arg(&exec_db)
            .current_dir(&root)
            .env("GIT_OBJECT_DIRECTORY", &isolated_objects)
            .env("GIT_ALTERNATE_OBJECT_DIRECTORIES", &objects)
            .env("TMPDIR", scratch.path())
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_CONFIG_COUNT")
            .env_remove("GIT_CONFIG_PARAMETERS")
            .env_remove("GITHUB_SHA");
        match thruline_core::bounded_child::output_until(&mut command, deadline)? {
            thruline_core::bounded_child::Outcome::TimedOut => {
                anyhow::bail!("receipt check timed out")
            }
            thruline_core::bounded_child::Outcome::Finished(out)
                if matches!(out.status.code(), Some(0..=2)) =>
            {
                Ok(String::from_utf8(out.stdout)?)
            }
            thruline_core::bounded_child::Outcome::Finished(_) => {
                anyhow::bail!("receipt verifier did not return a conclusion")
            }
        }
    };
    let check_run: serde_json::Value = serde_json::from_str(&run("check-run")?)?;
    let title = check_run["output"]["title"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("check run has no title"))?;
    let conclusion = check_run["conclusion"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("check run has no conclusion"))?;
    anyhow::ensure!(
        check_run["head_sha"].as_str() == Some(head),
        "check run moved to another head"
    );
    let badge_svg = run("badge")?;
    anyhow::ensure!(badge_svg.len() <= 4096, "badge is too large");
    let aria_label = badge_svg
        .split_once("aria-label=\"")
        .and_then(|(_, tail)| tail.split_once('"'))
        .map(|(label, _)| label)
        .ok_or_else(|| anyhow::anyhow!("badge has no label"))?;
    anyhow::ensure!(
        badge_matches(conclusion, title, aria_label),
        "badge disagrees with check run"
    );
    anyhow::ensure!(
        receipt_fingerprint(&receipt).as_deref() == Some(fingerprint),
        "receipt changed while checking"
    );
    match ask(
        &root,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        deadline,
    ) {
        Answer::Out(now) if now == head => {}
        _ => anyhow::bail!("head changed while checking"),
    }
    let shown = serde_json::json!({
        "name": check_run["name"],
        "head_sha": head,
        "status": check_run["status"],
        "conclusion": conclusion,
        "output": { "title": mask.text(title) },
    });
    Ok(Checked {
        receipt: mask.text(&receipt.file_name().unwrap_or_default().to_string_lossy()),
        head: head.to_string(),
        fingerprint: fingerprint.to_string(),
        check_run: shown,
        badge: CheckedBadge {
            aria_label: aria_label.to_string(),
            svg: badge_svg,
        },
    })
}

fn badge_matches(conclusion: &str, title: &str, aria_label: &str) -> bool {
    let legacy = match conclusion {
        "success" => Some(Conclusion::Success),
        "neutral" => Some(Conclusion::Neutral),
        "failure" => Some(Conclusion::Failure),
        _ => None,
    };
    if let Some(c) = legacy {
        if aria_label == format!("thruline: {}", badge_label(c).0) {
            return true;
        }
    }
    // Session-v2's CLI badge vocabulary is determined by the same fixed
    // cause title the check-run emits (session_v2::presentation).
    let v2 = match conclusion {
        "success" => Some("ThruLine: Verified"),
        "failure" | "action_required" => Some("ThruLine: Not proven"),
        "neutral" if title.starts_with("Fog:") => Some("ThruLine: Fog"),
        "neutral" => Some("ThruLine: Recorded"),
        _ => None,
    };
    v2 == Some(aria_label)
}

fn ensure_no_active_git_helpers(root: &Path) -> anyhow::Result<()> {
    let mut command = Command::new("git");
    clear_git_trace_env(&mut command);
    thruline_core::git_isolation::isolate_config(&mut command);
    command
        .current_dir(root)
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_PARAMETERS")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .args([
            "config",
            "--get-regexp",
            r"^(filter\..*\.(clean|process)|core\.fsmonitor)$",
        ]);
    match thruline_core::bounded_child::output_until(&mut command, Instant::now() + GIT_DEADLINE)? {
        thruline_core::bounded_child::Outcome::TimedOut => {
            anyhow::bail!("Git helper preflight timed out")
        }
        thruline_core::bounded_child::Outcome::Finished(out) if out.status.code() == Some(1) => {
            Ok(())
        }
        thruline_core::bounded_child::Outcome::Finished(out) if out.status.success() => {
            let found = String::from_utf8(out.stdout)?;
            if found
                .lines()
                .all(|line| line.trim() == "core.fsmonitor false")
            {
                Ok(())
            } else {
                anyhow::bail!("repository has a clean filter or fsmonitor helper")
            }
        }
        thruline_core::bounded_child::Outcome::Finished(_) => {
            anyhow::bail!("Git helper preflight failed")
        }
    }
}

/* ──────────────────────────────── The folder ──────────────────────────────── */

#[derive(Serialize)]
struct Folder {
    /// ThruLine recorded a folder for this Change.
    recorded: bool,
    /// That folder is a directory on this machine now. `null` when none is recorded.
    present: Option<bool>,
    /// The folder's last part, never its full path (a page must not show a
    /// home folder). `null` when none is recorded.
    name: Option<String>,
    reason: Option<String>,
}

fn folder(root: Option<&Path>, mask: &mut Masker) -> Folder {
    match root {
        None => Folder {
            recorded: false,
            present: None,
            name: None,
            reason: Some("ThruLine has no folder recorded for this Change".to_string()),
        },
        Some(root) => {
            let present = root.is_dir();
            Folder {
                recorded: true,
                present: Some(present),
                name: root.file_name().map(|n| mask.text(&n.to_string_lossy())),
                reason: (!present).then(|| {
                    "the folder recorded for this Change is not on this machine".to_string()
                }),
            }
        }
    }
}

/* ──────────────────────────────── The branch ──────────────────────────────── */

#[derive(Serialize)]
struct Git {
    /// The branch HEAD names, short (`main`). `null` when HEAD is detached
    /// (see `detached`) or git could not answer (see `reason`).
    branch: Option<String>,
    /// The commit HEAD is at, full. `null` on an unborn branch or when git
    /// could not answer.
    head: Option<String>,
    /// `true` when HEAD names a commit and no branch; `null` when git could
    /// not answer.
    detached: Option<bool>,
    reason: Option<String>,
}

impl Git {
    fn unknown(reason: Option<String>) -> Git {
        Git {
            branch: None,
            head: None,
            detached: None,
            reason,
        }
    }
}

enum Answer {
    Out(String),
    /// Git ran and said no (a non-zero exit): its first line of stderr.
    No(String),
    /// Git could not be asked, or did not answer in time.
    Unknown(String),
}

fn ask(root: &Path, args: &[&str], deadline: Instant) -> Answer {
    let mut command = Command::new("git");
    clear_git_trace_env(&mut command);
    thruline_core::git_isolation::isolate_config(&mut command);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("LC_ALL", "C")
        .env("LANG", "C")
        // About the recorded folder, not any repository this server's own
        // environment points git at.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_ALTERNATE_OBJECT_DIRECTORIES")
        .arg("-C")
        .arg(root)
        .args(args);
    match thruline_core::bounded_child::output_until(&mut command, deadline) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            Answer::Unknown("git is not installed on this machine".to_string())
        }
        Err(e) => Answer::Unknown(format!("git could not be started: {e}")),
        Ok(thruline_core::bounded_child::Outcome::TimedOut) => Answer::Unknown(format!(
            "git did not answer within {} s",
            GIT_DEADLINE.as_secs()
        )),
        Ok(thruline_core::bounded_child::Outcome::Finished(out)) => {
            if out.status.success() {
                Answer::Out(String::from_utf8_lossy(&out.stdout).trim().to_string())
            } else {
                let stderr = String::from_utf8_lossy(&out.stderr);
                Answer::No(stderr.lines().next().unwrap_or("").trim().to_string())
            }
        }
    }
}

/// An inherited Git trace destination may name a file in the repository.
/// Keep both the passive reads and the explicit verifier from opening it.
fn clear_git_trace_env(command: &mut Command) {
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_TRACE") {
            command.env_remove(key);
        }
    }
}

fn git(root: &Path, mask: &mut Masker) -> Git {
    let deadline = Instant::now() + GIT_DEADLINE;
    let head = match ask(
        root,
        &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"],
        deadline,
    ) {
        Answer::Out(h) if !h.is_empty() && h.bytes().all(|b| b.is_ascii_hexdigit()) => Some(h),
        Answer::Out(_) => None,
        Answer::No(why) if why.contains("not a git repository") => {
            return Git::unknown(Some("the folder is not a git repository".to_string()))
        }
        // `--quiet` makes an unborn HEAD a silent non-zero exit.
        Answer::No(_) => None,
        Answer::Unknown(why) => return Git::unknown(Some(mask.text(&why))),
    };
    match ask(
        root,
        &["symbolic-ref", "--quiet", "--short", "HEAD"],
        deadline,
    ) {
        Answer::Out(branch) if !branch.is_empty() => Git {
            branch: Some(mask.text(&branch)),
            head,
            detached: Some(false),
            reason: None,
        },
        // Exit 1 with nothing said: HEAD names no branch.
        Answer::Out(_) | Answer::No(_) if head.is_some() => Git {
            branch: None,
            head,
            detached: Some(true),
            reason: None,
        },
        Answer::Out(_) | Answer::No(_) => Git::unknown(Some(
            "git named no branch and no commit for HEAD".to_string(),
        )),
        Answer::Unknown(why) => Git {
            branch: None,
            head,
            detached: None,
            reason: Some(mask.text(&why)),
        },
    }
}

/* ─────────────────────────────── The receipts ─────────────────────────────── */

#[derive(Serialize)]
struct Receipts {
    /// Where the Action looks, relative to the folder.
    dir: &'static str,
    /// Regular `*.json` files directly in `dir`; `0` when it does not exist.
    /// `null` when the folder or `dir` could not be read (see `reason`).
    count: Option<u64>,
    /// The newest of them by modified time (ties: the greater name), as a
    /// file name inside `dir`. `null` when there is none.
    newest: Option<String>,
    /// Metadata only, so a displayed result can be kept across passive reads
    /// while the same file bytes are plausibly still at this path.
    newest_fingerprint: Option<String>,
    reason: Option<String>,
    /// The actual directory entry, never serialized or joined from a masked name.
    #[serde(skip)]
    newest_file: Option<PathBuf>,
}

impl Receipts {
    fn unknown(reason: String) -> Receipts {
        Receipts {
            dir: RECEIPTS_DIR,
            count: None,
            newest: None,
            newest_fingerprint: None,
            reason: Some(reason),
            newest_file: None,
        }
    }
}

fn receipts(root: &Path, mask: &mut Masker) -> Receipts {
    let dir = root.join(RECEIPTS_DIR);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Receipts {
                dir: RECEIPTS_DIR,
                count: Some(0),
                newest: None,
                newest_fingerprint: None,
                reason: None,
                newest_file: None,
            }
        }
        Err(e) => {
            return Receipts::unknown(mask.text(&format!("{RECEIPTS_DIR} could not be read: {e}")))
        }
    };
    let mut count = 0u64;
    let mut newest: Option<(std::time::SystemTime, String, PathBuf, Option<String>)> = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.ends_with(".json") {
            continue;
        }
        // Not followed: a link is not a regular file here.
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if !kind.is_file() {
            continue;
        }
        count += 1;
        let path = entry.path();
        let fingerprint = receipt_fingerprint(&path);
        let modified = entry
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::UNIX_EPOCH);
        let later = match &newest {
            None => true,
            Some((t, n, _, _)) => modified > *t || (modified == *t && name > *n),
        };
        if later {
            newest = Some((modified, name, path, fingerprint));
        }
    }
    Receipts {
        dir: RECEIPTS_DIR,
        count: Some(count),
        newest: newest.as_ref().map(|(_, n, _, _)| mask.text(n)),
        newest_fingerprint: newest.as_ref().and_then(|(_, _, _, fp)| fp.clone()),
        reason: None,
        newest_file: newest.map(|(_, _, path, _)| path),
    }
}

fn receipt_fingerprint(file: &Path) -> Option<String> {
    let metadata = std::fs::symlink_metadata(file).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    let nanos = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(format!("{nanos}:{}", metadata.len()))
}

/* ─────────────────────── The generator's own name and words ─────────────────────── */

#[derive(Serialize)]
struct CheckRun {
    /// The check-run name the CLI posts (`thruline verify-receipt --format check-run`).
    name: &'static str,
}

#[derive(Serialize)]
struct Badge {
    /// What the CLI's badge says for each conclusion
    /// (`thruline verify-receipt --format badge`), in its own words and colours.
    states: Vec<BadgeState>,
}

#[derive(Serialize)]
struct BadgeState {
    conclusion: &'static str,
    label: &'static str,
    colour: &'static str,
}
