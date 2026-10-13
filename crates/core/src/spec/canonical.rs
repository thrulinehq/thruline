//! RFC 8785 — JSON Canonicalization Scheme.
//!
//! BUILD-SPEC §3.1: *"Canonicalization: JSON Canonicalization Scheme
//! (RFC 8785)."* Every hash in the spec's chain is taken over the bytes this
//! module produces, so a disagreement here is a disagreement about identity
//! itself — two peers that canonicalize differently compute different hashes
//! for the same event and each concludes the other forged it.
//!
//! # Why this is not hand-rolled
//!
//! RFC 8785 is three rules, two of which are traps:
//!
//! 1. Object keys sort by **UTF-16 code unit**, not by code point and not by
//!    UTF-8 byte. The two orders disagree for every key containing a
//!    non-BMP character: `U+10000` and up encode as surrogate pairs in the
//!    `0xD800..=0xDFFF` range, so in UTF-16 order they sort *before* the
//!    private-use and specials blocks `U+E000..=U+FFFF`, while in code-point
//!    order they sort after. `serde_json`'s default `Map` is a `BTreeMap<String, _>`
//!    keyed on Rust's `Ord for str`, which is UTF-8 byte order — the wrong one.
//! 2. Numbers serialize by ECMAScript `Number::toString`, i.e. the shortest
//!    round-tripping decimal, with an exponent only outside `1e-7..1e21`.
//!    That is a Ryū variant, not `{}`, not `{:?}`, and not `serde_json`'s
//!    default float printer.
//! 3. Strings use the JSON minimal escape set, and only that set.
//!
//! Rule 2 in particular cannot be written correctly by inspection. This module
//! therefore delegates to [`serde_jcs`] (which is [`ryu_js`] underneath for
//! rule 2) and treats "is that crate actually correct?" as a question to be
//! *measured*, not assumed:
//!
//! - `crates/core/tests/jcs_rfc8785.rs` drives the reference test vectors from
//!   the JCS reference implementation (`cyberphone/json-canonicalization`,
//!   vendored at `crates/core/tests/fixtures/jcs/`) through this module,
//!   comparing **bytes** against `outhex/` as well as text against `output/`.
//! - The same suite differentially compares this module against a second,
//!   independently written RFC 8785 implementation
//!   (`serde_json_canonicalizer`) over the vectors and over generated input.
//!
//! Two independent implementations agreeing with the reference vectors *and*
//! with each other is the strongest available evidence short of a proof. If
//! that suite is ever deleted, the dependency becomes an unverified claim.
//!
//! # This is canonicalization scheme SIX in this repository
//!
//! Enumerated so that nobody invents a seventh by accident, and so the reason
//! this one is not one of the existing five is on the record:
//!
//! 1. **Length-prefixed binary** (`u64` LE length + UTF-8 bytes) — `chain.rs`,
//!    `workspace_identity.rs` (`WSID_V1`), `capsule_v2.rs` (`TLCAP_V2`),
//!    `evidence_binding.rs` (`EVB_V1`), `protected_store.rs` (`TLSNAP_V1`),
//!    `capture.rs::hash_fields`.
//! 2. **Recursive sorted-key `serde_json`** — `codex.rs::canonical_json`.
//! 3. **`serde_json` default map ordering** (BTreeMap, no `preserve_order`) —
//!    `observation.rs`.
//! 4. **ASCII type-tagged framing** (`S4:text;`) — `mind_grounded.rs`.
//! 5. **RFC 8785 (JCS)** — this module.
//!
//! Scheme 1 is the strongest of the existing five and would have served, but
//! BUILD-SPEC §3.1 names RFC 8785 specifically, and it names it for a reason
//! the others cannot meet: a receipt (§8) is verified by *other people's tools*,
//! in other languages, offline. A bespoke length-prefixed encoding is verifiable
//! only by a reimplementation of itself.
//!
//! **This scheme is scoped to `spec::Event` and nothing else.** The other four
//! are each versioned (`_V1`, `_V2`) and changing one is a migration with its
//! own cost; migrating them is not a tidy-up. BUILD-PLAN C.4.
//!
//! One rule travels with this scheme and lives in [`super::event`]: **hashed
//! event payloads may not contain non-integer numbers.** ECMAScript number
//! serialization is where conforming implementations most often diverge, and no
//! §3.1 payload needs a float, so the class is removed rather than managed. See
//! `super::event::reject_non_integer_numbers`.

use serde::Serialize;
use sha2::{Digest, Sha256};

use crate::chain::to_hex;

