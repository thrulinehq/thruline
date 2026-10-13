//! The sensor → §4.1 bridge: sensor deliveries, rendered as spool lines.
//!
//! ## Why this exists
//!
//! ThruLine has two recorders and they were never joined.
//!
//! * The **§4.1 capture pack** (`thruline hook <harness> --spool`) appends one
//!   JSON line per delivery to a spool file. [`thruline fold`] reads that file
//!   into the execution store, and the execution store is what `explain`,
//!   `claims`, `sessions`, `promises` and therefore the whole UI read.
//! * The **sensor pack** (`thruline sensor append <Event>`, what `thruline
//!   init` installs) appends the same deliveries to `sensor_events` in the
//!   ledger, on its own hash chain. [`crate::speaker`]-side code reads that
//!   table; nothing downstream of it ever reaches the execution store.
//!
//! `hook_install::refuse_double_recorder` forbids installing both packs at
//! once, and it is right to: two recorders write two records of one session and
//! nothing afterwards says which one a claim was checked against. But that
//! left the documented install (`thruline init`) feeding a store the product's
//! own UI cannot read. On the machine this was written on the consequence was
//! exact and silent: `sensor_events` grew past 31,000 rows while the spool had
//! not been written since 2026-09-05, so `thruline ui` served a store that had
//! stopped moving and said nothing about it.
//!
//! This module is the join. It reads sensor deliveries and appends them to the
//! spool in the spool's own shape, so **the existing, tested fold path stays the
//! only writer of the execution store**. Nothing here writes an event, computes
//! a hash chain, or interprets a payload.
//!
//! ## Why append, and never rewrite
//!
//! `fold` derives each line's `seq` from its **global ordinal in the file**,
//! precisely so a partial fold cannot renumber anything that follows it. A
//! rewrite would shift every ordinal after the edit and detach already-folded
//! events from their source line. So this only ever appends, and a delivery
//! already in the spool is never emitted again.
//!
//! ## The watermark, and why it is recoverable
//!
//! `sensor_events.seq` is gapless, append-only and enforced by triggers, so
//! "everything after `n`" is a durable, replayable cursor. The cursor lives in
//! a sidecar beside the spool — but it is a *cache*, not the record: every line
//! this module writes carries its own `sensor_seq`, so a lost or corrupt
//! sidecar is recomputed from the spool itself ([`watermark`]). That is what
//! makes a crash between the append and the sidecar write harmless: the next
//! run reads the truth back out of the file it just wrote.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_json::Value;

use thruline_capture::claude_code;
use thruline_capture::redaction::Redactor;
use thruline_core::ledger::Ledger;
use thruline_core::sensor;

/// The key each emitted line carries so its origin delivery is recoverable.
const PROVENANCE: &str = "sensor_seq";

/// What one bridge pass did. Every number is reported, including the ones that
/// are easy to leave out because they are not errors.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BridgeReport {
    /// Deliveries read from `sensor_events` above the watermark.
    pub deliveries_read: u64,
    /// Lines appended to the spool.
    pub lines_appended: u64,
    /// Deliveries whose stored payload is not JSON. They are still emitted —
    /// with a null payload and `parse_ok: false` — because dropping a recorded
    /// delivery silently is the one thing this must not do. The bytes stay in
    /// `sensor_events`, which is append-only, so nothing is lost.
    pub unparseable_payloads: u64,
    /// Deliveries with no `session_id`. Emitted as-is; `fold` quarantines them
    /// and names them, which is the correct destination for a line that belongs
    /// to no chain.
    pub without_session: u64,
    /// Secrets masked by the §4.1 hook redactor while rendering these lines.
    /// The sensor stored the payload verbatim, so this is the count of things
    /// that would otherwise have reached the execution store in the clear.
    pub redactions_applied: u64,
    /// Strings the CORE masker caught that the capture redactor did not — the
    /// measured size of the divergence between the two rule sets. Every one of
    /// these is a value the §4.1 hook path would write unmasked in a release
    /// build, because the assertion that would catch it is a `debug_assert`.
    pub second_pass_masks: u64,
    /// The sensor's `hook_event` disagreed with the event named inside the
    /// payload.
    pub event_name_disagreements: u64,
    /// The sensor's `payload_sha256` disagreed with the digest of the bytes
    /// handed to the line builder.
    pub digest_disagreements: u64,
    /// Lines emitted without a `sensor_seq`. Only reachable if a rendered line
    /// fails to parse as a JSON object, which it cannot; counted rather than
    /// asserted so the impossible case is visible instead of silent.
    pub provenance_omitted: u64,
    pub watermark_before: i64,
    pub watermark_after: i64,
    /// Set when the sidecar could not be written. The append still happened and
    /// the next run recovers the watermark from the spool, so this is a
    /// degraded-but-correct state — reported rather than raised.
    pub watermark_write_error: Option<String>,
}

