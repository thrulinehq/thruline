//! Harness adapters, the generic exec recorder, and the pre-hash redaction pass.
//!
//! **Scaffold. Nothing here is implemented yet.** BUILD-SPEC §4 governs this
//! crate; the slices are BUILD-PLAN M0.5 through M0.8. The crate exists now so
//! that the workspace graph, the lint gates, and — importantly —
//! `crates/core/tests/a209_masking_corpus.rs`'s call-site census, which walks
//! every member declared in the workspace manifest, cover it from the first
//! commit rather than from the commit that first fills it in.
//!
//! # Read this before writing §4.6 (redaction)
//!
//! Redaction is **M0.5**, and it is the one slice in M0 that must not be
//! written from the spec alone.
//!
//! 1. **Wrap `thruline_core::redact`; do not rewrite it.** That module is 2,892
//!    lines carrying seventeen rounds of measured leak repairs (register entries
//!    A209 through A209-C15 in `docs/P0-BUILD-BRIEF.md`). §4.6's list — AWS
//!    keys, GitHub tokens, JWTs, `Authorization:` headers, `.env` values,
//!    private-key blocks, entropy — is a *subset* of what it already does, and
//!    every one of the hard cases it handles was found by a leak, not by
//!    reading: terminated versus truncated PEM blocks, per-line prefixes, wrap
//!    width, 64-character Ed25519 bodies, transport decoration, mentioned-versus-
//!    standalone END banners in both letter cases.
//! 2. **A new call site must be classified.** `redact::MASK_CALL_SITES` is a
//!    census with a negative control; adding a `mask(...)` call anywhere in the
//!    workspace without adding it there fails the suite, deliberately.
//! 3. **The corpus is the gate, and the gate has a number.** The independent
//!    verifier's 1,958-row regression corpus scores **1748 PASS / 210 FAIL**
//!    against the current masker, with every one of the 210 mapping to an
//!    explicitly disclosed residual and **zero undisclosed leaks**. 210 is a
//!    ceiling this crate's §4.6 layer must not exceed — *and the failures must
//!    be the same 210*. A different 210 is a regression at an identical count.
//!    That corpus is not in this repository yet. Wiring it in is a precondition
//!    for M0.5, not a follow-up: shipping a redaction layer whose only evidence
//!    is its author's own hand-written cases is how the previous seventeen
//!    rounds each began.
//! 4. **Measure by surviving key material**, with a sliding window over the real
//!    payload — never by the presence of a `[masked: …]` marker. A
//!    marker-counting metric once reported 17 of 17 masked when 4 were safe.
//! 5. **Redaction runs before hashing and before bounding**, in that order.
//!    Cutting a credential's body below its detector threshold is a measured
//!    production leak (A209-C3), not a theoretical one, and
//!    `redact::TRUNCATION_SITES` enumerates the sites that got it wrong.
//!
//! # Two obligations this crate inherits from `core`
//!
//! - **NFC at the boundary.** `spec::event` normalises text at construction, so
//!   a path this crate hands it is normalised *for the hash*. But if this crate
//!   compares a filesystem path against a stored one, it must normalise on both
//!   sides — macOS hands back decomposed names and Linux does not.
//! - **`fs_change` must come from worktree observation, never solely from a tree
//!   diff.** A `.gitattributes` clean filter makes the worktree change while the
//!   tree hash does not (see `spec::snapshot`), so a diff-derived `fs_change` is
//!   blind to exactly the write it exists to detect.

pub mod bounded;
pub mod claude_code;
pub mod codex;
pub mod codex_rollout;
pub mod exec;
pub mod fold;
pub mod gemini;
pub mod harness;
pub mod pty;
pub mod redaction;
pub mod shim;
pub mod stream;

/// The BUILD-SPEC sections that govern this crate.
pub const SPEC_SECTION: &str = "§4";

#[cfg(test)]
mod tests {
    #[test]
    fn spec_section_is_recorded() {
        assert!(super::SPEC_SECTION.starts_with('§'));
    }
}
