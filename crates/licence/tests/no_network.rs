//! The licence crate opens no socket, and cannot.
//!
//! The shape `crates/app-core/tests/no_network.rs` established: the manifest
//! names no network crate, so the compiler is the first control; no source
//! names `std::net`, which needs no dependency at all. This is the READ half;
//! the process-level half — the real binary under an OS network denial — is
//! `crates/cli/tests/licence_cli.rs`, and only that one counts as proof.

use std::path::PathBuf;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_manifest_names_no_network_crate() {
    let manifest = std::fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("Cargo.toml");
    let deps: String = manifest
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    for name in [
        "ureq",
        "reqwest",
        "hyper",
        "curl",
        "isahc",
        "attohttpc",
        "surf",
        "rustls",
        "native-tls",
        "openssl",
        "tokio",
        "async-std",
        "socket2",
        "mio",
    ] {
        assert!(
            !deps.contains(name),
            "crates/licence depends on `{name}`: a licence check that can phone home \
             breaks the claim that nothing leaves the machine"
        );
    }
    assert!(
        deps.contains("ed25519-dalek"),
        "the walk is reading a real manifest"
    );
}

#[test]
fn no_source_names_a_socket() {
    let mut seen = 0;
    for entry in std::fs::read_dir(manifest_dir().join("src"))
        .expect("src")
        .flatten()
    {
        let p = entry.path();
        if p.extension().is_none_or(|x| x != "rs") {
            continue;
        }
        seen += 1;
        let text = std::fs::read_to_string(&p).expect("read");
        for needle in [
            "std::net",
            "TcpStream",
            "UdpSocket",
            "TcpListener",
            "UnixStream",
        ] {
            assert!(!text.contains(needle), "{} names `{needle}`", p.display());
        }
    }
    assert!(seen >= 3, "the walk found {seen} source files");
}
