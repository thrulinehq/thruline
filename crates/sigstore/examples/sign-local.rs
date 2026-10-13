//! Sign a payload with a real `cosign.key` and write the bundle out.
//!
//! ```text
//! sign-local <keydir> <out-bundle.json>
//! ```
//!
//! Generates the key pair on first use (private half `0600`, refusing to
//! overwrite). Produces a v0.3 bundle whose verification material is the bare
//! public key — which proves possession of that key and nothing about who holds
//! it, which is the honest shape of offline signing.

use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: sign-local <keydir> <out>")?,
    );
    let out = PathBuf::from(
        std::env::args()
            .nth(2)
            .ok_or("usage: sign-local <keydir> <out>")?,
    );
    std::fs::create_dir_all(&dir)?;

    let priv_path = dir.join("cosign.key");
    let pub_path = dir.join("cosign.pub");
    let pass = std::env::var("COSIGN_PASSWORD").unwrap_or_else(|_| "thruline".to_string());

    if !priv_path.exists() {
        thruline_sigstore::key::generate(&priv_path, &pub_path, pass.as_bytes())?;
        eprintln!(
            "generated {} and {}",
            priv_path.display(),
            pub_path.display()
        );
    }

    // Built from the crate's own constants, not a literal: the predicate domain
    // moved once already (core `290845c`) and a hardcoded copy is how a receipt
    // ends up naming a predicate no verifier keys on.
    let statement = format!(
        r#"{{"_type":"{}","subject":[{{"name":"signed-on-this-machine","digest":{{"sha256":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"}}}}],"predicateType":"{}","predicate":{{}}}}"#,
        thruline_sigstore::STATEMENT_TYPE,
        thruline_sigstore::PREDICATE_TYPE
    );
    let payload = statement.as_bytes();

    let bundle = thruline_sigstore::sign(
        payload,
        thruline_sigstore::OidcProvider::LocalKey {
            path: priv_path,
            passphrase: pass.into_bytes(),
        },
    )?;
    std::fs::write(&out, bundle.to_json()?)?;
    eprintln!("wrote {}", out.display());
    Ok(())
}
