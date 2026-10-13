//! RFC 6962 Merkle inclusion proofs.
//!
//! This module is the reason this crate exists. `sigstore` 0.14.0 ships correct
//! Merkle primitives and **never calls them from the bundle verify path** —
//! `src/bundle/verify/verifier.rs:195-202` carries `TODO(tnytown): Merkle
//! inclusion` and `TODO(tnytown) SET verification` exactly where steps 5 and 6
//! belong. Without them a bundle verifier checks that a log entry is *present
//! and self-consistent*, not that it is *in the log*.
//!
//! # The hashes are domain-separated, and that is the security property
//!
//! ```text
//! leaf(d)      = SHA256(0x00 ‖ d)
//! node(l, r)   = SHA256(0x01 ‖ l ‖ r)
//! ```
//!
//! The prefixes stop a leaf from being reinterpreted as an interior node — the
//! second-preimage attack the RFC's design exists to prevent. Omitting them
//! produces a verifier that accepts forged proofs, and nothing about the output
//! looks different.
//!
//! # Structure is checked before arithmetic
//!
//! The proof's *length* is a function of `(index, tree_size)` alone, so a proof
//! of the wrong length is rejected by name rather than by producing a mismatched
//! root. This matters because the two failures have different causes: a wrong
//! length is a malformed or mis-shard-ed proof, and a wrong root is a forged or
//! tampered one.
//!
//! **It does not, however, catch the shard mistake.** Rekor entries carry two
//! different indices — a global `logIndex` and a shard-local
//! `inclusionProof.logIndex` — and for many values both pass the length check
//! while computing different roots. The shard-local one is correct; see
//! [`crate::bundle::InclusionProof::log_index`].

use sha2::{Digest, Sha256};

use crate::error::{Error, Result};

pub const LEAF_PREFIX: u8 = 0x00;
pub const NODE_PREFIX: u8 = 0x01;

/// `SHA256(0x00 ‖ data)`.
///
/// `data` is the **base64-decoded `canonicalizedBody`**, used verbatim. Never
/// re-serialize the JSON first: round-tripping happens to preserve the bytes
/// for some encoders and not others, so a verifier that re-encodes works until
/// it meets an entry type whose ordering or whitespace differs.
pub fn leaf_hash(data: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([LEAF_PREFIX]);
    h.update(data);
    h.finalize().into()
}

/// `SHA256(0x01 ‖ left ‖ right)`.
pub fn node_hash(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update([NODE_PREFIX]);
    h.update(left);
    h.update(right);
    h.finalize().into()
}

/// How many hashes a proof for `index` in a tree of `size` must contain.
///
/// Returns `(inner, border)`. `inner` is the number of steps on the path from
/// the leaf up to the point where the path meets the tree's right border;
/// `border` is the number of remaining right-hand siblings. Their sum is the
/// only valid proof length. This mirrors `transparency-dev`'s
/// `decompInclProof`.
pub fn proof_shape(index: u64, size: u64) -> (usize, usize) {
    // Number of bits needed to represent index ^ (size - 1): the depth at which
    // the leaf's path diverges from the last leaf's path.
    let inner = u64::BITS as usize - (index ^ (size - 1)).leading_zeros() as usize;
    let border = (index >> inner).count_ones() as usize;
    (inner, border)
}

/// Reconstruct the tree root implied by an inclusion proof.
///
/// Returns the computed root; the caller compares it to the checkpoint's. It is
/// deliberately *not* a boolean — a function that returns `bool` invites
/// `if !verify(..) {}` with an inverted test, and a root the caller must look at
/// cannot be ignored by accident.
pub fn root_from_inclusion_proof(
    index: u64,
    size: u64,
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
) -> Result<[u8; 32]> {
    if size == 0 || index >= size {
        return Err(Error::InclusionIndexOutOfRange {
            index,
            tree_size: size,
        });
    }
    let (inner, border) = proof_shape(index, size);
    if proof.len() != inner + border {
        return Err(Error::InclusionProofShape {
            found: proof.len(),
            expected: inner + border,
            index,
            tree_size: size,
        });
    }

    // Up through the inner path: bit `i` of the index says which side we are on.
    let mut res = *leaf;
    for (i, sibling) in proof[..inner].iter().enumerate() {
        if (index >> i) & 1 == 0 {
            res = node_hash(&res, sibling);
        } else {
            res = node_hash(sibling, &res);
        }
    }
    // Along the right border every remaining sibling is on the left.
    for sibling in &proof[inner..] {
        res = node_hash(sibling, &res);
    }
    Ok(res)
}

