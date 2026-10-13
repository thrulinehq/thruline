//! **`tool_result.ok` has one reader, and this is what keeps it that way.**
//!
//! # The finding this exists to stop recurring
//!
//! BUILD-PLAN B.1's most important discovery was that Claude Code's
//! `PostToolUse` fires **only on success**, so an adapter built from the spec
//! alone would capture a session's green test runs and never its red ones — a
//! systematic bias in exactly the direction that destroys a false-success
//! detector.
//!
//! The same finding has a second life one consumer downstream. Codex fires one
//! `PostToolUse` for both outcomes and its documented payload names neither, so
//! `ok` there is not derivable and the fold stamps `ok_basis: "not_stated"`
//! beside it. **A reader that takes `ok` without the basis reads an unstated
//! Codex outcome as a success** — the same bias, produced by a different layer,
//! and invisible because the code looks correct.
//!
//! It was not hypothetical. When this census was written the workspace had
//! **two** readers: one guarded, and one in `explain`'s timeline renderer whose
//! `else` branch printed `ok` for every Codex tool result a human would read.
//!
//! # What a census has to assert, learned the hard way
//!
//! A guard that asserts two things *co-occur* somewhere in a file is not a
//! guard. The Sigstore-keyless document guard required `"Sigstore keyless"` and
//! `"not built"` to appear anywhere in a 1,300-line file: it fired when the
//! subject was deleted and **stayed green when its sentence was inverted**,
//! because an unrelated sentence still carried the predicate.
//!
//! So this asserts a **relationship**: every site that reads `ok` off a
//! `tool_result` payload must be [`thruline_capture::fold::tool_outcome`], and
//! there must be exactly one of it. Not "`ok` and `ok_basis` both appear
//! somewhere" — the two-reader defect above would have passed that, since
//! `explain_cmd.rs` mentioned `ok_basis` in the *other* function.
//!
//! # …and a search space no wider than the thing it guards
//!
//! The first version of that relationship matched the four characters `ok`
//! wherever they appeared as a JSON key, and so it accused
//! `cli/src/json_out.rs` — whose `thruline.doctor` envelope has a field called
//! `ok` that has nothing to do with a tool result — of being a second reader.
//! A census whose search space is wider than its subject is this program's
//! most-repeated defect; D6 records the same shape one round earlier, where a
//! guard grepped plaintext for a value stored in base64.
//!
//! `ok_basis` names itself. A bare `ok` does not, so it counts only with
//! [`reads_a_payload`] agreeing — and the narrowing is itself controlled by
//! [`an_unrelated_key_spelled_ok_is_not_a_tool_result_read`], which requires
//! the census to catch the identical spelling on a payload in the same file it
//! ignores it on a `Map`. A census that no longer catches what it was built
//! for is worse than the false positive it was narrowed to remove.
//!
//! # And both controls
//!
//! A census with no negative control is a tautology — this repository has
//! shipped one. [`the_census_fires_on_an_unguarded_read`] injects exactly the
//! defect into a real source file and asserts the census sees it;
//! [`the_census_finds_the_one_legitimate_reader`] asserts it is not simply
//! matching nothing.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use thruline_capture::fold::{tool_outcome, ToolOutcome, OK_NOT_STATED};

/// Every `src/` tree in the workspace.
///
/// Derived from `Cargo.toml`'s `members`, which is why that key must stay on
/// one line — the same constraint `redact::MASK_CALL_SITES` depends on, and for
/// the same reason.
fn member_sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("members"))
        .expect("workspace members");
    let members: Vec<String> = line
        .split('"')
        .filter(|s| s.contains('/') || s.starts_with("crates"))
        .map(str::to_string)
        .collect();
    assert!(
        members.len() >= 8,
        "the members list did not parse — is it still on one line? got {members:?}"
    );
    members
        .iter()
        .map(|m| root.join(m).join("src"))
        .filter(|p| p.is_dir())
        .collect()
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    paths.sort();
    for path in paths {
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            if let Ok(text) = std::fs::read_to_string(&path) {
                f(&path, &text);
            }
        }
    }
}

/// The spellings that read **`ok_basis`**, which needs no further evidence.
///
/// `ok_basis` is this program's own invention. Nothing else in the workspace
/// is spelled that way, so the name alone identifies the payload it belongs
/// to and the read can be counted wherever it appears.
///
/// Matched on **code**, never on a comment: this file and `fold.rs` both
/// discuss `ok` at length, and a census that read prose would accuse its own
/// explanation.
///
/// Reads only — writing the field is what the fold is *for*. `OK_NOT_STATED`
/// is deliberately absent: it is a constant name, and mentioning it is not
/// reading a payload. Including it caught the constant's own declaration and
/// `build_payload`'s choice of which basis to *write*, which is the census
/// accusing the producer of consuming.
const BASIS_SPELLINGS: &[&str] = &["get(\"ok_basis\")", "[\"ok_basis\"]"];