/// The sidecar carrying the last delivery `seq` written into `spool`.
pub fn watermark_path(spool: &Path) -> PathBuf {
    let mut name = spool
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "spool".to_string());
    name.push_str(".sensor-watermark");
    spool.with_file_name(name)
}

/// The highest sensor `seq` already present in `spool`.
///
/// Reads the sidecar when it parses, and otherwise **recomputes from the spool**
/// by scanning for the largest `sensor_seq`. The scan is the authority; the
/// sidecar only saves the read. A spool with no bridged lines yields 0, which
/// is also the right answer for a spool that does not exist yet.
pub fn watermark(spool: &Path) -> i64 {
    if let Ok(text) = std::fs::read_to_string(watermark_path(spool)) {
        if let Ok(n) = text.trim().parse::<i64>() {
            return n;
        }
    }
    let Ok(file) = std::fs::File::open(spool) else {
        return 0;
    };
    let mut high = 0i64;
    for line in BufReader::new(file).lines().map_while(std::io::Result::ok) {
        if !line.contains("sensor_seq") {
            continue;
        }
        if let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&line) {
            if let Some(n) = map.get("sensor_seq").and_then(Value::as_i64) {
                high = high.max(n);
            }
        }
    }
    high
}

/// Render one delivery through the hook's OWN line builder.
///
/// This does not construct a spool line field by field, and the distinction is
/// load-bearing rather than stylistic. `fold` asserts that re-masking a spool
/// line is a **fixpoint** — the §4.6 contract is that redaction already
/// happened before the text reached disk — and it does so with a
/// `debug_assert`, which means a hand-built unredacted line does not fail in a
/// release build. It writes the unredacted text into the execution store and
/// says nothing. The first draft of this module did exactly that, and the debug
/// assertion in `capture::fold::compact` is what caught it.
///
/// So the delivery is handed to [`claude_code::prepare`], the same function the
/// §4.1 hook calls, and the bytes it produces are the bytes that hook would
/// have written: masked, bounded, counted, with `redactions_applied` and
/// `redaction_families` describing a scrub that actually ran.
///
/// [`PROVENANCE`] is this module's one addition. `fold` reads named keys and
/// ignores the rest, so it is inert downstream and recovers the watermark.
fn spool_line(d: &sensor::Delivery, redactor: &Redactor, report: &mut BridgeReport) -> Value {
    let mut line = claude_code::prepare(d.payload.as_bytes(), &d.received_at, redactor);
    // SECOND PASS, and it is not belt-and-braces — it closes a real gap.
    //
    // `capture::redaction::Redactor` and `core::redact::mask` are two different
    // implementations with two different rule sets: `high_entropy` exists only
    // in the core one. `fold::compact` masks each extracted field with the CORE
    // masker and asserts the result is unchanged, so a payload the capture
    // redactor does not recognise — a base64 screenshot is the common case —
    // fails that assertion. It is a `debug_assert`, so in a release build the
    // blob is written to the execution store unmasked and nothing says so.
    //
    // This masks exactly what `compact` will mask, field by field, so the
    // invariant holds for every line this module writes. It does NOT fix the
    // divergence on the §4.1 hook path, which is a redaction-policy decision
    // and is reported rather than quietly changed here.
    mask_compact_fields(&mut line.payload, &mut report.second_pass_masks);

    if !line.parse_ok {
        report.unparseable_payloads += 1;
    }
    if line.session_id.is_none() {
        report.without_session += 1;
    }
    // Two independent records of the same delivery now exist. Where they
    // disagree, say so rather than silently preferring one: the sensor read
    // `hook_event` from its argv and hashed the bytes it stored, while
    // `prepare` reads the event out of the payload and hashes what it was
    // handed. A divergence means the sensor row and the payload disagree about
    // what arrived, which is worth a count even though neither is "wrong".
    if line.hook_event != d.hook_event {
        report.event_name_disagreements += 1;
    }
    if line.raw_sha256 != d.payload_sha256 {
        report.digest_disagreements += 1;
    }
    report.redactions_applied += line.redactions_applied;

    // Round-trip through the rendered form so the emitted text is exactly what
    // `render` produces, then attach provenance. Parsing what we just
    // serialised cannot fail; if it somehow did, emitting the line WITHOUT
    // provenance is still correct for the fold, and the watermark sidecar
    // remains the cursor.
    match serde_json::from_str::<Value>(line.render().trim_end()) {
        Ok(Value::Object(mut map)) => {
            map.insert(PROVENANCE.to_string(), Value::from(d.seq));
            Value::Object(map)
        }
        other => {
            report.provenance_omitted += 1;
            other.unwrap_or(Value::Null)
        }
    }
}

