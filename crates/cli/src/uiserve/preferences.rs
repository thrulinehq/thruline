//! HQ Next — a person's preferences (C30 R-PREFS).
//!
//!   - `GET  /api/preferences` — every preference, its default, and what the
//!     file on disk said that was not used.
//!   - `POST /api/preferences` — set one or more known preferences.
//!
//! **Preferences are not record (C30 R-LEDGER).** They live in one file,
//! `preferences.json`, in the folder that holds the ledger this server opened
//! (the folder `/api/storage` names; `~/.thruline` for the real product). This
//! module opens no ledger, appends no row and adds no DDL.
//!
//! The keys are a closed set with typed values, named from the Settings design
//! (`docs/hq-next/DESIGN-SPEC.md` §1.11, §2.9 and §3.10's state table). A POST
//! naming any other key, or any other value, is refused with 400 naming the key,
//! and nothing is written.
//!
//! **Writing.** The whole file is written to a new temporary file in the same
//! folder (created exclusively, so an existing name is never opened), flushed,
//! and renamed over `preferences.json`. A rename replaces a name and never
//! follows it, so a symlink is never written through; and if `preferences.json`
//! is a symlink (or anything but a regular file) the write is refused outright
//! rather than replacing the link a person put there.
//!
//! **Reading.** A missing file is the defaults. A file that cannot be read,
//! is not JSON, is a symlink, or carries a value this version does not accept
//! reads as the defaults for what it could not use, and the problem is served
//! beside them — never silently. Keys this version does not know are listed by
//! name, not served, and kept on the next write so a newer version's choices
//! are not lost.

use super::*;

/// The schema of both the served document and the file.
pub(super) const PREFERENCES_SCHEMA_VERSION: u64 = 1;

/// The file's name inside the records folder.
pub(super) const FILE_NAME: &str = "preferences.json";

/// The largest body a POST may carry. Every key at its longest value is about
/// 230 bytes; this leaves room for whitespace and nothing else.
pub(super) const BODY_MAX: usize = 1024;

/// One preference: its name and the values it accepts.
enum Kind {
    /// One of a fixed list of words. The first is the default.
    Word(&'static [&'static str]),
    /// A switch, with its default.
    Switch(bool),
    /// A set of named switches, each with its default.
    Switches(&'static [(&'static str, bool)]),
}

/// The closed set, in the order Settings draws them. Defaults are the
/// design's (§3.10 state table, with C51's four notifications off by default:
/// `face` plain, `redactCopy` true, `cvd` false,
/// `density` comfortable, `motion` system, `digest` false,
/// `keys` on).
const KEYS: &[(&str, Kind)] = &[
    ("face", Kind::Word(&["plain", "technical"])),
    ("density", Kind::Word(&["comfortable", "compact"])),
    // "Follow this Mac" / "Always reduce".
    ("motion", Kind::Word(&["system", "reduce"])),
    // §1.11: the one variant is "Proven is teal".
    ("colour_vision", Kind::Word(&["standard", "proven_teal"])),
    // "On" / "Only with ⌥" / "Off" (WCAG 2.1.4).
    ("keys", Kind::Word(&["on", "mod", "off"])),
    // "When you export, also make a masked copy".
    ("masked_copy", Kind::Switch(true)),
    // What the open HQ tab may announce. Browser permission is handled by the
    // page before it keeps an On choice, and is not a preference itself.
    (
        "notifications",
        Kind::Switches(&[
            ("ready", false),
            ("not_backed", false),
            ("agent_waiting", false),
            ("recording_stopped", false),
        ]),
    ),
    // "A weekly summary, written to a file".
    ("weekly_summary", Kind::Switch(false)),
];

/// One write at a time, so two POSTs cannot each read the old file and the
/// second undo the first.
static WRITE: Mutex<()> = Mutex::new(());

pub(super) fn owns(path: &str) -> bool {
    path == "/api/preferences"
}

fn file_of(state: &State) -> PathBuf {
    let folder = state
        .db
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    folder.join(FILE_NAME)
}