/// The spellings that read a field named **`ok`** — which is not, on its own,
/// evidence of anything.
///
/// **This is the correction.** These four characters are the most ordinary
/// key name in JSON, and the census used to count every one of them. It
/// therefore accused `json_out.rs`'s `assert_eq!(obj["ok"], true)` — a test of
/// the `--json` envelope's `thruline.doctor` body, which has a field called
/// `ok` for reasons that have nothing to do with a tool result — of being an
/// unguarded reader of a Codex outcome. The search space was wider than the
/// thing being guarded, which is this program's most-repeated defect: a census
/// that greps plaintext for a value stored in base64 has the same shape.
///
/// So a bare `ok` is counted only with [`reads_a_payload`] agreeing that the
/// thing being read is a JSON payload. What must NOT happen is the reverse
/// error — narrowing until the census stops catching the reader it was built
/// for — so the evidence is deliberately generous, and every clause of it is
/// witnessed by `the_census_fires_on_an_unguarded_read` and by the mutation
/// recorded in this file's history.
const OK_SPELLINGS: &[&str] = &["get(\"ok\")", "[\"ok\"]"];

/// Words that only appear in a file that handles tool-result payloads.
///
/// A file that never names a tool result cannot be reading one. A file that
/// does is treated as payload-handling throughout, because a payload reaches a
/// read site through calls (`ev.payload()`, a match arm's binding, a closure
/// parameter) that no line-oriented census can type-check — and missing a real
/// reader is the failure this exists to prevent, so the doubt resolves toward
/// counting it.
const TOOL_RESULT_VOCABULARY: &[&str] = &["tool_result", "ToolResult", "PostToolUse", "ok_basis"];

/// Type spellings whose binding is a JSON value, and therefore a candidate
/// payload.
///
/// `pub fn tool_outcome(payload: &Value)` and the injected control's
/// `fn injected_unguarded(p: &serde_json::Value)` both declare their receiver
/// this way; `let obj = out.as_object().unwrap()` — the false positive — does
/// not, and cannot, because it is not a `Value` at all.
const VALUE_TYPES: &[&str] = &[
    ": &Value",
    ": Value",
    ": &mut Value",
    ": &serde_json::Value",
    ": serde_json::Value",
    ": &mut serde_json::Value",
];

/// Is this read taking `ok` off a JSON payload?
///
/// Three independent affirmatives, any one of which is enough:
///
/// 1. the file handles tool-result payloads at all ([`TOOL_RESULT_VOCABULARY`]);
/// 2. the receiver is bound, in this function, to a `serde_json::Value`;
/// 3. the receiver is not a plain local at all but the result of a call or an
///    index — `ev.payload().get("ok")`, the shape a census that only knows
///    identifiers is blind to.
fn reads_a_payload(
    code: &str,
    spelling: &str,
    file_handles_tool_results: bool,
    bound: &[String],
) -> bool {
    if file_handles_tool_results {
        return true;
    }
    let Some(at) = code.find(spelling) else {
        return false;
    };
    // `get("ok")` is reached through `.`; `["ok"]` sits directly on its
    // receiver. Step back over the accessor punctuation to the receiver's
    // last character.
    let head = &code[..at];
    let head = head.strip_suffix('.').unwrap_or(head);
    match head.chars().next_back() {
        // A call or an index produced this value: no identifier to check, and
        // no grounds to dismiss it.
        Some(')') | Some(']') => true,
        Some(c) if c.is_alphanumeric() || c == '_' => {
            let start = head
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map(|i| i + 1)
                .unwrap_or(0);
            bound.iter().any(|b| b == &head[start..])
        }
        // `self`-less field access, a macro body, anything else: counted.
        _ => true,
    }
}

/// Identifiers this line binds to a JSON value, for [`reads_a_payload`].
fn value_bindings(code: &str, into: &mut Vec<String>) {
    for ty in VALUE_TYPES {
        let mut rest = code;
        while let Some(at) = rest.find(ty) {
            let head = &rest[..at];
            let start = head
                .rfind(|c: char| !(c.is_alphanumeric() || c == '_'))
                .map(|i| i + 1)
                .unwrap_or(0);
            let ident = &head[start..];
            if !ident.is_empty() {
                into.push(ident.to_string());
            }
            rest = &rest[at + ty.len()..];
        }
    }
}

