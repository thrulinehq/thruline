//! The exec path's redaction is held to the same corpus as the hook path's.
//!
//! `redaction_corpus_gate.rs` runs the 1,958-row corpus through
//! [`Redactor::redact`] in one piece. The exec recorder cannot do that: §4.4
//! captures a stream that may be 100 MB, so it redacts in blocks
//! (`capture::stream`). **A block boundary in the wrong place unmasks a
//! secret** — the detectors work on shapes and half a shape is not one — and
//! that is the same generator as A209-C3, where a truncation cut a token below
//! its detector's threshold.
//!
//! So the gate here is an **equivalence**, which is stronger than a second list
//! of hand-written cases: for every row of the corpus, at adversarial chunk and
//! block sizes, the streamed result must be byte-identical to the whole-text
//! result. If the streaming path ever masks less than the batch path, this
//! fails — without anyone having to think of the specific secret.

use std::path::PathBuf;

use thruline_capture::redaction::Redactor;
use thruline_capture::stream::CaptureSink;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crates/")
        .join("core/tests/fixtures/redaction-corpus")
}

fn corpus() -> Vec<(String, String)> {
    let path = fixture_dir().join("corpus.jsonl");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let v: serde_json::Value = serde_json::from_str(l).expect("corpus row is JSON");
            (
                v["family"].as_str().unwrap_or("?").to_string(),
                v["text"].as_str().expect("text").to_string(),
            )
        })
        .collect()
}

/// Run text through the sink at a given chunk and block size, returning the
/// stored bytes.
fn streamed(text: &str, chunk: usize, block: usize, dir: &std::path::Path) -> (String, String) {
    let mut sink = CaptureSink::new(Redactor::new(), Some(dir), "t").with_block_size(block);
    let bytes = text.as_bytes();
    let mut at = 0;
    while at < bytes.len() {
        let end = (at + chunk).min(bytes.len());
        sink.push(&bytes[at..end]);
        at = end;
    }
    let captured = sink.finish();
    let stored = captured
        .stored_path
        .as_ref()
        .map(|p| std::fs::read_to_string(p).expect("read stored"))
        .unwrap_or_default();
    (stored, captured.hash)
}