fn json(stream: &mut TcpStream, status: u16, value: &serde_json::Value) -> Result<()> {
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

fn origin_is_ours(state: &State, headers: &RequestHeaders) -> bool {
    if headers.origin_count != 1 {
        return false;
    }
    let port = port_of(&state.origin);
    headers.origin == format!("http://{}", state.origin)
        || headers.origin == format!("http://localhost:{port}")
}

pub(super) fn handle(
    state: &Arc<State>,
    method: &str,
    query: &str,
    headers: &RequestHeaders,
    reader: &mut BufReader<TcpStream>,
    stream: &mut TcpStream,
) -> Result<()> {
    if !query.is_empty() {
        return refuse(stream, 400, "unknown_query_parameter");
    }
    if headers.malformed {
        return refuse(stream, 400, "malformed_request");
    }
    let file = file_of(state);
    match method {
        "GET" => {
            if !headers.content_lengths.is_empty()
                || !headers.content_types.is_empty()
                || headers.transfer_encoding
            {
                return refuse(stream, 400, "malformed_request");
            }
            json(stream, 200, &document(&file, &read(&file)))
        }
        "POST" => {
            if !origin_is_ours(state, headers) {
                return refuse(stream, 403, "same_origin_required");
            }
            let body = match read_json_body(reader, headers, BODY_MAX) {
                Ok(body) => body,
                Err(BodyProblem::TooLarge) => return refuse(stream, 413, "request_too_large"),
                Err(BodyProblem::BadRequest) => return refuse(stream, 400, "malformed_request"),
            };
            let Ok(serde_json::Value::Object(asked)) = serde_json::from_slice(&body) else {
                return refuse(stream, 400, "malformed_request");
            };
            if asked.is_empty() {
                return refuse(stream, 400, "no_preferences");
            }
            if let Err(bad) = validate(&asked) {
                return json(stream, 400, &bad);
            }
            let _one_writer = match WRITE.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            let now = read(&file);
            match now.file {
                FileState::NotRegular => {
                    return json(
                        stream,
                        409,
                        &serde_json::json!({
                            "error": "preferences_file_not_regular",
                            "detail": "preferences.json is a symlink or not a regular file, so it was left as it is and nothing was written",
                            "path": file.display().to_string(),
                        }),
                    )
                }
                FileState::NewerSchema => {
                    return json(
                        stream,
                        409,
                        &serde_json::json!({
                            "error": "preferences_file_newer",
                            "detail": "preferences.json was written by a newer ThruLine, so it was left as it is and nothing was written",
                            "path": file.display().to_string(),
                        }),
                    )
                }
                _ => {}
            }
            let mut values = now.values.clone();
            for (key, value) in &asked {
                if let (Some(serde_json::Value::Object(have)), serde_json::Value::Object(set)) =
                    (values.get_mut(key), value)
                {
                    for (sub, on) in set {
                        have.insert(sub.clone(), on.clone());
                    }
                } else {
                    values.insert(key.clone(), value.clone());
                }
            }
            // Keys a newer version wrote are kept, not served.
            let mut whole = now.unknown.clone();
            for (key, value) in values {
                whole.insert(key, value);
            }
            let text = serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": PREFERENCES_SCHEMA_VERSION,
                "preferences": whole,
            }))?;
            if let Err(err) = write_replacing(&file, &text) {
                return json(
                    stream,
                    500,
                    &serde_json::json!({
                        "error": "write_failed",
                        "detail": err.to_string(),
                        "path": file.display().to_string(),
                    }),
                );
            }
            json(stream, 200, &document(&file, &read(&file)))
        }
        _ => refuse(stream, 405, "method_not_allowed"),
    }
}

