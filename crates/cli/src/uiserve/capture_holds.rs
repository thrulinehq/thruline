//! HQ Next — a person's pause and path exclusions, recorded (C30 R-LEDGER).
//!
//!   - `POST /api/hq/projects/<id>/capture/pause` — pause recording for this
//!     project's folder, or resume it (write-address row 26 in CLAUDE.md).
//!   - `POST /api/hq/projects/<id>/capture/exclusions` — leave one path out, or
//!     put it back (row 27).
//!
//! Both are dispatched from `hq::handle`, which has already required the page's
//! own origin. Each appends one row to `capture_holds`, on the `capture_hold`
//! hash chain that `thruline verify` re-folds, through
//! [`thruline_core::capture_hold::record_for_person`] — the function
//! `thruline capture pause|resume|exclude|include` calls too, so a hold put from
//! a terminal and one put from the screen leave rows of the same shape and are
//! refused for the same reasons, in the same words.
//!
//! **A PAUSE AND AN EXCLUSION ARE HONOURED AT DIFFERENT BOUNDARIES** (C34 Q4, Q5, Q6). The six
//! capture callers read a recorded pause through `capture_pause`: the hook
//! delivery, the Stop audit, the HQ load, the first-use register, the Codex
//! observer and `thruline ingest` — each gate in its own caller, none in
//! `ingest.rs`, `sensor::append` or `mirror.rs`, and each a byte-for-byte no-op
//! while the record holds no pause (`crates/cli/tests/capture_gates_noop.rs`).
//! A pause also sets the Codex observer's in-memory paused flag
//! (`Observer::pause`, the call `POST /api/observation` makes), and only for the
//! folder that observer watches. An exclusion is applied at serve and export
//! time. Every answer carries `holds.honoured: true`,
//! `holds.exclusions_honoured: true` and the sentence that says so, so no
//! screen has to guess.
//!
//! The routes are free: they ask no licence, and approving a version stays the
//! one paid act. The name on a row is the account ThruLine is running as. A row
//! is the project's id and root folder; no path is ever taken from the browser
//! except the pattern to leave out, which is checked and never opened.

use super::*;
use thruline_core::capture_hold::{self as holds, Act, Hold, HoldRow, HoldsView, Recorded};
use thruline_core::hq as core_hq;

/// The body cap of the pause route: `{"act":"resumed"}` and nothing else.
pub(super) const PAUSE_BODY_MAX: usize = 512;

/// The body cap of the exclusions route, sized from the one limit it holds — a
/// pattern of at most `MAX_PATTERN_CHARS` characters, every character written
/// the longest way JSON can write it (twelve bytes) — plus room for the act and
/// the field names. A body over this could not hold a pattern within the limit,
/// so it is refused with `413 too_long`.
pub(super) const EXCLUSIONS_BODY_MAX: usize = holds::MAX_PATTERN_CHARS * 12 + 1024;

fn refuse(stream: &mut TcpStream, status: u16, error: &str) -> Result<()> {
    answer(stream, status, &serde_json::json!({ "error": error }))
}

/// The body as `T`, only when it is a JSON object. A struct would also read
/// itself out of an array (`["paused"]`), which is not a request this page
/// makes.
fn parse<T: serde::de::DeserializeOwned>(body: &[u8]) -> Option<T> {
    let value: serde_json::Value = serde_json::from_slice(body).ok()?;
    if !value.is_object() {
        return None;
    }
    serde_json::from_value(value).ok()
}

fn answer(stream: &mut TcpStream, status: u16, body: &serde_json::Value) -> Result<()> {
    respond(
        stream,
        status,
        "application/json; charset=utf-8",
        serde_json::to_vec(body)?.as_slice(),
    )
}

/// `409` with the code a refusal has and the sentence a person reads.
fn conflict(stream: &mut TcpStream, why: holds::Refusal) -> Result<()> {
    answer(
        stream,
        409,
        &serde_json::json!({ "error": why.as_str(), "message": why.sentence() }),
    )
}