#[test]
fn every_corpus_row_streams_to_exactly_what_batch_redaction_produces() {
    let dir = tempfile::tempdir().expect("tempdir");
    let redactor = Redactor::new();
    let rows = corpus();
    assert!(
        rows.len() > 1_000,
        "the corpus did not load: {}",
        rows.len()
    );

    let mut checked = 0usize;
    let mut differed: Vec<String> = Vec::new();
    for (family, text) in &rows {
        let batch = redactor.redact(text).into_text();
        // Chunk 1 byte with block 1 byte is the worst case the sink can face:
        // it flushes at every line the row contains.
        for (chunk, block) in [(1usize, 1usize), (7, 3), (13, 64), (4096, 256 * 1024)] {
            let (stored, hash) = streamed(text, chunk, block, dir.path());
            checked += 1;
            if stored != batch {
                differed.push(format!(
                    "{family} at chunk {chunk}/block {block}: streamed {} bytes vs batch {}",
                    stored.len(),
                    batch.len()
                ));
                break;
            }
            // And the digest is over what was stored, not over something else.
            assert_eq!(
                hash,
                format!(
                    "sha256:{}",
                    thruline_core::chain::sha256_hex(stored.as_bytes())
                ),
                "{family}: the digest does not cover the stored bytes"
            );
        }
    }
    assert!(checked > 4_000, "the gate ran {checked} comparisons");
    assert!(
        differed.is_empty(),
        "{} of {} corpus rows redact differently when streamed. Block \
         boundaries are unmasking content:\n{}",
        differed.len(),
        rows.len(),
        differed
            .iter()
            .take(20)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn a_pem_block_split_across_chunks_is_still_masked() {
    // The one legitimately multi-line secret format, and the reason the sink
    // never cuts inside `-----BEGIN … -----END`.
    let body: String = (0..8)
        .map(|i| format!("MIIEow{i}IBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCd\n"))
        .collect();
    let pem = format!("-----BEGIN RSA PRIVATE KEY-----\n{body}-----END RSA PRIVATE KEY-----\n");
    let dir = tempfile::tempdir().expect("tempdir");
    let batch = Redactor::new().redact(&pem).into_text();
    assert!(
        !batch.contains("MIIEow0IBAAKCAQEA"),
        "the batch redactor does not mask this shape, so the test proves nothing"
    );
    for chunk in [1usize, 5, 37, 1024] {
        let (stored, _) = streamed(&pem, chunk, 1, dir.path());
        assert!(
            !stored.contains("MIIEow0IBAAKCAQEA"),
            "a key body survived streaming at chunk {chunk}"
        );
        assert_eq!(stored, batch, "chunk {chunk} redacted differently");
    }
}

#[test]
fn the_digest_does_not_depend_on_how_the_pipe_delivered_the_bytes() {
    // If it did, the same command run twice would produce different evidence,
    // and a receipt comparing digests across runs would be comparing noise.
    let dir = tempfile::tempdir().expect("tempdir");
    let text = "line one\nline two with a ghp_16C7e42F292c6912E7710c838347Ae178B4a token\n\
                line three with an é and a 𝔘 astral char\n"
        .repeat(50);
    let mut hashes = std::collections::BTreeSet::new();
    for chunk in [1usize, 2, 3, 17, 999, 65536] {
        let (_, hash) = streamed(&text, chunk, 1, dir.path());
        hashes.insert(hash);
    }
    assert_eq!(hashes.len(), 1, "the digest changed with the chunk size");
}

#[test]
fn a_multibyte_character_split_across_reads_is_not_mangled() {
    // A 4-byte astral character delivered one byte at a time. Without the
    // incremental decoder each byte becomes U+FFFD and the stored text — and
    // therefore the digest — depends on pipe timing.
    let dir = tempfile::tempdir().expect("tempdir");
    let text = "prefix 𝔘 suffix\n";
    let (stored, _) = streamed(text, 1, 1, dir.path());
    assert_eq!(stored, text);
    let captured = {
        let mut sink = CaptureSink::new(Redactor::new(), None, "t");
        for b in text.as_bytes() {
            sink.push(&[*b]);
        }
        sink.finish()
    };
    assert!(
        !captured.invalid_utf8,
        "a correctly split character was reported as invalid UTF-8"
    );
}

#[test]
fn output_that_is_not_utf8_is_flagged_rather_than_silently_replaced() {
    let mut sink = CaptureSink::new(Redactor::new(), None, "t");
    sink.push(&[0xff, 0xfe, b'o', b'k', b'\n']);
    let captured = sink.finish();
    assert!(
        captured.invalid_utf8,
        "invalid bytes were replaced with no record that they had been"
    );
    assert!(captured.bytes_raw > 0);
}

#[test]
fn a_line_that_never_ends_hits_the_cap_and_says_so() {
    // The bound that keeps a `-----BEGIN` with no `-----END` from forcing
    // unbounded buffering. It gives up the boundary guarantee, so it must be
    // recorded.
    let mut sink = CaptureSink::new(Redactor::new(), None, "t").with_block_size(64);
    let huge = "x".repeat(thruline_capture::stream::MAX_BLOCK + 1024);
    sink.push(huge.as_bytes());
    let captured = sink.finish();
    assert!(
        captured.block_cap_hit,
        "the cap fired silently; a mid-line boundary must be reported"
    );
}

#[test]
fn a_store_directory_that_cannot_be_used_costs_the_file_and_not_the_digest() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A file where a directory is required.
    let blocked = dir.path().join("not-a-dir");
    std::fs::write(&blocked, b"x").expect("write");
    let mut sink = CaptureSink::new(Redactor::new(), Some(&blocked), "t");
    sink.push(b"2 passed\n");
    let captured = sink.finish();
    assert!(captured.stored_path.is_none());
    assert!(
        captured.store_error.is_some(),
        "the store failure vanished instead of being counted"
    );
    assert_eq!(
        captured.hash,
        format!("sha256:{}", thruline_core::chain::sha256_hex(b"2 passed\n")),
        "losing the file also lost the digest"
    );
}

#[test]
fn the_boundary_rule_is_load_bearing_and_not_decoration() {
    // Negative control for the gate above. If redacting each line
    // independently gave the same answer as redacting the whole block, the
    // BEGIN/END deferral would be proving nothing and the equivalence test
    // would pass for the wrong reason.
    let body: String = (0..8)
        .map(|i| format!("MIIEow{i}IBAAKCAQEA7Q9Xn2mKfWvGqRt3sYbLpZcNdEhJkMnOpQrStUvWxYzAbCd\n"))
        .collect();
    let pem = format!("-----BEGIN RSA PRIVATE KEY-----\n{body}-----END RSA PRIVATE KEY-----\n");
    let redactor = Redactor::new();

    let whole = redactor.redact(&pem).into_text();
    let per_line: String = pem
        .split_inclusive('\n')
        .map(|line| redactor.redact(line).into_text())
        .collect();

    assert_ne!(
        whole, per_line,
        "cutting a key block at every newline produces the same text as not \
         cutting it, so the sink's deferral rule is untested by the corpus gate"
    );
}
