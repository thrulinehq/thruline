use sha2::{Digest, Sha256};

/// Lowercase hex, per CLAUDE.md.
pub(crate) fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        // `write!` to a String cannot fail, but push_str keeps it infallible
        // without an unwrap.
        s.push_str(HEX[(b >> 4) as usize]);
        s.push_str(HEX[(b & 0x0f) as usize]);
    }
    s
}

const HEX: [&str; 16] = [
    "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "a", "b", "c", "d", "e", "f",
];

pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    to_hex(&h.finalize())
}

/// Genesis link: `prev_hash = sha256(project_id)`.
///
/// `project_id` is deterministic (the munged directory name), so a ledger
/// rebuilt from scratch reproduces the same chain.
pub fn genesis_prev_hash(project_id: &str) -> String {
    sha256_hex(project_id.as_bytes())
}

/// `chain_hash = sha256(prev_hash ‖ raw_sha256 ‖ session_id ‖ seq)`, where `‖`
/// is the length-prefixed encoding defined below.
///
/// Naive concatenation is ambiguous: `session_id` and `seq` are both
/// variable-width and adjacent, so `("s1", 12)` and `("s", 112)` serialize to
/// the same bytes and produce the same link. Session ids are 36-char UUIDs in
/// practice, but nothing enforces that, and a collision in the chain function
/// is precisely what the ledger exists to make impossible.
///
/// Each string field is therefore encoded as its byte length (`u64`,
/// little-endian) followed by its UTF-8 bytes, and `seq` as a fixed-width
/// `i64` little-endian. That is injective for every possible input, so
/// distinct `(prev_hash, raw_sha256, session_id, seq)` tuples always produce
/// distinct pre-images.
///
/// This encoding is part of the on-disk contract — changing it invalidates
/// every stored chain (§3 of the build brief records it).
pub fn chain_hash(prev_hash: &str, raw_sha256: &str, session_id: &str, seq: i64) -> String {
    let mut h = Sha256::new();
    for field in [prev_hash, raw_sha256, session_id] {
        h.update((field.len() as u64).to_le_bytes());
        h.update(field.as_bytes());
    }
    h.update(seq.to_le_bytes());
    to_hex(&h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_matches_known_vector() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hex_is_lowercase_and_full_width() {
        let h = sha256_hex(b"anything");
        assert_eq!(h.len(), 64);
        assert!(h
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)));
    }

    #[test]
    fn chain_is_deterministic() {
        let a = chain_hash("00", "11", "sess", 3);
        let b = chain_hash("00", "11", "sess", 3);
        assert_eq!(a, b);
    }

    #[test]
    fn chain_is_sensitive_to_every_input() {
        let base = chain_hash("aa", "bb", "sess", 1);
        assert_ne!(base, chain_hash("ab", "bb", "sess", 1));
        assert_ne!(base, chain_hash("aa", "bc", "sess", 1));
        assert_ne!(base, chain_hash("aa", "bb", "sesx", 1));
        assert_ne!(base, chain_hash("aa", "bb", "sess", 2));
    }

    #[test]
    fn encoding_is_injective_across_field_boundaries() {
        // Regression: naive concatenation made these two tuples identical,
        // because "s1" ‖ "12" and "s" ‖ "112" are the same bytes.
        let p = sha256_hex(b"prev");
        let r = sha256_hex(b"raw");
        assert_ne!(chain_hash(&p, &r, "s1", 12), chain_hash(&p, &r, "s", 112));
        assert_ne!(chain_hash(&p, &r, "ab", 1), chain_hash(&p, &r, "a", 1));
        // ...and across the hash/hash and hash/session boundaries too.
        assert_ne!(
            chain_hash("aa", "bb", "s", 1),
            chain_hash("a", "abb", "s", 1)
        );
        assert_ne!(
            chain_hash("aa", "bb", "s", 1),
            chain_hash("aa", "b", "bs", 1)
        );
    }

    #[test]
    fn chain_is_stable_against_accidental_reencoding() {
        // Pins the wire format. If this value changes, every stored ledger is
        // invalidated — that must be a deliberate migration, never a refactor.
        assert_eq!(
            chain_hash(
                "0000000000000000000000000000000000000000000000000000000000000000",
                "1111111111111111111111111111111111111111111111111111111111111111",
                "11111111-2222-4333-8444-555555555555",
                1
            ),
            "bd257c8ce0b5b349abed4fdd77191c3e6529bf4fe95110722ba82bd2d0593188"
        );
    }

    #[test]
    fn genesis_depends_only_on_project_id() {
        assert_eq!(genesis_prev_hash("proj"), sha256_hex(b"proj"));
        assert_ne!(genesis_prev_hash("proj"), genesis_prev_hash("proj2"));
    }
}
