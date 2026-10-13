//! The synthetic claim-bearing spool fixtures in `tests/fixtures/claims/`, held
//! line by line to the rules that make them safe to fold and fit to be a
//! baseline.
//!
//! These files are **spool lines**, not transcripts: `thruline fold --spool`
//! reads them, and `crates/cli/tests/claim_fixture_sessions.rs` drives that and
//! pins the verdicts. This file checks what a hand-written line can get wrong
//! without the fold noticing:
//!
//! - **`raw_sha256`** feeds the derived event id (`capture/src/fold.rs`,
//!   `EventId::derive` over `session|seq|raw_sha256`). A digest that does not
//!   recompute from its line makes the chain a function of a typo.
//! - **The exec tree.** Every exec's `input_tree_hash` must be the work
//!   repository's tree, or no check binds by §6.1 rule 1 and every `supported`
//!   becomes `unverifiable` with nothing else failing.
//! - **Order.** A `Stop` that is not the last line leaves activity after the
//!   terminal hook, so `claims_cmd.rs`'s `lifecycle_gap` degrades capture and
//!   caps every claim in the session.
//! - **The masker.** The hook masks before it spools, and these lines never
//!   passed through it. `fold` re-masks tool inputs, outputs and errors and
//!   debug-asserts nothing changed (`fold.rs`, `compact`), so every string must
//!   already be a fixpoint of `redact::mask_recorded`.
//! - **Content.** The only absolute path is `/Users/dev/demo`, and no payload
//!   string carries a hex, base64 or credential-shaped run. The hash fields are
//!   the only hex on a line.
//!
//! Seven of these rules have a negative control at the bottom: a digest in a
//! tool payload, a path outside the demo root, a hook digest that does not
//! recompute, an exec edited after it was hashed, a `Stop` that is not last, an
//! exec tree that is not the work repository, and a string the masker rewrites.
//! Each gives the same checker a mutated copy of fixture 01 and requires a
//! report. The other checks in `violations`, `hook_line` and `exec_line` have no
//! control in this file. A rule no input can fail guards nothing.

use std::path::PathBuf;

use serde_json::Value;

/// The tree of `scripts/hq-invariance.sh`'s one-file work repository.
const TREE: &str = "c4eeb65ef9132009ad33aeb19abd8785cad50d8c";

/// The only absolute path a fixture may name.
const ROOT: &str = "/Users/dev/demo";

/// The synthetic environment behind every `env_hash`, sorted.
const ENV: &[&str] = &["LANG=C", "TERM=dumb", "TZ=UTC"];

/// The fixtures, in path order. A file added to the directory without a row
/// here fails [`the_directory_holds_exactly_the_documented_fixtures`].
const FIXTURES: &[&str] = &[
    "01-three-asks-one-done-one-claimed-one-untouched.jsonl",
    "02-supported-lint-claim.jsonl",
    "03-contradicted-tests-claim.jsonl",
    "04-insufficient-evidence-tests-claim.jsonl",
    "05-open-promise-beside-supported-build.jsonl",
];

/// `capture::claude_code::SUBSCRIBED_EVENTS`, restated because `core` cannot
/// depend on `capture`.
const SUBSCRIBED: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PostToolUse",
    "PostToolUseFailure",
    "PreCompact",
    "PostCompact",
    "SubagentStop",
    "Stop",
    "SessionEnd",
];

/// Fields that hold a digest by design, and so may hold hex.
const HASH_KEYS: &[&str] = &[
    "raw_sha256",
    "argv_hash",
    "env_hash",
    "stdout_hash",
    "stderr_hash",
    "input_tree_hash",
    "output_tree_hash",
];

/// The hook payload fields `fold` re-masks (`fold.rs`, `compact`).
const REMASKED: &[&str] = &["tool_input", "tool_response", "error"];

fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/claims")
}

fn read(name: &str) -> String {
    std::fs::read_to_string(dir().join(name)).unwrap_or_else(|e| panic!("reading {name}: {e}"))
}

fn sha(text: &str) -> String {
    thruline_core::chain::sha256_hex(text.as_bytes())
}

/// Compact JSON with every object's keys sorted: the one byte form every digest
/// on these lines is taken over.
///
/// Written out rather than left to `serde_json::to_string`, whose key order
/// depends on whether a build unifies the `preserve_order` feature.
fn canonical(value: &Value) -> String {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let fields: Vec<String> = keys
                .into_iter()
                .map(|k| {
                    format!(
                        "{}:{}",
                        Value::String(k.clone()),
                        canonical(&map[k.as_str()])
                    )
                })
                .collect();
            format!("{{{}}}", fields.join(","))
        }
        Value::Array(items) => format!(
            "[{}]",
            items.iter().map(canonical).collect::<Vec<_>>().join(",")
        ),
        other => other.to_string(),
    }
}