/// Mask exactly the fields `fold::compact` masks, exactly as it reads them.
///
/// `capture::fold::compact` is called on three payload fields and no others —
/// `tool_input`, `tool_response`, `error` (fold.rs:905, 928, 1086, 1108). For
/// each it takes the value as a string when it is one and `to_string()`s it
/// otherwise, masks that text with the CORE masker, and asserts the mask
/// changed nothing.
///
/// Two earlier attempts got this wrong and both are worth recording, because
/// each looked obviously right:
///
/// 1. **Masking each leaf string.** `core::redact` only lets entropy fire in a
///    *value position* — after `=`, `:`, or inside quotes. A leaf string handed
///    over alone is prose, so entropy never fires, and a base64 blob sailed
///    through to break the assertion.
/// 2. **Masking the serialized payload.** Closer, but the text is JSON-ESCAPED
///    there and not escaped when `compact` reads a field back out. The masker
///    is not escaping-invariant: it left the Python decorator `@app.middleware`
///    alone in `\n\n@app.middleware(\"http\")` and masked it as an email in the
///    unescaped form. Same bytes, two answers.
///
/// So the only reliable rule is to compute the same text `compact` will, mask
/// that, and — when masking changed it — store the masked STRING in the field.
/// `compact`'s string branch then returns it verbatim and re-masking is a
/// fixpoint by construction.
///
/// **Cost, stated rather than hidden:** replacing a structured field with a
/// string loses that field's shape for bridged lines. It happens only when
/// masking actually fired, `fold` reduces these three fields to strings anyway,
/// and the alternative is a `debug_assert` that is compiled out of the release
/// build while unmasked text goes to disk. The structure remains intact in
/// `sensor_events`, which is append-only.
fn mask_compact_fields(payload: &mut Value, applied: &mut u64) {
    let Some(map) = payload.as_object_mut() else {
        return;
    };
    for key in ["tool_input", "tool_response", "error"] {
        let Some(field) = map.get_mut(key) else {
            continue;
        };
        // Exactly `compact`'s own reading of the value.
        let text = match &*field {
            Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        if text.is_empty() {
            continue;
        }
        let (masked, _record) = thruline_core::redact::mask_recorded(&text);
        if masked != text {
            *applied += 1;
            *field = Value::String(masked);
        }
    }
}

/// The redactor for a delivery's own repository.
///
/// Per `cwd`, not one global redactor: a repo's `.thruline/redact.yml` is that
/// repo's policy, and these deliveries span 96 of them. A repo whose config is
/// missing or broken falls back to the built-in rules — the same choice
/// `claude_code::repo_redactor` makes on the hook path, and for the same
/// reason: built-in redaction beats refusing to redact.
fn redactor_for<'a>(cwd: Option<&str>, cache: &'a mut HashMap<String, Redactor>) -> &'a Redactor {
    let key = cwd.unwrap_or_default().to_string();
    cache.entry(key).or_insert_with_key(|k| {
        if k.is_empty() {
            Redactor::new()
        } else {
            Redactor::from_repo(Path::new(k)).unwrap_or_else(|_| Redactor::new())
        }
    })
}

