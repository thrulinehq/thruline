//! The predicate domain is written into every receipt ever issued, and a
//! verifier keys on it. A stray occurrence of the retired domain in any tracked
//! file is a receipt that will not verify — so this census fails on one.
//!
//! The retired domain is assembled from parts rather than written out, so this
//! file does not match its own search. A census that matches itself either
//! reports a permanent false positive or gets an exception that hides the real
//! thing — this program has one census that passed 6/6 after 135 lines of the
//! guard it watched were deleted, and the lesson is that the search space must
//! be the one that matters, not the one that is convenient.

use std::process::Command;

fn retired_domain() -> String {
    // Deliberately not a literal: see the module comment.
    format!("{}.{}", "thruline", "dev")
}

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("repo root")
        .to_path_buf()
}

#[test]
fn no_tracked_file_carries_the_retired_predicate_domain() {
    let root = repo_root();
    let out = Command::new("git")
        .current_dir(&root)
        .args(["grep", "-lI", "-e", &retired_domain()])
        .output()
        .expect("git grep");

    // `git grep` exits 1 with no output when nothing matches. That is the pass.
    let hits = String::from_utf8_lossy(&out.stdout);
    let files: Vec<&str> = hits.lines().filter(|l| !l.is_empty()).collect();

    assert!(
        files.is_empty(),
        "the retired predicate domain survives in {} tracked file(s): {}.\n\
         It is written into every receipt and verifiers key on it, so a stray \
         occurrence is a receipt that will not verify. Replace it with \
         thrulinehq.com.",
        files.len(),
        files.join(", ")
    );
}

#[test]
fn the_predicate_type_and_schema_id_agree_on_the_domain() {
    // The two were ruled together and must move together. Reading them from the
    // crate rather than restating them: a test that restates the constant it
    // guards proves only that someone typed it twice.
    let pt = thruline_attest::statement::PREDICATE_TYPE;
    let sid = thruline_attest::statement::SCHEMA_ID;
    let host = "https://thrulinehq.com/";
    assert!(
        pt.starts_with(host),
        "predicate type is not on the ruled domain: {pt}"
    );
    assert!(
        sid.starts_with(host),
        "schema id is not on the ruled domain: {sid}"
    );
    assert!(
        pt.contains("/attestation/") && sid.contains("/spec/"),
        "the two paths were swapped: predicate={pt} schema={sid}"
    );
}

/// The plaintext census above cannot see a receipt, because a receipt is a DSSE
/// envelope and the predicate type lives inside a **base64** payload. D4
/// recorded "no fixture receipt embeds the predicate domain" — true when
/// written, false as soon as `receipt-real.json` landed, and the census
/// reported clean throughout because it was searching the wrong encoding.
///
/// A guard that searches a space the guarded value is not stored in is this
/// program's most-repeated defect. This one decodes.
#[test]
fn no_committed_receipt_carries_the_retired_domain_inside_its_payload() {
    use base64::Engine as _;

    let root = repo_root();
    let out = Command::new("git")
        .current_dir(&root)
        .args(["ls-files", "-z", "*.json"])
        .output()
        .expect("git ls-files");
    let files = String::from_utf8_lossy(&out.stdout);

    let retired = retired_domain();
    let mut checked = 0usize;
    let mut offenders = Vec::new();

    for rel in files.split('\0').filter(|s| !s.is_empty()) {
        let Ok(text) = std::fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let Ok(doc) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        // A DSSE envelope: {payload, payloadType, signatures}.
        let Some(payload) = doc.get("payload").and_then(|p| p.as_str()) else {
            continue;
        };
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(payload) else {
            continue;
        };
        let Ok(decoded) = String::from_utf8(bytes) else {
            continue;
        };
        checked += 1;
        if decoded.contains(&retired) {
            offenders.push(rel.to_string());
        }
    }

    assert!(
        checked > 0,
        "this census decoded ZERO receipt payloads, so it proves nothing. It is \
         meant to guard committed DSSE fixtures; if none is tracked any more, \
         delete this test rather than letting it pass on an empty search."
    );
    assert!(
        offenders.is_empty(),
        "{} committed receipt payload(s) still carry the retired domain once \
         base64-decoded: {}. Regenerate them; do not hand-edit a signed envelope.",
        offenders.len(),
        offenders.join(", ")
    );
}