/// `(file, function)` for every site in the workspace that reads the pair.
///
/// `injection` adds a synthetic function to one named file, for the negative
/// control, so the control exercises the real walker rather than a copy of it.
fn readers(injection: Option<(&str, &str)>) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    let mut injected_anywhere = false;
    for src in member_sources() {
        walk(&src, &mut |path, raw| {
            let name = path
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or_default()
                .to_string();
            let mut text = raw.to_string();
            if let Some((target, code)) = injection {
                if name == target {
                    text.push_str(code);
                    injected_anywhere = true;
                }
            }
            // Code only, here as below. Everything from `//` on is prose, and
            // a file must not become payload-handling because a comment
            // explains what a payload is.
            let file_handles_tool_results = text.lines().any(|l| {
                let code = l.split("//").next().unwrap_or("");
                TOOL_RESULT_VOCABULARY.iter().any(|w| code.contains(w))
            });
            let mut current_fn = String::new();
            let mut bound: Vec<String> = Vec::new();
            for line in text.lines() {
                let trimmed = line.trim_start();
                if let Some(rest) = trimmed
                    .strip_prefix("pub fn ")
                    .or_else(|| trimmed.strip_prefix("fn "))
                {
                    current_fn = rest
                        .split(['(', '<'])
                        .next()
                        .unwrap_or_default()
                        .trim()
                        .to_string();
                    bound.clear();
                }
                let code = line.split("//").next().unwrap_or("");
                value_bindings(code, &mut bound);
                let hit = BASIS_SPELLINGS.iter().any(|s| code.contains(s))
                    || OK_SPELLINGS.iter().any(|s| {
                        code.contains(s)
                            && reads_a_payload(code, s, file_handles_tool_results, &bound)
                    });
                if hit {
                    found.insert((name.clone(), current_fn.clone()));
                }
            }
        });
    }
    if let Some((target, _)) = injection {
        assert!(
            injected_anywhere,
            "the control never found {target} to inject into"
        );
    }
    found
}

/// **The single permitted reader.** A second entry is a build failure, and
/// adding one deliberately means writing down why here.
///
/// One entry, not a list with room to grow. `redact::MASK_CALL_SITES` learned
/// this the expensive way: a predicate serving two callers with opposite
/// safety requirements leaked every time, and only a declared-and-gated
/// registry stopped it.
const PERMITTED: &[(&str, &str)] = &[("fold.rs", "tool_outcome")];

// ---------------------------------------------------------------------------
// The census
// ---------------------------------------------------------------------------

#[test]
fn tool_result_ok_has_exactly_one_reader() {
    let found = readers(None);
    let permitted: BTreeSet<(String, String)> = PERMITTED
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let extra: Vec<_> = found.difference(&permitted).collect();
    assert!(
        extra.is_empty(),
        "these read `tool_result.ok` or `ok_basis` outside `fold::tool_outcome`: \
         {extra:?}.\nA reader that takes `ok` without the basis treats an \
         unstated Codex outcome as a success — the \
         `PostToolUse`-fires-only-on-success bias, one consumer downstream. Call \
         `fold::tool_outcome` and match on its three states."
    );
}

/// The positive control: the census is not simply matching nothing.
///
/// Without this, deleting `tool_outcome` entirely would make the census above
/// pass with an empty set — a guard that is happiest when the thing it guards
/// does not exist.
#[test]
fn the_census_finds_the_one_legitimate_reader() {
    let found = readers(None);
    assert!(
        found.contains(&("fold.rs".to_string(), "tool_outcome".to_string())),
        "the census found no reader at all, so it is asserting nothing: {found:?}"
    );
    assert_eq!(found.len(), 1, "{found:?}");
}

/// The negative control: inject the defect and watch it fire.
///
/// The injected function is the *exact* shape of the reader that was in
/// `explain_cmd.rs` when this file was written — `== Some(false)` with an
/// `else` that says `ok` — so this control reproduces a real defect rather
/// than a plausible one.
#[test]
fn the_census_fires_on_an_unguarded_read() {
    let baseline = readers(None);
    let injected = readers(Some((
        "timeline.rs",
        "\npub fn injected_unguarded(p: &serde_json::Value) -> &'static str {\n    \
         if p.get(\"ok\").and_then(serde_json::Value::as_bool) == Some(false) {\n        \
         \"FAILED\"\n    } else {\n        \"ok\"\n    }\n}\n",
    )));
    assert!(
        injected.iter().any(|(_, n)| n == "injected_unguarded"),
        "the census did not see an injected unguarded read: {injected:?}"
    );
    assert_ne!(
        baseline, injected,
        "an injected read left the census unchanged — it is not checking anything"
    );
    // ...and it lands OUTSIDE the permitted set, so the census's own assertion
    // fails on it rather than merely noticing it. Without this line the control
    // proves the walker sees the injection, not that the guard rejects it.
    let permitted: BTreeSet<(String, String)> = PERMITTED
        .iter()
        .map(|(f, n)| ((*f).to_string(), (*n).to_string()))
        .collect();
    let would_fail: Vec<_> = injected.difference(&permitted).collect();
    assert!(
        !would_fail.is_empty(),
        "the injected read was somehow permitted: {injected:?}"
    );
}