/// A served string, masked the way every recorded string is.
fn mask(raw: &str) -> String {
    thruline_core::redact::mask_recorded(raw).0
}

/// `view` with every string a person typed masked: the names and the paths.
/// `mask` is the caller's, so the capture route can merge what it masked into
/// its own record.
pub(super) fn masked(view: HoldsView, mut mask: impl FnMut(&str) -> String) -> HoldsView {
    let mut view = view;
    view.paused_by = view.paused_by.as_deref().map(&mut mask);
    for e in &mut view.exclusions {
        e.key = mask(&e.key);
        e.covers = mask(&e.covers);
        e.by = e.by.as_deref().map(&mut mask);
    }
    view
}

/// One row as a person is told about it: what, when and by whom. The folder and
/// the hashes stay in the record.
fn served_row(row: &HoldRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.id,
        "act": row.act.as_str(),
        "pattern": row.pattern.as_deref().map(mask),
        "at": row.recorded_at,
        "by": mask(&row.actor),
    })
}

/// Follow a pause onto the Codex observer, for the folder it watches and no
/// other: pausing one project must not stop another's live read. Returns what
/// was done, as the word the answer serves as `codex_observer`.
fn follow_observer(
    observer: Option<&Arc<observation::Observer>>,
    root: &Path,
    paused: bool,
) -> &'static str {
    let Some(observer) = observer else {
        return "not_watching";
    };
    if capture_health::observer_watches(observer, root) != Some(true) {
        return "not_watching";
    }
    observer.pause(paused);
    if paused {
        "paused"
    } else {
        "resumed"
    }
}

/// Everything the two routes share, once the body is understood: the open
/// ledger and the project. `None` means the answer has been written.
fn open_project(
    state: &State,
    project: &str,
    stream: &mut TcpStream,
) -> Result<Option<(thruline_core::ledger::Ledger, core_hq::HqProject)>> {
    let Some(ledger) = open_ledger_for_read(state, stream)? else {
        return Ok(None);
    };
    ledger
        .wait_when_busy(super::hq::BUSY_WAIT)
        .context("waiting for the record's write lock")?;
    let Some(found) = core_hq::find_project(&ledger, project).context("looking for the project")?
    else {
        refuse(stream, 404, "no_such_project")?;
        return Ok(None);
    };
    Ok(Some((ledger, found)))
}

/// What the two routes do with `record_for_person`'s answer, when it is not a
/// new row.
fn written(
    stream: &mut TcpStream,
    outcome: std::result::Result<Recorded, thruline_core::error::CoreError>,
) -> Result<Option<HoldRow>> {
    match outcome {
        Ok(Recorded::Appended(row)) => Ok(Some(*row)),
        Ok(Recorded::Refused(why)) => conflict(stream, why).map(|()| None),
        Err(problem) if problem.is_busy() => {
            super::hq::store_busy(stream, super::hq::BUSY_WAIT).map(|()| None)
        }
        Err(problem) => Err(anyhow::Error::new(problem).context("keeping the hold")),
    }
}