/// Append every sensor delivery not yet in `spool`, and return what was done.
///
/// Opens `ledger_db` through the ordinary [`Ledger::open`], so a pending schema
/// migration refuses here exactly as it does everywhere else (A193) rather than
/// being paid for silently by a bridge run.
///
/// Writing nothing is a normal outcome, not an error: it means the spool is
/// already level with the sensor.
pub fn bridge(ledger_db: &Path, spool: &Path) -> Result<BridgeReport> {
    let ledger = Ledger::open(ledger_db)
        .with_context(|| format!("opening the sensor ledger {}", ledger_db.display()))?;
    let before = watermark(spool);
    let deliveries =
        sensor::since(&ledger, before).context("reading sensor deliveries above the watermark")?;

    let mut report = BridgeReport {
        watermark_before: before,
        watermark_after: before,
        ..BridgeReport::default()
    };
    report.deliveries_read = deliveries.len() as u64;
    if deliveries.is_empty() {
        return Ok(report);
    }

    if let Some(parent) = spool.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating the spool directory {}", parent.display()))?;
    }
    let mut buf = String::new();
    let mut highest = before;
    let mut redactors: HashMap<String, Redactor> = HashMap::new();
    for d in &deliveries {
        let line = {
            let redactor = redactor_for(d.cwd.as_deref(), &mut redactors);
            spool_line(d, redactor, &mut report)
        };
        buf.push_str(&serde_json::to_string(&line).context("serialising a spool line")?);
        buf.push('\n');
        highest = highest.max(d.seq);
    }

    // One append, then one flush, then the sidecar. A crash anywhere in here
    // costs at most a re-read: `watermark` recovers the true position from the
    // lines that did land.
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(spool)
        .with_context(|| format!("opening the spool {} to append", spool.display()))?;
    file.write_all(buf.as_bytes())
        .with_context(|| format!("appending to the spool {}", spool.display()))?;
    file.flush()
        .with_context(|| format!("flushing the spool {}", spool.display()))?;
    report.lines_appended = deliveries.len() as u64;
    report.watermark_after = highest;

    if let Err(e) = std::fs::write(watermark_path(spool), format!("{highest}\n")) {
        report.watermark_write_error = Some(e.to_string());
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn delivery(seq: i64, payload: &str) -> sensor::Delivery {
        sensor::Delivery {
            seq,
            received_at: "2026-09-09T00:00:00Z".to_string(),
            hook_event: "PreToolUse".to_string(),
            recognized: true,
            parse_ok: true,
            spine_complete: true,
            session_id: Some("s1".to_string()),
            transcript_path: Some("/t".to_string()),
            cwd: Some("/repo".to_string()),
            payload: payload.to_string(),
            payload_sha256: thruline_core::chain::sha256_hex(payload.as_bytes()),
        }
    }

    fn render(d: &sensor::Delivery, r: &mut BridgeReport) -> Value {
        spool_line(d, &Redactor::new(), r)
    }

    /// The line has to be exactly the shape `fold` already reads, or the bridge
    /// silently produces events nothing joins to.
    #[test]
    fn a_bridged_line_carries_every_field_the_spool_shape_declares() {
        let mut r = BridgeReport::default();
        let line = render(
            &delivery(7, r#"{"session_id":"s1","hook_event_name":"PreToolUse"}"#),
            &mut r,
        );
        let obj = line.as_object().expect("a line is an object");
        for key in [
            "cwd",
            "harness",
            "hook_event",
            "parse_ok",
            "payload",
            "raw_sha256",
            "received_at",
            "recognized",
            "redaction_families",
            "redactions_applied",
            "session_id",
            "spine_complete",
            "transcript_path",
            "truncated_stdin",
        ] {
            assert!(obj.contains_key(key), "spool line is missing `{key}`");
        }
        assert_eq!(
            obj[PROVENANCE],
            Value::from(7),
            "provenance must be carried"
        );
        assert_eq!(obj["harness"], Value::from("claude-code"));
    }

    /// THE REGRESSION THIS MODULE EXISTS TO NOT REPEAT.
    ///
    /// `capture::fold::compact` asserts that re-masking spool text changes
    /// nothing, because §4.6 says redaction happened before the write. It is a
    /// `debug_assert`, so a hand-built unredacted line passes a release build
    /// and writes the secret into the execution store. Building the line
    /// through `claude_code::prepare` is what makes the fixpoint hold; this
    /// test pins that, on text containing something the redactor must catch.
    #[test]
    fn a_bridged_line_is_already_masked_so_remasking_it_is_a_fixpoint() {
        let mut r = BridgeReport::default();
        let secret = r#"{"session_id":"s1","hook_event_name":"Stop","k":"sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"}"#;
        let line = render(&delivery(1, secret), &mut r);
        let text = serde_json::to_string(&line).expect("serialise");
        assert!(
            !text.contains("sk-ant-api03-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"),
            "the raw secret reached the spool line: {text}"
        );
        let (remasked, _) = thruline_core::redact::mask_recorded(&text);
        assert_eq!(
            remasked, text,
            "re-masking changed the line; the fixpoint fold asserts is broken"
        );
        assert!(
            r.redactions_applied > 0,
            "the redaction must be counted, not just performed"
        );
    }

    /// The divergence that made the first end-to-end run panic: a base64 blob
    /// the capture redactor does not recognise and the core masker does. The
    /// bridge must close it, or `fold::compact` writes the blob unmasked in a
    /// release build.
    #[test]
    fn a_high_entropy_blob_the_capture_redactor_misses_is_still_masked() {
        let mut r = BridgeReport::default();
        // Deterministic, but genuinely high-entropy: cycles the whole base64
        // alphabet, so Shannon entropy clears the 3.6 bar a run of one
        // character never would. A real screenshot payload looks like this.
        const B64: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let blob: String = (0usize..600)
            .map(|i| B64[(i * 37 + 11) % B64.len()] as char)
            .collect();
        let payload = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "PostToolUse",
            "tool_response": [{"source": {"data": blob, "type": "base64"}}],
        })
        .to_string();
        let line = render(&delivery(5, &payload), &mut r);
        assert!(
            r.second_pass_masks > 0,
            "the core masker caught nothing, so this test no longer covers the divergence"
        );
        assert_fixpoint_for_fold(&line);
    }

    /// `@app.middleware("http")` in captured stdout. The core masker reads it as
    /// an email address in the UNESCAPED form and leaves it alone in the escaped
    /// one — which is why masking the serialized payload was not enough, and why
    /// this reproduces the second real end-to-end panic.
    #[test]
    fn a_decorator_the_masker_mistakes_for_an_email_still_reaches_a_fixpoint() {
        let mut r = BridgeReport::default();
        let payload = serde_json::json!({
            "session_id": "s1",
            "hook_event_name": "PostToolUse",
            "tool_response": {"stdout": "import os\n\n@app.middleware(\"http\")\ndef f(): pass\n"},
        })
        .to_string();
        let line = render(&delivery(6, &payload), &mut r);
        assert_fixpoint_for_fold(&line);
    }

    /// THE DELIVERY SHAPE THAT PANICKED THE SECOND AND THIRD END-TO-END RUNS.
    ///
    /// A `PostToolUse` payload with the same shape as the one that arrived as
    /// `sensor_events.seq = 22420`: a multi-section shell capture whose stdout
    /// carries several decorators, one of them `@app.middleware("http")`. The
    /// committed fixture is a synthetic rewrite of that payload (invented
    /// project, invented ids) kept at full size, because two small hand-built
    /// reproductions of the case passed while the full-size payload failed.
    #[test]
    fn the_real_delivery_that_panicked_the_fold_reaches_a_fixpoint() {
        let payload = include_str!("../tests/fixtures/decorator_payload.json");
        let mut r = BridgeReport::default();
        let line = render(&delivery(22420, payload), &mut r);
        assert_fixpoint_for_fold(&line);
    }

    /// CORPUS CHECK — the whole bridged spool, not one specimen.
    ///
    /// Ignored by default because it needs a spool on disk. Point it at one and
    /// it asserts the fold's invariant on every line:
    ///
    /// ```text
    /// THRULINE_SPOOL_UNDER_TEST=/path/to/claude-code.jsonl \
    ///   cargo test -p thruline-cli --bin thruline -- --ignored fixpoint_holds
    /// ```
    #[test]
    #[ignore = "needs THRULINE_SPOOL_UNDER_TEST pointing at a spool file"]
    fn fixpoint_holds_for_every_line_of_a_real_spool() {
        let Ok(path) = std::env::var("THRULINE_SPOOL_UNDER_TEST") else {
            return;
        };
        let text = std::fs::read_to_string(&path).expect("read the spool");
        let (mut checked, mut violations) = (0u64, Vec::new());
        for (i, raw) in text.lines().enumerate() {
            let Ok(line) = serde_json::from_str::<Value>(raw) else {
                continue;
            };
            let payload = &line["payload"];
            for key in ["tool_input", "tool_response", "error"] {
                let field = &payload[key];
                let body = match field {
                    Value::Null => continue,
                    Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                if body.is_empty() {
                    continue;
                }
                checked += 1;
                let (remasked, _) = thruline_core::redact::mask_recorded(&body);
                if remasked != body {
                    violations.push((i + 1, key, line["sensor_seq"].clone()));
                }
            }
        }
        // Split by origin. A violation on a line carrying no `sensor_seq` was
        // written by the §4.1 hook: a pre-existing defect this module neither
        // caused nor can fix from here. A violation on a bridged line is ours.
        let (bridged, hook): (Vec<_>, Vec<_>) =
            violations.iter().partition(|(_, _, seq)| !seq.is_null());
        eprintln!(
            "checked {checked} field(s) · {} violation(s): {} BRIDGED, {} pre-existing hook lines",
            violations.len(),
            bridged.len(),
            hook.len()
        );
        for (line_no, key, seq) in bridged.iter().take(10) {
            eprintln!("  BRIDGED line {line_no} field `{key}` sensor_seq={seq}");
        }
        assert!(
            bridged.is_empty(),
            "{} BRIDGED field(s) are not a masking fixpoint ({} more are pre-existing on hook-written lines)",
            bridged.len(),
            hook.len()
        );
    }

    /// The invariant `capture::fold::compact` asserts, applied to the three
    /// fields it actually reads, in the form it actually reads them.
    fn assert_fixpoint_for_fold(line: &Value) {
        let payload = &line["payload"];
        for key in ["tool_input", "tool_response", "error"] {
            let text = match &payload[key] {
                Value::Null => continue,
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            let (remasked, _) = thruline_core::redact::mask_recorded(&text);
            assert_eq!(
                remasked, text,
                "re-masking `{key}` changed it; fold's debug_assert would fire here"
            );
        }
    }

    /// Rule 12: an unparseable payload is counted and flagged, never dropped.
    #[test]
    fn an_unparseable_payload_is_flagged_and_counted_rather_than_dropped() {
        let mut r = BridgeReport::default();
        let line = render(&delivery(2, "not json"), &mut r);
        assert_eq!(r.unparseable_payloads, 1);
        assert_eq!(line["parse_ok"], Value::from(false));
        assert_eq!(
            line[PROVENANCE],
            Value::from(2),
            "still emitted, with provenance"
        );
    }

    #[test]
    fn a_delivery_without_a_session_is_counted_and_still_emitted() {
        let mut r = BridgeReport::default();
        render(&delivery(3, r#"{"hook_event_name":"Stop"}"#), &mut r);
        assert_eq!(r.without_session, 1);
    }

    /// The sidecar is a cache. Losing it must not re-append a single line.
    #[test]
    fn the_watermark_is_recovered_from_the_spool_when_the_sidecar_is_gone() {
        let dir = std::env::temp_dir().join(format!("tl-bridge-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let spool = dir.join("claude-code.jsonl");
        let mut r = BridgeReport::default();
        let mut text = String::new();
        for seq in [4i64, 11, 9] {
            let line = render(
                &delivery(seq, r#"{"session_id":"s1","hook_event_name":"Stop"}"#),
                &mut r,
            );
            text.push_str(&serde_json::to_string(&line).expect("serialise"));
            text.push('\n');
        }
        std::fs::write(&spool, text).expect("write spool");
        assert!(!watermark_path(&spool).exists(), "no sidecar in this test");
        assert_eq!(watermark(&spool), 11, "the highest sensor_seq present wins");
        std::fs::remove_dir_all(&dir).ok();
    }

    /// A spool written by the §4.1 hook carries no `sensor_seq`, and must read
    /// as 0 rather than as "unknown" — otherwise the first bridge run on a
    /// machine that already used the capture pack would skip everything.
    #[test]
    fn a_spool_with_no_bridged_lines_reads_as_zero() {
        let dir = std::env::temp_dir().join(format!("tl-bridge-empty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp dir");
        let spool = dir.join("claude-code.jsonl");
        std::fs::write(&spool, "{\"session_id\":\"s\",\"hook_event\":\"Stop\"}\n").expect("write");
        assert_eq!(watermark(&spool), 0);
        assert_eq!(watermark(&dir.join("absent.jsonl")), 0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
