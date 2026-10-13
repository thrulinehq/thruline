//! BUILD-SPEC §8.1's commit trailer.
//!
//! > *`ThruLine-Receipt: <sha256 of DSSE envelope>[ rekor:<uuid>]` via
//! > `git interpret-trailers`; **never rewrite commits the user didn't ask to
//! > amend**.*
//!
//! # This module writes nothing
//!
//! It renders the trailer line and can add one to a *message*. It does not run
//! `git commit --amend`, and there is no code path here that could: rewriting a
//! commit changes its hash, which breaks every reference to it, and the spec
//! says never. The CLI prints the line, or passes it to
//! `git interpret-trailers` on a message the user is already writing.

/// The trailer key.
pub const KEY: &str = "ThruLine-Receipt";

/// `sha256:<hex>` of the DSSE envelope's exact bytes.
pub fn envelope_digest(envelope_json: &[u8]) -> String {
    format!("sha256:{}", thruline_core::chain::sha256_hex(envelope_json))
}

/// The trailer line for an envelope, with an optional Rekor entry.
pub fn line(envelope_json: &[u8], rekor_uuid: Option<&str>) -> String {
    match rekor_uuid {
        Some(uuid) => format!("{KEY}: {} rekor:{uuid}", envelope_digest(envelope_json)),
        None => format!("{KEY}: {}", envelope_digest(envelope_json)),
    }
}

/// Add the trailer to a commit message, the way `git interpret-trailers` would.
///
/// Idempotent: a message that already carries this exact trailer is returned
/// unchanged, so re-running never stacks duplicates.
pub fn add_to_message(message: &str, trailer: &str) -> String {
    if message.lines().any(|l| l.trim() == trailer.trim()) {
        return message.to_string();
    }
    let body = message.trim_end();
    // A trailer block is separated from the body by a blank line, unless the
    // message already ends in one.
    let has_trailers = body.lines().last().is_some_and(is_trailer_line);
    if has_trailers {
        format!("{body}\n{trailer}\n")
    } else {
        format!("{body}\n\n{trailer}\n")
    }
}

/// `Key: value` with no space in the key — git's own trailer shape.
fn is_trailer_line(line: &str) -> bool {
    match line.split_once(':') {
        Some((key, _)) => !key.trim().is_empty() && !key.trim().contains(' '),
        None => false,
    }
}

/// Read the receipt digest out of a commit message, if it carries one.
pub fn from_message(message: &str) -> Option<(String, Option<String>)> {
    let line = message
        .lines()
        .rev()
        .find(|l| l.trim_start().starts_with(KEY))?;
    let value = line.split_once(':')?.1.trim();
    let mut parts = value.split_whitespace();
    let digest = parts.next()?.to_string();
    let rekor = parts
        .find_map(|p| p.strip_prefix("rekor:"))
        .map(str::to_string);
    Some((digest, rekor))
}