/// Every string in `value`, with the key it sits under.
fn strings<'a>(value: &'a Value, key: &'a str, out: &mut Vec<(&'a str, &'a str)>) {
    match value {
        Value::Object(map) => {
            for (k, v) in map {
                strings(v, k, out);
            }
        }
        Value::Array(items) => {
            for v in items {
                strings(v, key, out);
            }
        }
        Value::String(s) => out.push((key, s)),
        _ => {}
    }
}

fn longest_run(s: &str, class: fn(char) -> bool) -> usize {
    let mut best = 0;
    let mut run = 0;
    for c in s.chars() {
        if class(c) {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    best
}

fn is_hex(c: char) -> bool {
    c.is_ascii_hexdigit()
}

fn is_base64(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '+' || c == '/'
}

/// `2026-01-02T00:MM:SSZ`: the day every fixture is set on, at the second
/// precision `ingest::iso8601_utc_now` writes.
fn is_timestamp(s: &str) -> bool {
    let b = s.as_bytes();
    s.len() == 20
        && s.starts_with("2026-01-02T")
        && s.ends_with('Z')
        && b[13] == b':'
        && b[16] == b':'
        && [11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
}

/// Every rule a fixture file breaks, each message tagged with its rule.
fn violations(name: &str, text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let Some(number) = name.get(..2).and_then(|n| n.parse::<u32>().ok()) else {
        return vec![format!(
            "[shape] {name} does not start with a two-digit number"
        )];
    };
    let session = format!("cccccccc-{number:04}-4000-8000-000000000000");
    if !text.ends_with('\n') {
        out.push("[shape] the file does not end with a newline".to_string());
    }

    let mut lines: Vec<(usize, Value)> = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        match serde_json::from_str::<Value>(raw) {
            Ok(value) => {
                if raw != canonical(&value) {
                    out.push(format!(
                        "[shape] line {n}: not compact JSON with sorted keys, so no digest \
                         on it is reproducible"
                    ));
                }
                lines.push((n, value));
            }
            Err(e) => out.push(format!("[shape] line {n}: not JSON: {e}")),
        }
    }
    let Some((_, first)) = lines.first() else {
        out.push("[shape] the file holds no lines".to_string());
        return out;
    };
    if first["hook_event"] != "SessionStart" {
        out.push("[order] the first line is not SessionStart".to_string());
    }
    if lines
        .last()
        .is_none_or(|(_, last)| last["hook_event"] != "Stop")
    {
        out.push(
            "[order] the last line is not Stop, so the session would read as degraded".to_string(),
        );
    }
    let stops = lines
        .iter()
        .filter(|(_, v)| v["hook_event"] == "Stop")
        .count();
    if stops != 1 {
        out.push(format!("[order] {stops} Stop lines, not one"));
    }

    let null = Value::Null;
    let mut previous = String::new();
    for (n, line) in &lines {
        if line["session_id"] != session.as_str() {
            out.push(format!("[session] line {n}: session_id is not {session}"));
        }
        let at = line["received_at"].as_str().unwrap_or_default();
        if !is_timestamp(at) {
            out.push(format!(
                "[time] line {n}: received_at {at:?} is not 2026-01-02T00:MM:SSZ"
            ));
        }
        if at < previous.as_str() {
            out.push(format!("[time] line {n}: received_at goes backwards"));
        }
        previous = at.to_string();

        let payload: &Value = if let Some(event) = line["hook_event"].as_str() {
            hook_line(*n, event, line, &mut out);
            &line["payload"]
        } else if line["source"] == "exec-recorder" {
            exec_line(*n, &session, line, &mut out);
            &line["spec_payload"]["payload"]
        } else {
            out.push(format!(
                "[shape] line {n}: neither a hook line nor an exec-recorder line"
            ));
            &null
        };

        let mut every: Vec<(&str, &str)> = Vec::new();
        strings(line, "", &mut every);
        for (key, s) in &every {
            for needle in ["/private/", "/var/folders/", "/tmp/", "~/"] {
                if s.contains(needle) {
                    out.push(format!("[path] line {n}: `{key}` holds {needle}"));
                }
            }
            for token in s.split(|c: char| c.is_whitespace() || "\"'`()[]{}<>,;=".contains(c)) {
                if token.starts_with('/')
                    && token != ROOT
                    && !token.starts_with(&format!("{ROOT}/"))
                {
                    out.push(format!(
                        "[path] line {n}: `{key}` names {token:?}, outside {ROOT}"
                    ));
                }
            }
        }

        let mut scanned: Vec<(&str, &str)> = Vec::new();
        strings(payload, "", &mut scanned);
        for (key, s) in &scanned {
            if HASH_KEYS.contains(key) {
                continue;
            }
            if longest_run(s, is_hex) >= 16 {
                out.push(format!(
                    "[token] line {n}: `{key}` carries a hex run of 16 or more"
                ));
            }
            if longest_run(s, is_base64) >= 32 {
                out.push(format!(
                    "[token] line {n}: `{key}` carries a base64-shaped run of 32 or more"
                ));
            }
            if thruline_core::redact::resembles_known_credential(s) {
                out.push(format!(
                    "[token] line {n}: `{key}` begins like a known credential"
                ));
            }
            let (masked, record) = thruline_core::redact::mask_recorded(s);
            if masked != *s || record.masked > 0 {
                out.push(format!(
                    "[mask] line {n}: `{key}` is not a masker fixpoint: {masked:?}"
                ));
            }
        }
    }
    out
}

fn hook_line(n: usize, event: &str, line: &Value, out: &mut Vec<String>) {
    let payload = &line["payload"];
    if line["harness"] != "claude-code" || !SUBSCRIBED.contains(&event) {
        out.push(format!(
            "[hook] line {n}: {} / {event} is not a Claude Code subscription",
            line["harness"]
        ));
    }
    if payload["hook_event_name"] != event {
        out.push(format!(
            "[hook] line {n}: the payload's hook_event_name is not {event}"
        ));
    }
    for key in ["session_id", "cwd", "transcript_path"] {
        if line[key] != payload[key] {
            out.push(format!(
                "[hook] line {n}: `{key}` differs between the line and its payload"
            ));
        }
    }
    let flags = line["recognized"] == true
        && line["parse_ok"] == true
        && line["spine_complete"] == true
        && line["truncated_stdin"] == false
        && line["redactions_applied"] == 0
        && line["redaction_families"] == serde_json::json!([]);
    if !flags {
        out.push(format!(
            "[hook] line {n}: capture flags are not those of a whole, unredacted delivery"
        ));
    }
    if line["raw_sha256"] != sha(&canonical(payload)).as_str() {
        out.push(format!(
            "[hash] line {n}: raw_sha256 is not sha256 of the payload's canonical JSON"
        ));
    }
    // What `fold` actually masks: the value, rendered as `compact` renders it.
    for key in REMASKED {
        let Some(value) = payload.get(*key) else {
            continue;
        };
        let rendered = match value {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        for form in [rendered, canonical(value)] {
            let (masked, record) = thruline_core::redact::mask_recorded(&form);
            if masked != form || record.masked > 0 {
                out.push(format!(
                    "[mask] line {n}: `{key}` as fold renders it is not a masker fixpoint: \
                     {masked:?}"
                ));
            }
        }
    }
}

fn exec_line(n: usize, session: &str, line: &Value, out: &mut Vec<String>) {
    let tagged = &line["spec_payload"];
    let p = &tagged["payload"];
    if tagged["kind"] != "exec" {
        out.push(format!("[exec] line {n}: spec_payload is not an exec"));
    }
    if p["input_tree_hash"] != TREE || p["output_tree_hash"] != TREE {
        out.push(format!(
            "[tree] line {n}: the exec's tree is not the work repository's {TREE}, so its \
             check cannot bind"
        ));
    }
    let argv: Vec<&str> = p["argv"]
        .as_array()
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    if argv.is_empty() {
        out.push(format!("[exec] line {n}: the exec has no argv"));
    }
    if p["argv_hash"] != format!("sha256:{}", sha(&argv.join("\u{1}"))).as_str() {
        out.push(format!(
            "[hash] line {n}: argv_hash is not sha256 of the argv joined by U+0001"
        ));
    }
    if p["env_hash"] != format!("sha256:{}", sha(&ENV.join("\n"))).as_str() {
        out.push(format!(
            "[hash] line {n}: env_hash is not sha256 of the documented environment"
        ));
    }
    for stream in ["stdout", "stderr"] {
        let excerpt_key = format!("{stream}_excerpt");
        let hash_key = format!("{stream}_hash");
        let store_key = format!("{stream}_store");
        let excerpt = p[excerpt_key.as_str()].as_str().unwrap_or_default();
        if p[hash_key.as_str()] != format!("sha256:{}", sha(excerpt)).as_str() {
            out.push(format!(
                "[hash] line {n}: {hash_key} is not sha256 of {excerpt_key}"
            ));
        }
        if p.get(store_key.as_str()).is_some() {
            out.push(format!(
                "[exec] line {n}: {store_key} names a file the fixtures do not ship"
            ));
        }
    }
    let at = line["received_at"].as_str().unwrap_or_default();
    let started = p["started"].as_str().unwrap_or_default();
    if p["ended"] != at || !is_timestamp(started) || started > at {
        out.push(format!(
            "[time] line {n}: started/ended do not fit the line's received_at"
        ));
    }
    let material = format!("{session}|{at}|{}", canonical(tagged));
    if line["raw_sha256"] != sha(&material).as_str() {
        out.push(format!(
            "[hash] line {n}: raw_sha256 is not sha256(session|received_at|spec_payload)"
        ));
    }
}

#[test]
fn the_directory_holds_exactly_the_documented_fixtures() {
    let mut found: Vec<String> = std::fs::read_dir(dir())
        .expect("tests/fixtures/claims")
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name.ends_with(".jsonl"))
        .collect();
    found.sort();
    let documented: Vec<String> = FIXTURES.iter().map(|s| s.to_string()).collect();
    assert_eq!(
        found, documented,
        "a claim fixture was added or removed without its conventions being checked"
    );
    assert!(
        dir().join("README.md").is_file(),
        "the claim fixtures' README is missing"
    );
}

#[test]
fn every_claim_fixture_line_keeps_the_synthetic_conventions() {
    let mut broken: Vec<String> = Vec::new();
    for name in FIXTURES {
        broken.extend(
            violations(name, &read(name))
                .into_iter()
                .map(|v| format!("{name}: {v}")),
        );
    }
    assert!(
        broken.is_empty(),
        "the claim fixtures break their own conventions:\n  {}",
        broken.join("\n  ")
    );
}

/* ── negative controls: each rule, made to fire ─────────────────────── */

fn fires(text: &str, tag: &str) -> Vec<String> {
    violations(FIXTURES[0], text)
        .into_iter()
        .filter(|v| v.starts_with(tag))
        .collect()
}

fn mutated(from: &str, to: &str) -> String {
    let text = read(FIXTURES[0]);
    assert!(
        text.contains(from),
        "control premise: fixture 01 no longer holds {from:?}"
    );
    text.replacen(from, to, 1)
}

#[test]
fn control_a_digest_in_a_tool_payload_is_reported() {
    let digest_shaped = "Build at 0123456789abcdef0123456789abcdef01234567";
    // A 40-hex id is a published identifier, so the masker leaves it alone.
    // That is why the explicit run rule exists: the masker check alone would
    // pass this line.
    assert_eq!(
        thruline_core::redact::mask_recorded(digest_shaped).0,
        digest_shaped,
        "control premise: the masker now rewrites a bare hex id"
    );
    let text = mutated("Build the crate", digest_shaped);
    assert!(
        !fires(&text, "[token]").is_empty(),
        "a 40-hex id in a tool payload was not reported"
    );
}

#[test]
fn control_a_path_outside_the_demo_root_is_reported() {
    for elsewhere in [
        "/private/var/folders/xy/src/parser.rs",
        "/Users/someone/src/parser.rs",
    ] {
        let text = mutated("/Users/dev/demo/src/parser.rs", elsewhere);
        assert!(
            !fires(&text, "[path]").is_empty(),
            "{elsewhere} was not reported"
        );
    }
}

#[test]
fn control_a_hook_digest_that_does_not_recompute_is_reported() {
    let text = read(FIXTURES[0]);
    let stop: Value = serde_json::from_str(text.lines().last().expect("a last line"))
        .expect("the last line is JSON");
    let digest = stop["raw_sha256"]
        .as_str()
        .expect("a raw_sha256")
        .to_string();
    let text = mutated(&digest, &"0".repeat(64));
    assert!(
        !fires(&text, "[hash]").is_empty(),
        "a Stop line whose raw_sha256 does not recompute was not reported"
    );
}

#[test]
fn control_an_exec_edited_after_it_was_hashed_is_reported() {
    let text = mutated("\"exit_code\":0", "\"exit_code\":1");
    assert!(
        !fires(&text, "[hash]").is_empty(),
        "an exec whose exit code changed under its raw_sha256 was not reported"
    );
}

#[test]
fn control_a_stop_that_is_not_last_is_reported() {
    let text = read(FIXTURES[0]);
    let mut lines: Vec<&str> = text.lines().collect();
    let n = lines.len();
    lines.swap(n - 1, n - 2);
    let text = format!("{}\n", lines.join("\n"));
    assert!(
        !fires(&text, "[order]").is_empty(),
        "a Stop before the last line was not reported"
    );
}

#[test]
fn control_an_exec_tree_that_is_not_the_work_repository_is_reported() {
    let text = mutated(TREE, &"0".repeat(40));
    assert!(
        !fires(&text, "[tree]").is_empty(),
        "an exec naming another tree was not reported"
    );
}

#[test]
fn control_a_string_the_masker_rewrites_is_reported() {
    // A Google-shaped key, assembled so this source holds none: the prefix and
    // a body past the detector's 20-character floor.
    let prefix = ["AI", "za"].concat();
    let key = format!("{prefix}TLCANARYaaZZ11bbYY22ccXX");
    let text = mutated("Build the crate", &format!("export K={key}"));
    assert!(
        !fires(&text, "[mask]").is_empty(),
        "a key-shaped string in a tool payload was not reported by the masker check"
    );
}