/// Every key named, every value one this version accepts; otherwise the first
/// problem, naming its key.
fn validate(
    asked: &serde_json::Map<String, serde_json::Value>,
) -> std::result::Result<(), serde_json::Value> {
    for (key, value) in asked {
        let Some((_, kind)) = KEYS.iter().find(|(name, _)| name == key) else {
            return Err(serde_json::json!({ "error": "unknown_preference", "key": key }));
        };
        match (kind, value) {
            (Kind::Word(words), serde_json::Value::String(word))
                if words.contains(&word.as_str()) => {}
            (Kind::Word(words), _) => {
                return Err(
                    serde_json::json!({ "error": "invalid_value", "key": key, "allowed": words }),
                )
            }
            (Kind::Switch(_), serde_json::Value::Bool(_)) => {}
            (Kind::Switch(_), _) => {
                return Err(serde_json::json!({
                    "error": "invalid_value", "key": key, "allowed": [true, false],
                }))
            }
            (Kind::Switches(subs), serde_json::Value::Object(set)) if !set.is_empty() => {
                for (sub, on) in set {
                    let named = format!("{key}.{sub}");
                    if !subs.iter().any(|(s, _)| s == sub) {
                        return Err(
                            serde_json::json!({ "error": "unknown_preference", "key": named }),
                        );
                    }
                    if !on.is_boolean() {
                        return Err(serde_json::json!({
                            "error": "invalid_value", "key": named, "allowed": [true, false],
                        }));
                    }
                }
            }
            (Kind::Switches(subs), _) => {
                let names: Vec<&str> = subs.iter().map(|(s, _)| *s).collect();
                return Err(serde_json::json!({
                    "error": "invalid_value", "key": key,
                    "allowed": format!("an object of one or more of {} set to true or false", names.join(", ")),
                }));
            }
        }
    }
    Ok(())
}

fn default_of(kind: &Kind) -> serde_json::Value {
    match kind {
        Kind::Word(words) => serde_json::Value::from(words.first().copied().unwrap_or("")),
        Kind::Switch(on) => serde_json::Value::Bool(*on),
        Kind::Switches(subs) => serde_json::Value::Object(
            subs.iter()
                .map(|(s, on)| (s.to_string(), serde_json::Value::Bool(*on)))
                .collect(),
        ),
    }
}

fn defaults() -> serde_json::Map<String, serde_json::Value> {
    KEYS.iter()
        .map(|(name, kind)| (name.to_string(), default_of(kind)))
        .collect()
}

#[derive(Clone, Copy, PartialEq)]
enum FileState {
    Absent,
    Read,
    NotRegular,
    Unreadable,
    NotJson,
    Unrecognised,
    NewerSchema,
}

impl FileState {
    fn word(self) -> &'static str {
        match self {
            FileState::Absent => "absent",
            FileState::Read => "read",
            FileState::NotRegular => "not_a_regular_file",
            FileState::Unreadable => "unreadable",
            FileState::NotJson => "not_json",
            FileState::Unrecognised => "unrecognised",
            FileState::NewerSchema => "newer_schema",
        }
    }
}

struct Reading {
    file: FileState,
    /// Every known key: the file's value where it was usable, else the default.
    values: serde_json::Map<String, serde_json::Value>,
    /// Known keys whose value came from the file.
    from_file: Vec<String>,
    /// Keys the file holds that this version does not know, as written.
    unknown: serde_json::Map<String, serde_json::Value>,
    problems: Vec<serde_json::Value>,
}

