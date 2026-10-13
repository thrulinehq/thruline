//! Streaming capture of a child process's output: **redact, then hash, then
//! bound** — in that order, over an unbounded stream, in bounded memory.
//!
//! # Why this file is not `read_to_end` + `mask` + `sha256`
//!
//! Two requirements pull against each other.
//!
//! - **§4.6 / §3.1:** *"Redaction runs before hashing so hashes are stable and
//!   never over secrets."* So the digest cannot be taken over the raw bytes as
//!   they arrive.
//! - **BUILD-PLAN M0.8(c):** *"a command that writes 100 MB to stdout does not
//!   blow memory (streamed digest)."* So the whole output cannot be held in
//!   order to redact it first.
//!
//! The resolution is to redact in **blocks**, feed the redacted bytes to the
//! hasher, and never hold more than one block. What makes that safe rather than
//! merely cheap is where the block boundary is allowed to fall.
//!
//! # The boundary rule, which is the whole correctness argument
//!
//! A credential split across a block boundary would be masked in neither half:
//! the detectors work on shapes, and half a shape is not one. This is the same
//! generator as A209-C3, where a *truncation* cut a token below its detector's
//! threshold and it reached a sealed capsule.
//!
//! So a block ends only at a newline, and never inside a
//! `-----BEGIN … -----END` region — the one secret format that is legitimately
//! multi-line. A region that never ends is capped at [`MAX_BLOCK`] so a
//! malicious `-----BEGIN` cannot be used to force unbounded buffering; when the
//! cap fires it is **recorded**, not silently applied.
//!
//! The rule is not argued from, it is measured: `streaming_redaction_gate.rs`
//! runs the whole 1,958-row redaction corpus through this sink at adversarial
//! chunk sizes and asserts the result is byte-identical to redacting the same
//! text in one piece.
//!
//! # What lands on disk
//!
//! §4.4 says *"Capture stdout/stderr of wrapped runners fully to a temp file"*.
//! What is written is the **redacted** stream, not the raw one. A file in the
//! user's home directory holding a plaintext credential is a leak whether or
//! not anything ever hashes it, and this is the path where a credential most
//! plausibly appears in plaintext — a failing test printing an environment, a
//! curl trace, a stack trace carrying a config object.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::redaction::Redactor;

/// Block size at which the sink tries to flush. Chosen so a 100 MB stream costs
/// this much resident memory rather than 100 MB.
pub const BLOCK: usize = 256 * 1024;

/// Hard cap on a deferred block. Reached only when a `-----BEGIN` marker never
/// closes; recorded on [`Captured::block_cap_hit`] when it fires.
pub const MAX_BLOCK: usize = 4 * 1024 * 1024;

/// Bound for the excerpt §3.1 stores on the event.
pub const EXCERPT_BOUND: usize = 4_000;

/// What a captured stream turned out to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Captured {
    /// `sha256:…` over the **redacted** bytes.
    pub hash: String,
    /// Bounded, redacted, head of the stream.
    pub excerpt: String,
    pub bytes_raw: u64,
    pub bytes_redacted: u64,
    pub redactions_applied: u64,
    pub redaction_families: Vec<String>,
    /// Where the full redacted stream was written, if it was.
    pub stored_path: Option<PathBuf>,
    /// Why it was not. **Counted, never silent** — a missing output file means
    /// a check cannot be parsed later, and a caller must be able to say so.
    pub store_error: Option<String>,
    /// The stream was not valid UTF-8 and was lossily decoded.
    pub invalid_utf8: bool,
    /// A `-----BEGIN` region exceeded [`MAX_BLOCK`] and was flushed anyway.
    pub block_cap_hit: bool,
}

impl Captured {
    /// The digest with its `sha256:` prefix removed, which is how the output
    /// store names files.
    pub fn hex(&self) -> &str {
        self.hash.strip_prefix("sha256:").unwrap_or(&self.hash)
    }
}

