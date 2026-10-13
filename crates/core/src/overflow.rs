//! MANDATE-v4 §13 (A207) — overflowed tool-result references.
//!
//! Claude Code spills a tool result that exceeds its inline budget to a side
//! file and replaces the transcript content with a reference block. The real
//! shape, read from live transcripts under `~/.claude/projects` before this
//! module was written (structure shown, content redacted):
//!
//! ```text
//! <persisted-output>
//! Output too large (85KB). Full output saved to: /…/<project>/<session>/tool-results/<id>.txt
//!
//! Preview (first 2KB):
//! …
//! </persisted-output>
//! ```
//!
//! The block arrives as the `tool_result` content string of a `user`-typed
//! line; the side-file name is either the `toolu_…` tool-use id or a short
//! id, with `.txt` or `.json` extensions observed, and it can point into a
//! *different* session's directory (resumed sessions reference their
//! predecessor's spill).
//!
//! §13 names the failure this module exists to stop: silently searching
//! only the transcript excerpt while claiming a complete search. This stage
//! builds the disclosure half — detect the reference, count it at ingest,
//! and mark any evidence search that meets one `INCOMPLETE_SEARCH`. The
//! other half (content-addressed ingestion of the referenced bytes into the
//! sealed mirror) is registered as open work in A207; until it lands, the
//! referenced bytes are **never** read live as evidence — un-ingested,
//! un-hashed files outside the ledger cannot back a verdict.

use serde::{Deserialize, Serialize};

/// The marker that opens a persisted-output reference block.
const OPEN_MARKER: &str = "<persisted-output>";
/// The phrase that names the side file, verbatim from real transcripts.
const SAVED_TO: &str = "Full output saved to: ";

/// One overflow reference, as parsed from a tool result's content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverflowRef {
    /// The absolute side-file path the transcript names.
    pub path: String,
    /// The stated size (e.g. `85KB`), verbatim, where present.
    pub stated_size: Option<String>,
}

/// Parse a tool result's text content for an overflow reference.
///
/// Detection requires the opening marker at the start of the (trimmed)
/// content AND the saved-to phrase — a tool result that merely *mentions*
/// either string deeper inside real output is not a reference block.
pub fn overflow_reference(text: &str) -> Option<OverflowRef> {
    let trimmed = text.trim_start();
    let body = trimmed.strip_prefix(OPEN_MARKER)?;
    let at = body.find(SAVED_TO)?;
    let rest = &body[at + SAVED_TO.len()..];
    let path = rest.lines().next()?.trim().to_string();
    if path.is_empty() {
        return None;
    }
    let stated_size = body[..at]
        .find("Output too large (")
        .and_then(|i| {
            body[..at][i + "Output too large (".len()..]
                .split(')')
                .next()
        })
        .map(str::to_string);
    Some(OverflowRef { path, stated_size })
}

/// Scan the lossless tool-result envelope the parser stores (`{"content":
/// …, "toolUseResult": …}`) for an overflow reference. The block's
/// `content` may be a plain string or an array of typed blocks; both real
/// shapes are covered. Malformed JSON is `None`, never a panic.
pub fn in_tool_result_envelope(envelope_json: &str) -> Option<OverflowRef> {
    let v: serde_json::Value = serde_json::from_str(envelope_json).ok()?;
    let content = v.get("content")?;
    match content {
        serde_json::Value::String(s) => overflow_reference(s),
        serde_json::Value::Array(blocks) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(serde_json::Value::as_str))
            .find_map(overflow_reference),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The real transcript shape, structure verbatim, content redacted.
    const REAL_SHAPE: &str = "<persisted-output>\nOutput too large (85KB). Full output saved \
        to: /Users/u/.claude/projects/-p/921b589e/tool-results/toolu_018w.txt\n\nPreview \
        (first 2KB):\n{\"method\":\"get_file\",…}\n…\n</persisted-output>";

    #[test]
    fn the_real_shape_is_detected_with_path_and_size() {
        let r = overflow_reference(REAL_SHAPE).expect("detected");
        assert_eq!(
            r.path,
            "/Users/u/.claude/projects/-p/921b589e/tool-results/toolu_018w.txt"
        );
        assert_eq!(r.stated_size.as_deref(), Some("85KB"));
    }

    #[test]
    fn ordinary_output_mentioning_the_phrases_is_not_a_reference() {
        assert_eq!(overflow_reference("build ok — no overflow"), None);
        // The phrases deep inside real output do not make a reference block:
        // the opening marker must lead the content.
        assert_eq!(
            overflow_reference("log line\n<persisted-output>\nFull output saved to: /x\n"),
            None
        );
        // A marker with no saved-to phrase is not a reference either.
        assert_eq!(overflow_reference("<persisted-output>\njust text"), None);
        assert_eq!(overflow_reference(""), None);
    }

    #[test]
    fn envelope_scanning_covers_both_content_shapes_and_never_panics() {
        let string_shape = serde_json::json!({ "content": REAL_SHAPE }).to_string();
        assert!(in_tool_result_envelope(&string_shape).is_some());

        let array_shape = serde_json::json!({
            "content": [{ "type": "text", "text": REAL_SHAPE }],
            "toolUseResult": { "truncated": true }
        })
        .to_string();
        assert!(in_tool_result_envelope(&array_shape).is_some());

        for garbage in ["", "not json", "{}", "[]", r#"{"content": 7}"#] {
            assert_eq!(in_tool_result_envelope(garbage), None, "{garbage:?}");
        }
    }
}