/// **The false positive, kept as a control so it cannot come back quietly.**
///
/// `json_out.rs` asserts `obj["ok"] == true` about the `--json` envelope's
/// `thruline.doctor` body. That `ok` is a field of an unrelated shape, on a
/// `serde_json::Map` that is not a payload at all, in a file that never names
/// a tool result — and the census called it an unguarded reader of a Codex
/// outcome, because it matched the four characters and nothing more.
///
/// This is deliberately **not** a bare negative assertion, which would pass
/// just as happily if the matcher stopped matching anything. It injects both
/// shapes into the same file and requires the census to separate them: the
/// unrelated key is ignored, the identical spelling on a payload is caught.
#[test]
fn an_unrelated_key_spelled_ok_is_not_a_tool_result_read() {
    let injected = readers(Some((
        "timeline.rs",
        "\npub fn injected_unrelated_ok(\n    \
         out: &serde_json::Map<String, serde_json::Value>,\n) -> bool {\n    \
         out[\"ok\"] == serde_json::Value::Bool(true)\n}\n\
         \npub fn injected_payload_ok(payload: &serde_json::Value) -> bool {\n    \
         payload[\"ok\"] == serde_json::Value::Bool(true)\n}\n",
    )));
    assert!(
        !injected.iter().any(|(_, n)| n == "injected_unrelated_ok"),
        "an unrelated field named `ok` was counted as a tool-result read — the \
         search space is wider than the thing being guarded: {injected:?}"
    );
    assert!(
        injected.iter().any(|(_, n)| n == "injected_payload_ok"),
        "the SAME spelling on a payload went uncaught, so the narrowing above \
         opened the hole this census exists to close: {injected:?}"
    );
}

/// A comment is not a read. The census must not accuse its own explanation.
#[test]
fn prose_about_ok_is_not_a_read() {
    let injected = readers(Some((
        "timeline.rs",
        "\n// A comment mentioning p.get(\"ok\") and OK_NOT_STATED and [\"ok_basis\"].\n\
         pub fn injected_comment_only() -> u8 {\n    0\n}\n",
    )));
    assert!(
        !injected.iter().any(|(_, n)| n == "injected_comment_only"),
        "a comment was counted as a read, which would make every explanation of \
         this rule a violation of it"
    );
}

// ---------------------------------------------------------------------------
// The behaviour the census protects
// ---------------------------------------------------------------------------

/// Three states, and the third is neither of the other two.
#[test]
fn an_unstated_outcome_is_neither_a_success_nor_a_failure() {
    let stated_success = serde_json::json!({ "ok": true, "ok_basis": "event_name" });
    let stated_failure = serde_json::json!({ "ok": false, "ok_basis": "event_name" });
    let by_field = serde_json::json!({ "ok": false, "ok_basis": "tool_response.error" });
    let unstated = serde_json::json!({ "ok": true, "ok_basis": OK_NOT_STATED });

    assert_eq!(tool_outcome(&stated_success), ToolOutcome::Succeeded);
    assert_eq!(tool_outcome(&stated_failure), ToolOutcome::Failed);
    assert_eq!(tool_outcome(&by_field), ToolOutcome::Failed);
    assert_eq!(
        tool_outcome(&unstated),
        ToolOutcome::NotStated,
        "Codex's `ok` is `true` only because a bool has no third value; the \
         basis is what says it means nothing"
    );

    // The predicate a stop classifier uses: evidence of failure, which an
    // absence of evidence does not supply.
    assert!(tool_outcome(&stated_failure).is_stated_failure());
    assert!(tool_outcome(&by_field).is_stated_failure());
    assert!(!tool_outcome(&unstated).is_stated_failure());
    assert!(!tool_outcome(&stated_success).is_stated_failure());
}

/// A payload with no `ok` at all is not a passing one.
#[test]
fn an_absent_field_is_not_a_passing_one() {
    assert_eq!(tool_outcome(&serde_json::json!({})), ToolOutcome::NotStated);
    assert_eq!(
        tool_outcome(&serde_json::json!({ "tool": "Bash" })),
        ToolOutcome::NotStated
    );
}

/// The rendered word for `NotStated` borrows neither outcome's.
///
/// This is what the unguarded reader got wrong: it printed `ok`, which a
/// person reads as "the tool worked".
#[test]
fn the_unstated_label_is_its_own_word() {
    let labels: Vec<&str> = [
        ToolOutcome::Succeeded,
        ToolOutcome::Failed,
        ToolOutcome::NotStated,
    ]
    .iter()
    .map(|o| o.label())
    .collect();
    assert_eq!(labels, vec!["ok", "FAILED", "outcome not stated"]);
    let distinct: BTreeSet<&str> = labels.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        3,
        "two states render as one word: {labels:?}"
    );
}