fn read(file: &Path) -> Reading {
    let mut reading = Reading {
        file: FileState::Absent,
        values: defaults(),
        from_file: Vec::new(),
        unknown: serde_json::Map::new(),
        problems: Vec::new(),
    };
    let meta = match std::fs::symlink_metadata(file) {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => return reading,
        Err(err) => {
            reading.file = FileState::Unreadable;
            reading.problems.push(serde_json::json!({
                "problem": "unreadable", "detail": err.to_string(),
            }));
            return reading;
        }
    };
    if !meta.file_type().is_file() {
        reading.file = FileState::NotRegular;
        reading.problems.push(serde_json::json!({
            "problem": "not_a_regular_file",
            "detail": "preferences.json is a symlink or not a regular file; it is not followed, and the defaults are served",
        }));
        return reading;
    }
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(err) => {
            reading.file = FileState::Unreadable;
            reading.problems.push(serde_json::json!({
                "problem": "unreadable", "detail": err.to_string(),
            }));
            return reading;
        }
    };
    let parsed: Option<serde_json::Map<String, serde_json::Value>> =
        match serde_json::from_slice::<serde_json::Value>(&bytes) {
            Ok(serde_json::Value::Object(top)) => Some(top),
            _ => None,
        };
    let Some(top) = parsed else {
        reading.file = FileState::NotJson;
        reading.problems.push(serde_json::json!({
            "problem": "not_json",
            "detail": "preferences.json is not a JSON object; the defaults are served, and the next change replaces it",
        }));
        return reading;
    };
    match top.get("schema_version").and_then(|v| v.as_u64()) {
        Some(PREFERENCES_SCHEMA_VERSION) => {}
        Some(newer) if newer > PREFERENCES_SCHEMA_VERSION => {
            reading.file = FileState::NewerSchema;
            reading.problems.push(serde_json::json!({
                "problem": "newer_schema",
                "detail": format!("preferences.json is schema {newer}, newer than this ThruLine reads; the defaults are served and the file is left as it is"),
            }));
            return reading;
        }
        _ => {
            reading.file = FileState::Unrecognised;
            reading.problems.push(serde_json::json!({
                "problem": "no_schema_version",
                "detail": "preferences.json has no schema_version this ThruLine reads; the defaults are served, and the next change replaces it",
            }));
            return reading;
        }
    }
    reading.file = FileState::Read;
    let Some(serde_json::Value::Object(held)) = top.get("preferences") else {
        reading.problems.push(serde_json::json!({
            "problem": "no_preferences_object",
            "detail": "preferences.json has no preferences object; the defaults are served",
        }));
        return reading;
    };
    for (key, value) in held {
        let Some((_, kind)) = KEYS.iter().find(|(name, _)| name == key) else {
            reading.unknown.insert(key.clone(), value.clone());
            continue;
        };
        match kind {
            Kind::Switches(subs) => {
                // Each switch on its own: one bad switch does not cost the others.
                let serde_json::Value::Object(set) = value else {
                    reading
                        .problems
                        .push(serde_json::json!({ "problem": "invalid_value", "key": key }));
                    continue;
                };
                let mut used = false;
                for (sub, on) in set {
                    let named = format!("{key}.{sub}");
                    if !subs.iter().any(|(s, _)| s == sub) {
                        reading
                            .problems
                            .push(serde_json::json!({ "problem": "unknown_key", "key": named }));
                    } else if let (Some(b), Some(map)) = (
                        on.as_bool(),
                        reading.values.get_mut(key).and_then(|v| v.as_object_mut()),
                    ) {
                        map.insert(sub.clone(), serde_json::Value::Bool(b));
                        used = true;
                    } else {
                        reading
                            .problems
                            .push(serde_json::json!({ "problem": "invalid_value", "key": named }));
                    }
                }
                if used {
                    reading.from_file.push(key.clone());
                }
            }
            _ => {
                let mut one = serde_json::Map::new();
                one.insert(key.clone(), value.clone());
                if validate(&one).is_ok() {
                    reading.values.insert(key.clone(), value.clone());
                    reading.from_file.push(key.clone());
                } else {
                    reading
                        .problems
                        .push(serde_json::json!({ "problem": "invalid_value", "key": key }));
                }
            }
        }
    }
    reading
}

fn document(file: &Path, reading: &Reading) -> serde_json::Value {
    let unknown: Vec<&String> = reading.unknown.keys().collect();
    serde_json::json!({
        "schema_version": PREFERENCES_SCHEMA_VERSION,
        "preferences": reading.values,
        "defaults": defaults(),
        "path": file.display().to_string(),
        "file": reading.file.word(),
        "from_file": reading.from_file,
        "unknown_keys": unknown,
        "problems": reading.problems,
    })
}

/// Write `bytes` as `file`'s whole content: a new temporary file in the same
/// folder, flushed to disk, then renamed over `file`. Refuses when `file`
/// exists and is not a regular file, so a symlink is never replaced or
/// written through.
fn write_replacing(file: &Path, bytes: &[u8]) -> std::io::Result<()> {
    match std::fs::symlink_metadata(file) {
        Ok(meta) if !meta.file_type().is_file() => {
            return Err(std::io::Error::other(
                "preferences.json is not a regular file; refusing to replace it",
            ))
        }
        Ok(_) => {}
        Err(err) if err.kind() == ErrorKind::NotFound => {}
        Err(err) => return Err(err),
    }
    let folder = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let temp = folder.join(format!(".{FILE_NAME}.{}.{nanos}.tmp", std::process::id()));
    let result = (|| {
        let mut out = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        out.write_all(bytes)?;
        out.sync_all()?;
        drop(out);
        std::fs::rename(&temp, file)?;
        if let Ok(dir) = std::fs::File::open(folder) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result
}
