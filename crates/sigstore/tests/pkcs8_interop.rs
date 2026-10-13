//! The PKCS#8 export, judged by parsers that are not this crate.
//!
//! # Why this test exists at all
//!
//! `~/thruline/crates/attest/src/key.rs:70` records a **measured** failure, not
//! a hypothetical one: `ed25519-dalek`'s `to_pkcs8_der` emits RFC 5958
//! `OneAsymmetricKey` **v2** — 83 bytes, `version = 1`, public key in a `[1]`
//! attribute — and both OpenSSL 3 and Python `cryptography` refuse it for
//! Ed25519. `openssl pkey` answered
//! `ossl_store_handle_load_result:unsupported`; `load_pem_private_key` raised
//! `ValueError`. That crate shipped a key format nothing else could read, and
//! the round-trip test it *did* have passed the whole time, because it round-
//! tripped through the same library that produced the bytes.
//!
//! P-256 has no v0/v2 split in the same shape, and `p256` emits RFC 5208 v0.
//! But "emits v0" is a claim about a dependency's behaviour on a particular
//! version, and the whole lesson of that defect is that such a claim is worth
//! exactly what a foreign parser says about it. So this shells out.
//!
//! # A skip that asserts nothing is not a pass
//!
//! Each parser is skipped, loudly, if it genuinely is not installed. But if
//! **neither** could be run, the test fails: at that point it has checked that
//! this crate agrees with itself, which is the exact failure mode above.
//!
//! On the development machine both run. Which *build* of each runs is worth
//! stating, because the shell and `Command::new` do not agree: an interactive
//! zsh aliases `python3` to `/usr/local/bin/python3` (`cryptography` 49.0.0),
//! while the PATH a test process inherits resolves it to
//! `/opt/anaconda3/bin/python3` (`cryptography` 46.0.3). Both were run by hand
//! against this export and both accept it; the test prints the version it
//! actually used rather than the one someone measured in a terminal.
//! OpenSSL likewise: 3.0.18 at `/opt/anaconda3/bin/openssl`, and LibreSSL 3.3.6
//! at `/usr/bin/openssl` — both verified to accept the PEM, the DER, and to
//! derive the identical public key.

use std::path::Path;
use std::process::Command;

use thruline_sigstore::key;

/// OpenSSL candidates in preference order. The Homebrew/conda OpenSSL 3 is
/// preferred over macOS's `/usr/bin/openssl`, which is LibreSSL wearing the
/// name — a different implementation, and worth *more* as a second opinion, not
/// less, so it is the fallback rather than an exclusion.
const OPENSSL_CANDIDATES: [&str; 3] = ["/opt/anaconda3/bin/openssl", "/usr/bin/openssl", "openssl"];

fn find_openssl() -> Option<String> {
    for candidate in OPENSSL_CANDIDATES {
        if let Ok(out) = Command::new(candidate).arg("version").output() {
            if out.status.success() {
                let banner = String::from_utf8_lossy(&out.stdout).trim().to_string();
                eprintln!("pkcs8_interop: using {candidate} ({banner})");
                return Some(candidate.to_string());
            }
        }
    }
    eprintln!(
        "pkcs8_interop: SKIPPING the OpenSSL half — none of {OPENSSL_CANDIDATES:?} could be run"
    );
    None
}

fn find_python() -> Option<&'static str> {
    match Command::new("python3")
        .args(["-c", "import cryptography; print(cryptography.__version__)"])
        .output()
    {
        Ok(out) if out.status.success() => {
            eprintln!(
                "pkcs8_interop: using python3 cryptography {}",
                String::from_utf8_lossy(&out.stdout).trim()
            );
            Some("python3")
        }
        Ok(out) => {
            eprintln!(
                "pkcs8_interop: SKIPPING the Python half — `import cryptography` failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        Err(e) => {
            eprintln!("pkcs8_interop: SKIPPING the Python half — python3 could not be run: {e}");
            None
        }
    }
}

