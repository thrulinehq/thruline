use super::*;

/// The corpus-wide equivalence gate lives in
/// `crates/capture/tests/streaming_redaction_gate.rs`; these are the unit-level
/// properties of the boundary rule itself.

#[test]
fn a_block_never_ends_inside_an_unterminated_key_region() {
    let sink = CaptureSink::new(Redactor::new(), None, "t");
    // Two clean lines, then a BEGIN with no END: the cut must land after the
    // second line and never inside the region.
    let text = "one\ntwo\n-----BEGIN KEY-----\nbody\n";
    assert_eq!(sink.safe_cut(text), Some("one\ntwo\n".len()));
    // Once the region closes, cutting after it is allowed again.
    let closed = "one\n-----BEGIN KEY-----\nbody\n-----END KEY-----\n";
    assert_eq!(sink.safe_cut(closed), Some(closed.len()));
    // No newline at all: nothing is safe to cut.
    assert_eq!(sink.safe_cut("no newline here"), None);
}

#[test]
fn an_empty_stream_hashes_to_the_digest_of_nothing_rather_than_to_a_placeholder() {
    let captured = CaptureSink::new(Redactor::new(), None, "t").finish();
    assert_eq!(
        captured.hash,
        format!("sha256:{}", thruline_core::chain::sha256_hex(b""))
    );
    assert_eq!(captured.bytes_raw, 0);
    assert!(captured.excerpt.is_empty());
    assert!(!captured.block_cap_hit);
}

#[test]
fn the_excerpt_is_bounded_while_the_digest_covers_everything() {
    // §3.1's "bounded excerpt" and "output_hash" are different obligations, and
    // conflating them is how an event either becomes unreadable or stops
    // naming what actually ran.
    let dir = tempfile::tempdir().expect("tempdir");
    let mut sink = CaptureSink::new(Redactor::new(), Some(dir.path()), "t");
    let line = "x".repeat(1_000);
    for _ in 0..50 {
        sink.push(format!("{line}\n").as_bytes());
    }
    let captured = sink.finish();
    assert!(
        captured.excerpt.len() <= EXCERPT_BOUND + 64,
        "excerpt is {} bytes",
        captured.excerpt.len()
    );
    assert_eq!(captured.bytes_raw, 50 * 1_001);
    let stored = captured
        .stored_path
        .as_ref()
        .map(|p| std::fs::read(p).expect("read"))
        .expect("stored");
    assert_eq!(stored.len(), 50 * 1_001);
    assert_eq!(
        captured.hash,
        format!("sha256:{}", thruline_core::chain::sha256_hex(&stored))
    );
}

#[test]
fn two_identical_streams_share_one_content_addressed_file() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut paths = Vec::new();
    for _ in 0..2 {
        let mut sink = CaptureSink::new(Redactor::new(), Some(dir.path()), "t");
        sink.push(b"2 passed in 0.01s\n");
        paths.push(sink.finish().stored_path.expect("stored"));
    }
    assert_eq!(paths[0], paths[1]);
    let files: Vec<_> = std::fs::read_dir(dir.path())
        .expect("read dir")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(
        files.len(),
        1,
        "a duplicate output was stored twice: {files:?}"
    );
}

#[test]
fn the_hex_helper_names_the_file_the_digest_names() {
    let mut sink = CaptureSink::new(Redactor::new(), None, "t");
    sink.push(b"x");
    let captured = sink.finish();
    assert_eq!(captured.hex(), thruline_core::chain::sha256_hex(b"x"));
    assert_eq!(captured.hash, format!("sha256:{}", captured.hex()));
}
