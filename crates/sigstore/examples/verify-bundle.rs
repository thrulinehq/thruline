//! Verify a bundle from the command line, with nothing but files.
//!
//! ```text
//! verify-bundle <bundle.json> --trust-root <trusted_root.json>   # keyless
//! verify-bundle <bundle.json> --public-key <cosign.pub>          # local key
//! ```
//!
//! Exists so the fresh-machine proof can verify a bundle **produced on a
//! different machine** without that machine's code, keys, caches or network —
//! only the artifact and the trust material handed to it.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let bundle_path = PathBuf::from(
        args.next()
            .ok_or("usage: verify-bundle <bundle> --trust-root <f> | --public-key <f>")?,
    );
    let mode = args.next().ok_or("expected --trust-root or --public-key")?;
    let material = PathBuf::from(args.next().ok_or("expected a path after the flag")?);

    let bundle = thruline_sigstore::Bundle::from_json(&std::fs::read_to_string(&bundle_path)?)?;

    match mode.as_str() {
        "--trust-root" => {
            let root = thruline_sigstore::TrustRoot::from_file(&material)?;
            let id = thruline_sigstore::verify(&bundle, &thruline_sigstore::Policy::new(root))?;
            println!("verified: {} ({}) via {}", id.san, id.san_kind, id.issuer);
            if let Some(i) = id.log_index {
                println!("  rekor index {i}");
            }
        }
        "--public-key" => {
            // A public-key bundle proves possession of a key and nothing about
            // who holds it, so this prints what it actually established rather
            // than an identity it cannot substantiate.
            let pem = std::fs::read_to_string(&material)?;
            let spki = pem_to_der(&pem)?;
            let key_id = thruline_sigstore::verify_with_public_key(&bundle, &spki)?;
            println!(
                "verified: the payload was signed by the holder of {}",
                material.display()
            );
            // Ruling 8: a gate reaching `success` on a policy-listed key has to
            // be able to say WHICH listed key. Naming it is that half.
            println!("  key    {key_id}");
            println!("  (no certificate, so no identity, no issuer, no transparency log)");
        }
        other => return Err(format!("unknown flag {other:?}").into()),
    }
    Ok(())
}

fn pem_to_der(pem: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    use base64::Engine as _;
    let body: String = pem
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect::<Vec<_>>()
        .join("");
    Ok(base64::engine::general_purpose::STANDARD.decode(body.trim())?)
}