fn ok(cmd: &mut Command) -> String {
    let out = cmd.output().unwrap_or_else(|e| panic!("{cmd:?}: {e}"));
    assert!(
        out.status.success(),
        "{cmd:?} exited {}\nstdout: {}\nstderr: {}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A generated key, exported three ways, on disk.
struct Exported {
    dir: tempfile::TempDir,
    public_pem: String,
    der: Vec<u8>,
}

impl Exported {
    fn new() -> Exported {
        let dir = tempfile::tempdir().expect("tempdir");
        // Through the public door: `generate` is what a user actually runs, so
        // the key under test is one this crate really produces, not one a test
        // assembled by hand.
        let (key_path, pub_path) = (dir.path().join("cosign.key"), dir.path().join("cosign.pub"));
        key::generate(&key_path, &pub_path, b"interop").expect("generate");
        let signing = key::load_cosign_key(
            &std::fs::read_to_string(&key_path).expect("cosign.key"),
            b"interop",
        )
        .expect("load");

        let pem = key::pkcs8_pem(&signing).expect("pkcs8 pem");
        let der = key::pkcs8_der(&signing).expect("pkcs8 der");
        std::fs::write(dir.path().join("key.pem"), &pem).expect("key.pem");
        std::fs::write(dir.path().join("key.der"), &der).expect("key.der");
        Exported {
            public_pem: std::fs::read_to_string(&pub_path).expect("cosign.pub"),
            der,
            dir,
        }
    }

    fn pem_path(&self) -> std::path::PathBuf {
        self.dir.path().join("key.pem")
    }
    fn der_path(&self) -> std::path::PathBuf {
        self.dir.path().join("key.der")
    }
}

fn path(p: &Path) -> String {
    p.to_str().expect("utf-8 path").to_string()
}

#[test]
fn the_pkcs8_export_is_read_by_parsers_that_are_not_this_crate() {
    let exported = Exported::new();
    let mut parsers_that_ran = Vec::new();

    if let Some(openssl) = find_openssl() {
        // PEM: parses at all.
        ok(Command::new(&openssl).args(["pkey", "-in", &path(&exported.pem_path()), "-noout"]));
        // DER: the same bytes, unarmoured. `-inform DER` is the form the
        // Ed25519 v2 document failed on.
        ok(Command::new(&openssl).args([
            "pkey",
            "-inform",
            "DER",
            "-in",
            &path(&exported.der_path()),
            "-noout",
        ]));
        // And it is the *right* key: OpenSSL's own idea of the public half has
        // to equal the `cosign.pub` this crate wrote. Parsing without this is
        // only a check that the bytes are well-formed ASN.1.
        let derived = ok(Command::new(&openssl).args([
            "pkey",
            "-in",
            &path(&exported.pem_path()),
            "-pubout",
        ]));
        assert_eq!(
            derived, exported.public_pem,
            "OpenSSL derived a different public key than this crate wrote to cosign.pub"
        );
        parsers_that_ran.push("openssl");
    }

    if let Some(python) = find_python() {
        let program = r#"
import sys
from cryptography.hazmat.primitives import serialization

pem_path, der_path = sys.argv[1], sys.argv[2]
with open(pem_path, "rb") as f:
    from_pem = serialization.load_pem_private_key(f.read(), password=None)
with open(der_path, "rb") as f:
    from_der = serialization.load_der_private_key(f.read(), password=None)

spki = lambda k: k.public_key().public_bytes(
    encoding=serialization.Encoding.PEM,
    format=serialization.PublicFormat.SubjectPublicKeyInfo,
).decode()

assert spki(from_pem) == spki(from_der), "PEM and DER are different keys"
sys.stdout.write(spki(from_pem))
"#;
        let derived = ok(Command::new(python).args([
            "-c",
            program,
            &path(&exported.pem_path()),
            &path(&exported.der_path()),
        ]));
        assert_eq!(
            derived, exported.public_pem,
            "python cryptography derived a different public key than this crate wrote"
        );
        parsers_that_ran.push("python cryptography");
    }

    assert!(
        !parsers_that_ran.is_empty(),
        "neither OpenSSL nor python cryptography could be run, so this test checked only that \
         this crate agrees with itself — which is exactly the shape of the round-trip test that \
         let crates/attest ship an unreadable Ed25519 key. Failing rather than reporting a pass."
    );
    eprintln!("pkcs8_interop: verified by {}", parsers_that_ran.join(", "));
}

/// RFC 5208 `version` must be **0**.
///
/// This is the field that broke Ed25519 in `crates/attest`: `version = 1`
/// (RFC 5958 v2) is legal ASN.1 and unreadable to the two parsers above. The
/// assertion is on the bytes rather than on a library's opinion of them,
/// because the library's opinion is the thing under test.
#[test]
fn the_der_declares_rfc_5208_version_zero_not_rfc_5958_version_one() {
    let exported = Exported::new();
    let der = &exported.der;

    // 30 81 LL      SEQUENCE, one-byte long-form length
    //    02 01 00   INTEGER 0   <- version
    assert_eq!(der[0], 0x30, "PKCS#8 must start with a SEQUENCE");
    assert_eq!(
        der[1], 0x81,
        "expected a one-byte long-form length for a ~138-byte P-256 document, got {:#04x}",
        der[1]
    );
    assert_eq!(
        usize::from(der[2]) + 3,
        der.len(),
        "the declared SEQUENCE length does not match the document"
    );
    assert_eq!(
        &der[3..6],
        &[0x02, 0x01, 0x00],
        "version INTEGER is not 0 — this is RFC 5958 v2, the encoding OpenSSL 3 and python \
         cryptography reject"
    );
}