/// Accumulates a child's output stream.
pub struct CaptureSink {
    redactor: Redactor,
    block: usize,
    hasher: Sha256,
    store_dir: Option<PathBuf>,
    file: Option<std::fs::File>,
    temp_path: Option<PathBuf>,
    store_error: Option<String>,
    /// Bytes that did not form a complete UTF-8 sequence in the last chunk.
    byte_carry: Vec<u8>,
    pending: String,
    excerpt: String,
    bytes_raw: u64,
    bytes_redacted: u64,
    redactions: u64,
    families: BTreeSet<String>,
    invalid_utf8: bool,
    block_cap_hit: bool,
}

impl CaptureSink {
    /// A sink that writes its redacted stream into `store_dir`, if given.
    ///
    /// A store directory that cannot be created is recorded on
    /// [`Captured::store_error`] and capture continues: losing the full output
    /// must not also lose the digest and the excerpt.
    pub fn new(redactor: Redactor, store_dir: Option<&Path>, tag: &str) -> CaptureSink {
        let mut store_error = None;
        let mut file = None;
        let mut temp_path = None;
        let mut dir = None;
        if let Some(root) = store_dir {
            match std::fs::create_dir_all(root) {
                Ok(()) => {
                    let name = format!(
                        ".partial-{}-{}-{tag}",
                        std::process::id(),
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_nanos())
                            .unwrap_or(0)
                    );
                    let path = root.join(name);
                    match std::fs::File::create(&path) {
                        Ok(f) => {
                            file = Some(f);
                            temp_path = Some(path);
                            dir = Some(root.to_path_buf());
                        }
                        Err(e) => store_error = Some(format!("{}: {e}", path.display())),
                    }
                }
                Err(e) => store_error = Some(format!("{}: {e}", root.display())),
            }
        }
        CaptureSink {
            redactor,
            block: BLOCK,
            hasher: Sha256::new(),
            store_dir: dir,
            file,
            temp_path,
            store_error,
            byte_carry: Vec::new(),
            pending: String::new(),
            excerpt: String::new(),
            bytes_raw: 0,
            bytes_redacted: 0,
            redactions: 0,
            families: BTreeSet::new(),
            invalid_utf8: false,
            block_cap_hit: false,
        }
    }

    /// Flush at a smaller block size. **Test seam**, and the reason it exists
    /// is that at the production 256 KB every row of the redaction corpus fits
    /// in one block, so a gate run at the default would prove nothing about
    /// boundaries. `streaming_redaction_gate.rs` drives it down to a single
    /// byte, which forces a cut at every line the corpus contains.
    pub fn with_block_size(mut self, block: usize) -> CaptureSink {
        self.block = block.max(1);
        self
    }

    /// Feed the next raw chunk from the child.
    pub fn push(&mut self, chunk: &[u8]) {
        self.bytes_raw += chunk.len() as u64;
        self.byte_carry.extend_from_slice(chunk);
        // Decode the longest valid UTF-8 prefix and keep the remainder, so a
        // multi-byte character split across two reads is not mangled into two
        // replacement characters — which would change the redacted bytes and
        // therefore the digest, depending only on how the pipe happened to
        // deliver the stream.
        let carry = std::mem::take(&mut self.byte_carry);
        match std::str::from_utf8(&carry) {
            Ok(text) => self.pending.push_str(text),
            Err(e) => {
                let valid = e.valid_up_to();
                self.pending
                    .push_str(&String::from_utf8_lossy(&carry[..valid]));
                match e.error_len() {
                    // Truncated at the end: keep it for the next chunk.
                    None => self.byte_carry.extend_from_slice(&carry[valid..]),
                    // Genuinely invalid: replace it and continue, flagged.
                    Some(len) => {
                        self.invalid_utf8 = true;
                        self.pending.push('\u{FFFD}');
                        self.byte_carry
                            .extend_from_slice(&carry[(valid + len).min(carry.len())..]);
                    }
                }
            }
        }
        self.maybe_flush(false);
    }

    /// Flush whole blocks that end at a safe boundary.
    fn maybe_flush(&mut self, final_flush: bool) {
        loop {
            if final_flush {
                if self.pending.is_empty() {
                    return;
                }
                let block = std::mem::take(&mut self.pending);
                self.emit(&block);
                return;
            }
            if self.pending.len() < self.block {
                return;
            }
            match self.safe_cut(&self.pending) {
                Some(cut) => {
                    let rest = self.pending.split_off(cut);
                    let block = std::mem::replace(&mut self.pending, rest);
                    self.emit(&block);
                }
                None => {
                    if self.pending.len() >= MAX_BLOCK {
                        // Recorded, not silent: the boundary guarantee was
                        // given up here and the caller has to be able to say so.
                        self.block_cap_hit = true;
                        let block = std::mem::take(&mut self.pending);
                        self.emit(&block);
                        return;
                    }
                    return;
                }
            }
        }
    }

    /// The largest offset ≤ `text.len()` that ends a line and is not inside an
    /// unterminated `-----BEGIN … -----END` region.
    fn safe_cut(&self, text: &str) -> Option<usize> {
        let mut cut = None;
        let mut depth = 0i32;
        let mut offset = 0usize;
        for line in text.split_inclusive('\n') {
            offset += line.len();
            if line.contains("-----BEGIN") {
                depth += 1;
            }
            if line.contains("-----END") {
                depth = (depth - 1).max(0);
            }
            if line.ends_with('\n') && depth == 0 {
                cut = Some(offset);
            }
        }
        cut
    }

    /// Redact one block, then hash it, then store it, then extend the excerpt.
    fn emit(&mut self, block: &str) {
        let redacted = self.redactor.redact(block);
        self.redactions += redacted.applied();
        self.families.extend(redacted.families().iter().cloned());
        let bytes = redacted.text().as_bytes();
        self.bytes_redacted += bytes.len() as u64;
        self.hasher.update(bytes);
        if let Some(file) = self.file.as_mut() {
            if let Err(e) = file.write_all(bytes) {
                self.store_error = Some(format!("writing the captured stream: {e}"));
                self.file = None;
            }
        }
        if self.excerpt.len() < EXCERPT_BOUND {
            // Bounded through the redactor's own seam, so the cut cannot land
            // inside a `[masked: …]` marker (A209-C4/B2) and cannot precede a
            // mask (A209-C3).
            let room = EXCERPT_BOUND - self.excerpt.len();
            self.excerpt.push_str(redacted.bounded(room).text());
        }
    }

    /// Close the stream and name what it was.
    pub fn finish(mut self) -> Captured {
        if !self.byte_carry.is_empty() {
            let carry = std::mem::take(&mut self.byte_carry);
            self.invalid_utf8 = true;
            self.pending.push_str(&String::from_utf8_lossy(&carry));
        }
        self.maybe_flush(true);
        let digest = self.hasher.finalize();
        let mut hex = String::with_capacity(digest.len() * 2);
        for byte in digest.iter() {
            hex.push_str(&format!("{byte:02x}"));
        }
        let mut stored_path = None;
        if let (Some(file), Some(temp), Some(dir)) = (
            self.file.take(),
            self.temp_path.take(),
            self.store_dir.take(),
        ) {
            drop(file);
            // Content-addressed by the digest of what is in it, so two
            // identical outputs cost one file and the exec event's
            // `stdout_hash` already names the path.
            let final_path = dir.join(format!("{hex}.out"));
            if final_path.exists() {
                let _ = std::fs::remove_file(&temp);
                stored_path = Some(final_path);
            } else {
                match std::fs::rename(&temp, &final_path) {
                    Ok(()) => stored_path = Some(final_path),
                    Err(e) => {
                        self.store_error = Some(format!("naming {}: {e}", final_path.display()));
                        let _ = std::fs::remove_file(&temp);
                    }
                }
            }
        }
        Captured {
            hash: format!("sha256:{hex}"),
            excerpt: self.excerpt,
            bytes_raw: self.bytes_raw,
            bytes_redacted: self.bytes_redacted,
            redactions_applied: self.redactions,
            redaction_families: self.families.into_iter().collect(),
            stored_path,
            store_error: self.store_error,
            invalid_utf8: self.invalid_utf8,
            block_cap_hit: self.block_cap_hit,
        }
    }
}

#[cfg(test)]
mod tests;
