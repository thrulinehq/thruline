//! The cloud backend has no success path, and that is the assertion.
//!
//! §18 leaves the provider undecided. The failure this test guards against is
//! the ordinary one: someone builds a placeholder that returns a plausible
//! `ExecResult`, a claim reaches `supported` on evidence nobody obtained, and
//! the only thing standing between that and a receipt was a comment.

#![cfg(feature = "reexec-cloud")]

use thruline_reexec::cloud::Cloud;
use thruline_reexec::Sandbox;

#[test]
fn the_cloud_backend_cannot_produce_a_result() {
    let mut c = Cloud::new(Some("https://example.invalid".to_string()));
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/src/cloud.rs"))
        .expect("cloud.rs");
    // `prepare` and `run` return only errors. Checked by reading the source as
    // well as by calling it, because a future `Ok(...)` added under a condition
    // this test does not reach would still be a success path.
    assert!(
        !src.contains("Ok(Prepared") && !src.contains("Ok(ExecResult"),
        "the cloud backend has gained a success path while §18's provider \
         decision is still open"
    );
    assert!(c.destroy().is_ok(), "there is nothing to destroy");
    // And the refusal says which decision is open, so a reader is not left
    // guessing whether this is a bug.
    let req = thruline_reexec::PrepareRequest {
        repo: std::path::PathBuf::from("."),
        tree_hash: "0".repeat(40),
        image: thruline_reexec::image::ImageSpec {
            reference: "scratch".into(),
            source: thruline_reexec::image::ImageSource::EcosystemDefault,
            build_context: None,
            deps: None,
        },
        network: false,
    };
    let e = c.prepare(&req).expect_err("prepare must refuse");
    let msg = e.to_string();
    assert!(msg.contains("§18"), "{msg}");
    assert!(msg.contains("unverifiable"), "{msg}");
}
