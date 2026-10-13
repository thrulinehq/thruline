//! Sign a payload keyless against the Sigstore **staging** instance, then
//! verify the resulting bundle offline against the staging trust root.
//!
//! This is the round trip the crate exists for, end to end, on real
//! infrastructure:
//!
//!   device flow → Fulcio certificate → DSSE signature → Rekor entry → bundle
//!   → verify (chain, SCT, inclusion proof, SET, identity)
//!
//! Staging only. A Rekor entry is permanent and public, so production is never
//! the default; `OidcProvider::staging_device_flow()` names staging explicitly
//! and `net` warns if it is ever pointed at production.
//!
//! Run with:
//!   cargo run --features net,oidc --example staging-sign
//!
//! A human must approve the device code in a browser within five minutes.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "staging-bundle.sigstore.json".to_string()),
    );

    // A synthetic payload. Nothing about the machine or the repository goes
    // into a public transparency log.
    // Built from the crate's own constants, not a literal: the predicate domain
    // moved once already (core `290845c`) and a hardcoded copy is how a receipt
    // ends up naming a predicate no verifier keys on.
    let statement = format!(
        r#"{{"_type":"{}","subject":[{{"name":"thruline-sigstore-selftest","digest":{{"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}}}}],"predicateType":"{}","predicate":{{}}}}"#,
        thruline_sigstore::STATEMENT_TYPE,
        thruline_sigstore::PREDICATE_TYPE
    );
    let payload = statement.as_bytes();

    eprintln!("== signing against Sigstore STAGING ==");
    let bundle = thruline_sigstore::sign(
        payload,
        thruline_sigstore::OidcProvider::staging_device_flow(),
    )?;

    std::fs::write(&out, bundle.to_json()?)?;
    eprintln!("\nwrote {}", out.display());

    // Now verify what we just produced, offline, against the staging trust
    // root. Signing and verifying with the same code proves less than either
    // does alone — but a bundle that this crate cannot verify is a bundle it
    // should not have produced, and the fixtures let a third party check it.
    let root = thruline_sigstore::TrustRoot::from_json(include_str!(
        "../fixtures/trustroot/staging-trusted_root.json"
    ))?;
    let identity = thruline_sigstore::verify(&bundle, &thruline_sigstore::Policy::new(root))?;

    eprintln!("\n== verified ==");
    eprintln!("  identity      : {}", identity.san);
    eprintln!("  form          : {}", identity.san_kind);
    eprintln!("  issuer        : {}", identity.issuer);
    eprintln!(
        "  rekor index   : {}",
        identity
            .log_index
            .map(|i| i.to_string())
            .unwrap_or_else(|| "-".into())
    );
    eprintln!(
        "  logged at     : {}",
        identity
            .integrated_time
            .map(|t| t.to_string())
            .unwrap_or_else(|| "-".into())
    );
    eprintln!(
        "  cert validity : {} .. {}  ({} minutes)",
        identity.not_before,
        identity.not_after,
        (identity.not_after - identity.not_before).whole_minutes()
    );
    Ok(())
}