/// Verify an inclusion proof against an expected root.
pub fn verify_inclusion(
    index: u64,
    size: u64,
    leaf: &[u8; 32],
    proof: &[[u8; 32]],
    expected_root: &[u8; 32],
) -> Result<()> {
    let computed = root_from_inclusion_proof(index, size, leaf, proof)?;
    if &computed != expected_root {
        return Err(Error::InclusionProofInvalid {
            computed: hex::encode(computed),
            expected: hex::encode(expected_root),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 6962's own worked example: SHA256 of the empty leaf.
    #[test]
    fn leaf_hash_matches_rfc6962() {
        assert_eq!(
            hex::encode(leaf_hash(b"")),
            "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d"
        );
    }

    /// A one-element tree's root is its only leaf hash, with an empty proof.
    #[test]
    fn a_single_leaf_tree_needs_no_proof() {
        let leaf = leaf_hash(b"hello");
        assert_eq!(root_from_inclusion_proof(0, 1, &leaf, &[]).unwrap(), leaf);
    }

    /// The domain separation is real: a leaf hash and a node hash of the same
    /// bytes must differ, or a leaf can be presented as an interior node.
    #[test]
    fn leaves_and_nodes_are_domain_separated() {
        let z = [0u8; 32];
        let mut concat = Vec::new();
        concat.extend_from_slice(&z);
        concat.extend_from_slice(&z);
        assert_ne!(leaf_hash(&concat), node_hash(&z, &z));
    }

    /// Two leaves: root = node(leaf0, leaf1), and each proves with the other.
    #[test]
    fn a_two_leaf_tree_verifies_both_ways() {
        let l0 = leaf_hash(b"a");
        let l1 = leaf_hash(b"b");
        let root = node_hash(&l0, &l1);
        verify_inclusion(0, 2, &l0, &[l1], &root).unwrap();
        verify_inclusion(1, 2, &l1, &[l0], &root).unwrap();
    }

    /// Swapping the sibling order must fail — otherwise the tree is unordered
    /// and any leaf proves any position.
    #[test]
    fn sibling_order_is_load_bearing() {
        let l0 = leaf_hash(b"a");
        let l1 = leaf_hash(b"b");
        let root = node_hash(&l0, &l1);
        // Claiming index 1 for leaf 0 hashes it on the wrong side.
        assert!(verify_inclusion(1, 2, &l0, &[l1], &root).is_err());
    }

    /// Build a full tree of `n` leaves the naive way and check every index.
    /// This is the property test the shape arithmetic has to survive.
    #[test]
    fn every_index_of_every_tree_up_to_33_verifies() {
        for n in 1u64..=33 {
            let leaves: Vec<[u8; 32]> = (0..n).map(|i| leaf_hash(&i.to_be_bytes())).collect();
            let root = reference_root(&leaves);
            for i in 0..n {
                let proof = reference_proof(&leaves, i);
                verify_inclusion(i, n, &leaves[i as usize], &proof, &root)
                    .unwrap_or_else(|e| panic!("n={n} i={i}: {e}"));
            }
        }
    }

    /// A proof of the right length but wrong content must not verify.
    #[test]
    fn a_tampered_sibling_is_refused_at_every_position() {
        let n = 17u64;
        let leaves: Vec<[u8; 32]> = (0..n).map(|i| leaf_hash(&i.to_be_bytes())).collect();
        let root = reference_root(&leaves);
        for i in 0..n {
            let mut proof = reference_proof(&leaves, i);
            for k in 0..proof.len() {
                let saved = proof[k];
                proof[k][0] ^= 0xff;
                assert!(
                    verify_inclusion(i, n, &leaves[i as usize], &proof, &root).is_err(),
                    "n={n} i={i} corrupting hash {k} still verified"
                );
                proof[k] = saved;
            }
        }
    }

    /// Out-of-range indices are their own refusal, not a root mismatch.
    #[test]
    fn an_index_outside_the_tree_is_named_as_such() {
        let leaf = leaf_hash(b"x");
        let e = verify_inclusion(5, 5, &leaf, &[], &leaf).unwrap_err();
        assert!(matches!(e, Error::InclusionIndexOutOfRange { .. }), "{e}");
        let e = verify_inclusion(0, 0, &leaf, &[], &leaf).unwrap_err();
        assert!(matches!(e, Error::InclusionIndexOutOfRange { .. }), "{e}");
    }

    /// A proof of the wrong length is refused for being the wrong length.
    #[test]
    fn a_proof_of_the_wrong_length_is_named_as_such() {
        let l0 = leaf_hash(b"a");
        let l1 = leaf_hash(b"b");
        let root = node_hash(&l0, &l1);
        let e = verify_inclusion(0, 2, &l0, &[], &root).unwrap_err();
        assert!(matches!(e, Error::InclusionProofShape { .. }), "{e}");
        let e = verify_inclusion(0, 2, &l0, &[l1, l1], &root).unwrap_err();
        assert!(matches!(e, Error::InclusionProofShape { .. }), "{e}");
    }

    /// `proof_shape` agrees with the reference builder for every tree we build.
    #[test]
    fn the_shape_arithmetic_agrees_with_a_real_tree() {
        for n in 1u64..=64 {
            let leaves: Vec<[u8; 32]> = (0..n).map(|i| leaf_hash(&i.to_be_bytes())).collect();
            for i in 0..n {
                let (inner, border) = proof_shape(i, n);
                assert_eq!(
                    inner + border,
                    reference_proof(&leaves, i).len(),
                    "n={n} i={i}"
                );
            }
        }
    }

    // --- a deliberately naive reference implementation ---------------------
    //
    // Written recursively from the RFC's definition rather than from the
    // optimised algorithm above, so a shared misreading cannot make both agree.

    fn reference_root(leaves: &[[u8; 32]]) -> [u8; 32] {
        match leaves.len() {
            0 => Sha256::digest([]).into(),
            1 => leaves[0],
            n => {
                let k = split(n);
                node_hash(&reference_root(&leaves[..k]), &reference_root(&leaves[k..]))
            }
        }
    }

    fn reference_proof(leaves: &[[u8; 32]], index: u64) -> Vec<[u8; 32]> {
        let n = leaves.len();
        if n <= 1 {
            return Vec::new();
        }
        let k = split(n) as u64;
        if index < k {
            let mut p = reference_proof(&leaves[..k as usize], index);
            p.push(reference_root(&leaves[k as usize..]));
            p
        } else {
            let mut p = reference_proof(&leaves[k as usize..], index - k);
            p.push(reference_root(&leaves[..k as usize]));
            p
        }
    }

    /// RFC 6962's split: the largest power of two strictly less than `n`.
    fn split(n: usize) -> usize {
        let mut k = 1;
        while k * 2 < n {
            k *= 2;
        }
        k
    }
}