/// A value could not be reduced to canonical JSON.
#[derive(Debug, thiserror::Error)]
pub enum CanonicalError {
    /// The value is not representable as RFC 8785 JSON.
    ///
    /// Reached by a non-finite float (`NaN`, `±Infinity`), by a map with
    /// non-string keys, or by a `Serialize` impl that itself errors. RFC 8785
    /// inherits JSON's number grammar, which has no spelling for any of them.
    #[error("value is not canonicalizable as RFC 8785 JSON: {0}")]
    NotCanonicalizable(#[source] serde_json::Error),
}

/// The canonical bytes of `value` per RFC 8785.
///
/// UTF-8, no insignificant whitespace, object keys in UTF-16 code-unit order,
/// numbers in ECMAScript `Number::toString` form.
pub fn canonical_bytes<T>(value: &T) -> Result<Vec<u8>, CanonicalError>
where
    T: Serialize + ?Sized,
{
    serde_jcs::to_vec(value).map_err(CanonicalError::NotCanonicalizable)
}

/// [`canonical_bytes`], as a `String`.
///
/// Infallible in the UTF-8 sense: RFC 8785 output is UTF-8 by definition, and
/// `serde_jcs` builds it from Rust `str`s, so the conversion cannot fail. The
/// signature still returns `Result` because canonicalization itself can.
pub fn canonical_string<T>(value: &T) -> Result<String, CanonicalError>
where
    T: Serialize + ?Sized,
{
    serde_jcs::to_string(value).map_err(CanonicalError::NotCanonicalizable)
}

/// `sha256:<lowercase hex>` over [`canonical_bytes`].
///
/// The `sha256:` prefix is part of the spec's wire format (§3.1 writes
/// `"prev_hash": "sha256:…"`), so it belongs to the value, not to its
/// rendering: a bare hex string and a prefixed one are different strings and
/// would chain differently.
pub fn canonical_digest<T>(value: &T) -> Result<String, CanonicalError>
where
    T: Serialize + ?Sized,
{
    let bytes = canonical_bytes(value)?;
    Ok(digest_of_bytes(&bytes))
}

/// `sha256:<lowercase hex>` over arbitrary bytes, in the spec's spelling.
pub fn digest_of_bytes(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    format!("sha256:{}", to_hex(&h.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn object_keys_are_sorted_by_utf16_code_unit_not_utf8_byte() {
        // The discriminating case, and the whole reason this module does not
        // sort with `BTreeMap`. U+E000 (private use) is three UTF-8 bytes
        // starting 0xEE; U+10000 is four bytes starting 0xF0. UTF-8 order puts
        // U+E000 first. UTF-16 order puts U+10000 first, because it encodes as
        // the surrogate pair D800 DC00 and 0xD800 < 0xE000.
        let value = json!({ "\u{e000}": 1, "\u{10000}": 2 });
        let out = canonical_string(&value).expect("canonicalizable");
        let hi = out.find('\u{10000}').expect("non-BMP key present");
        let lo = out.find('\u{e000}').expect("BMP key present");
        assert!(
            hi < lo,
            "RFC 8785 sorts by UTF-16 code unit: U+10000 must precede U+E000, got {out:?}"
        );

        // And prove the naive order really is the other one, so this test
        // cannot pass by accident on an implementation that does nothing.
        assert!(
            "\u{e000}" < "\u{10000}",
            "Rust's str Ord is UTF-8 byte order — if this ever flips, the \
             assertion above stops being a discriminating case"
        );
    }

    // The `333333333.33333329` literal below carries more digits than an f64
    // can hold — deliberately. It is the RFC 8785 reference vector, and the
    // whole point is that ECMAScript prints the *stored* double as
    // `333333333.3333333`. Taking clippy's advice and writing the exact f64
    // would delete the rounding step this case exists to exercise.
    #[allow(clippy::excessive_precision)]
    #[test]
    fn numbers_use_ecmascript_tostring_not_rust_display() {
        // Every one of these is a case where `{}` or `{:?}` on an f64 differs
        // from ECMAScript `Number::toString`.
        for (input, want) in [
            (json!(1e30), "1e+30"),
            (json!(4.50), "4.5"),
            (json!(0.002), "0.002"),
            (json!(1e-7), "1e-7"),
            (json!(1e21), "1e+21"),
            (json!(1e20), "100000000000000000000"),
            (json!(333333333.33333329), "333333333.3333333"),
            (json!(-0.0f64), "0"),
        ] {
            let got = canonical_string(&input).expect("canonicalizable");
            assert_eq!(got, want, "canonical form of {input}");
        }
    }

    #[test]
    fn nonfinite_floats_are_refused_rather_than_coerced() {
        // JSON has no spelling for NaN. Silently emitting `null` would make
        // two different events hash the same, so this must be an error.
        let err = canonical_bytes(&f64::NAN).expect_err("NaN is not JSON");
        assert!(matches!(err, CanonicalError::NotCanonicalizable(_)));
    }

    #[test]
    fn digest_carries_the_wire_prefix() {
        // sha256 of the empty string, the one vector every implementation
        // agrees on.
        assert_eq!(
            digest_of_bytes(b""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn whitespace_and_key_order_in_the_input_do_not_change_the_output() {
        let a: serde_json::Value =
            serde_json::from_str(r#"{ "b" : 1 , "a" : [ 1 , 2 ] }"#).expect("parses");
        let b: serde_json::Value = serde_json::from_str("{\"a\":[1,2],\n\"b\":1}").expect("parses");
        assert_eq!(
            canonical_string(&a).expect("canonicalizable"),
            canonical_string(&b).expect("canonicalizable")
        );
        assert_eq!(
            canonical_string(&a).expect("canonicalizable"),
            r#"{"a":[1,2],"b":1}"#
        );
    }
}