/// `POST /api/hq/projects/<id>/capture/pause` — pause this project's folder, or
/// resume it (row 26 of the write-address table). One `capture_holds` row.
///
/// The body is `{"act": "paused" | "resumed"}` and nothing else (an unknown
/// field — a folder, a project, a name, a time — is `400 malformed_request`; any
/// other act word, the exclusion words included, is `400 unknown_act`). The
/// folder is the project's own root as HQ derives it; the browser names none.
/// Answers:
///
///   - `200 {schema_version, act, hold, holds, codex_observer, message}` —
///     `hold` the row as served (masked), `holds` where the folder stands now
///     (`HoldsView`, with `honoured: true` and `exclusions_honoured: true`), `codex_observer` one of `paused`,
///     `resumed`, `not_watching`
///   - `400 malformed_request` / `400 unknown_act`
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_project`
///   - `409 already_paused` / `not_paused` `{message}` — the act would change
///     nothing; nothing was appended
///   - `413 too_long` — a body larger than `{"act":…}` could be
///   - `503 upgrade_pending` — the store needs `thruline upgrade`
///   - `503 store_busy` — another ThruLine process held the write lock for 30 s
pub(super) fn pause(
    state: &State,
    project: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        act: String,
    }
    let Some(request) = parse::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let act = match request.act.as_str() {
        "paused" => Act::Paused,
        "resumed" => Act::Resumed,
        _ => return refuse(stream, 400, "unknown_act"),
    };
    let Some((ledger, found)) = open_project(state, project, stream)? else {
        return Ok(());
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let hold = Hold {
        project_id: &found.id,
        project_folder: &found.root,
        actor: &actor,
        recorded_at: &at,
    };
    let Some(row) = written(stream, holds::record_for_person(&ledger, act, &hold, None))? else {
        return Ok(());
    };
    // Only after the row is in the record, and only for the folder the observer
    // watches.
    let codex_observer = follow_observer(
        state.observer.as_ref(),
        Path::new(&found.root),
        act == Act::Paused,
    );
    let view =
        holds::holds_view(&ledger, &found.root).context("reading where the folder stands")?;
    answer(
        stream,
        200,
        &serde_json::json!({
            "schema_version": core_hq::SCHEMA_VERSION,
            "act": act.as_str(),
            "hold": served_row(&row),
            "holds": masked(view, mask),
            "codex_observer": codex_observer,
            "message": if act == Act::Paused {
                "Pause recorded for this project."
            } else {
                "Resume recorded for this project."
            },
        }),
    )
}

/// `POST /api/hq/projects/<id>/capture/exclusions` — leave one path out, or put
/// it back (row 27 of the write-address table). One `capture_holds` row; the
/// first time a person turns off a built-in default it is two (the default
/// written down under ThruLine's own name, then the person's act ending it).
///
/// The body is `{"act": "excluded" | "included", "pattern": "<path>"}` and
/// nothing else. `pattern` is a built-in key (`env`, `secrets`, `generated`) or
/// one relative path pattern, trimmed; it is checked for its shape and never
/// opened. Answers:
///
///   - `200 {schema_version, act, pattern, hold, holds, message}` — `hold` is
///     the person's row
///   - `400 malformed_request` / `400 unknown_act` (the pause words are unknown
///     here)
///   - `400 bad_pattern {message}` — empty, absolute, climbing out with `..`,
///     holding a control character, or using a wildcard other than `*`
///   - `403 same_origin_required` — not from HQ's own page (the caller checks)
///   - `404 no_such_project`
///   - `409 already_excluded` / `not_excluded` `{message}` — nothing appended
///   - `409 too_long {field: "pattern", limit}` — over 256 characters
///   - `413 too_long` — a body larger than a pattern within the limit could make
///   - `503 upgrade_pending` / `503 store_busy`
pub(super) fn exclusion(
    state: &State,
    project: &str,
    body: &[u8],
    stream: &mut TcpStream,
) -> Result<()> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Request {
        act: String,
        pattern: String,
    }
    let Some(request) = parse::<Request>(body) else {
        return refuse(stream, 400, "malformed_request");
    };
    let act = match request.act.as_str() {
        "excluded" => Act::Excluded,
        "included" => Act::Included,
        _ => return refuse(stream, 400, "unknown_act"),
    };
    let pattern = request.pattern.trim();
    if pattern.chars().count() > holds::MAX_PATTERN_CHARS {
        return super::person_acts::too_long(stream, "pattern", holds::MAX_PATTERN_CHARS);
    }
    if let Some(why) = holds::pattern_problem(pattern) {
        return answer(
            stream,
            400,
            &serde_json::json!({
                "error": "bad_pattern",
                "message": holds::bad_pattern_sentence(why),
            }),
        );
    }
    let Some((ledger, found)) = open_project(state, project, stream)? else {
        return Ok(());
    };
    let actor = crate::check::account_name();
    let at = thruline_core::ingest::iso8601_utc_now();
    let hold = Hold {
        project_id: &found.id,
        project_folder: &found.root,
        actor: &actor,
        recorded_at: &at,
    };
    let Some(row) = written(
        stream,
        holds::record_for_person(&ledger, act, &hold, Some(pattern)),
    )?
    else {
        return Ok(());
    };
    let view =
        holds::holds_view(&ledger, &found.root).context("reading where the folder stands")?;
    answer(
        stream,
        200,
        &serde_json::json!({
            "schema_version": core_hq::SCHEMA_VERSION,
            "act": act.as_str(),
            "pattern": mask(pattern),
            "hold": served_row(&row),
            "holds": masked(view, mask),
            "message": if act == Act::Excluded {
                "Recorded: this path is excluded."
            } else {
                "Recorded: this path is no longer excluded."
            },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    fn observer_watching(repo: &Path, scratch: &Path) -> Arc<observation::Observer> {
        // `stop` is already set, so the observer's thread ends at once; the
        // object stays, with its state and its flag. Wait for the thread to be
        // done, so its last word about the state cannot land in the middle of
        // an assertion.
        let observer = observation::Observer::start(
            scratch.join("codex"),
            repo.to_path_buf(),
            scratch.join("ledger.db"),
            scratch.join("thruline"),
            Arc::new(AtomicBool::new(true)),
            None,
        )
        .expect("observer");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while observer.active.load(std::sync::atomic::Ordering::SeqCst) {
            assert!(
                std::time::Instant::now() < deadline,
                "the observer's thread never ended"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        observer
    }

    fn state_word(observer: &observation::Observer) -> String {
        observer.read()["state"].as_str().unwrap_or("").to_string()
    }

    #[test]
    fn a_pause_follows_onto_the_observer_of_that_folder_and_of_no_other() {
        let scratch = tempfile::tempdir().expect("tempdir");
        let watched = scratch.path().join("watched");
        let elsewhere = scratch.path().join("elsewhere");
        std::fs::create_dir_all(&watched).expect("dir");
        std::fs::create_dir_all(&elsewhere).expect("dir");
        let observer = observer_watching(&watched, scratch.path());
        let before = state_word(&observer);
        assert_ne!(before, "paused");

        // Another project's pause is not this observer's.
        assert_eq!(
            follow_observer(Some(&observer), &elsewhere, true),
            "not_watching"
        );
        assert_eq!(state_word(&observer), before, "nothing was touched");

        // The folder it watches: paused, then resumed, as row 12 does it.
        assert_eq!(follow_observer(Some(&observer), &watched, true), "paused");
        assert_eq!(state_word(&observer), "paused");
        assert_eq!(follow_observer(Some(&observer), &watched, false), "resumed");
        assert_ne!(state_word(&observer), "paused");
    }

    /// C34 pause gate (5 of 6): the observer follows the pause RECORD for the
    /// folder it watches, and the in-memory switch cannot resume what the
    /// record holds paused.
    #[test]
    fn an_observer_follows_the_pause_record_and_the_switch_cannot_resume_it() {
        use thruline_core::capture_hold::{record, Hold};
        let scratch = tempfile::tempdir().expect("tempdir");
        let watched = scratch.path().join("watched");
        let elsewhere = scratch.path().join("elsewhere");
        std::fs::create_dir_all(&watched).expect("dir");
        std::fs::create_dir_all(&elsewhere).expect("dir");
        let ledger_path = scratch.path().join("record.db");
        let start = |record: Option<std::path::PathBuf>| {
            let observer = observation::Observer::start(
                scratch.path().join("codex"),
                watched.clone(),
                scratch.path().join("exec.db"),
                scratch.path().join("thruline"),
                Arc::new(AtomicBool::new(true)),
                record,
            )
            .expect("observer");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            while observer.active.load(std::sync::atomic::Ordering::SeqCst) {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the thread never ended"
                );
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            observer
        };

        // No record given, or none on disk yet: nothing holds it.
        assert!(!start(None).held_by_record());
        assert!(!start(Some(ledger_path.clone())).held_by_record());

        // A record with a pause for ANOTHER folder: not held.
        let ledger = thruline_core::ledger::Ledger::open(&ledger_path).expect("ledger");
        fn hold(folder: &str) -> Hold<'_> {
            Hold {
                project_id: "p",
                project_folder: folder,
                actor: "robin",
                recorded_at: "2026-10-08T09:00:00Z",
            }
        }
        record(
            &ledger,
            Act::Paused,
            &hold(elsewhere.to_str().expect("utf8")),
            None,
        )
        .expect("pause elsewhere");
        assert!(!start(Some(ledger_path.clone())).held_by_record());

        // A pause for the watched folder: held, whatever the switch says.
        record(
            &ledger,
            Act::Paused,
            &hold(watched.to_str().expect("utf8")),
            None,
        )
        .expect("pause watched");
        let observer = start(Some(ledger_path.clone()));
        assert!(observer.held_by_record());
        observer.pause(false);
        assert!(
            observer.held_by_record(),
            "resuming the switch does not lift the record's pause"
        );

        // A record that cannot be read holds the observer too: a read of a
        // paused folder cannot be undone, a delayed one can.
        drop(ledger);
        for sidecar in ["record.db-wal", "record.db-shm"] {
            let _ = std::fs::remove_file(scratch.path().join(sidecar));
        }
        std::fs::write(&ledger_path, b"this is not a ledger").expect("overwrite");
        assert!(start(Some(ledger_path)).held_by_record());
    }

    #[test]
    fn with_no_observation_source_there_is_nothing_to_follow() {
        let scratch = tempfile::tempdir().expect("tempdir");
        assert_eq!(follow_observer(None, scratch.path(), true), "not_watching");
    }

    #[test]
    fn an_observer_whose_folder_cannot_be_resolved_is_not_assumed_to_watch_this_one() {
        let scratch = tempfile::tempdir().expect("tempdir");
        let gone = scratch.path().join("gone");
        let here = scratch.path().join("here");
        std::fs::create_dir_all(&here).expect("dir");
        let observer = observer_watching(&gone, scratch.path());
        let before = state_word(&observer);
        assert_eq!(
            follow_observer(Some(&observer), &here, true),
            "not_watching"
        );
        assert_eq!(state_word(&observer), before);
    }

    #[test]
    fn masked_changes_only_what_a_person_typed() {
        let ledger = thruline_core::ledger::Ledger::open_in_memory().expect("ledger");
        let hold = Hold {
            project_id: "p",
            project_folder: "/work/app",
            actor: "ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e",
            recorded_at: "2026-10-08T09:00:00Z",
        };
        holds::record_for_person(&ledger, Act::Paused, &hold, None).expect("pause");
        holds::record_for_person(
            &ledger,
            Act::Excluded,
            &hold,
            Some("keys/ghp_TLCANARYaaZZ11bbYY22ccXX33ddWW44e"),
        )
        .expect("exclude");
        let view = holds::holds_view(&ledger, "/work/app").expect("view");
        let served = masked(view.clone(), mask);
        let text = serde_json::to_string(&served).expect("json");
        assert!(!text.contains("ghp_TLCANARY"), "{text}");
        assert_eq!(served.paused_at, view.paused_at, "times are not masked");
        assert_eq!(served.exclusions.len(), view.exclusions.len());
        assert_eq!(served.exclusions[0].key, "env");
    }
}
