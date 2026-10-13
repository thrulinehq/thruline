//! The App's decisions cannot come from anywhere but their arguments.
//!
//! §9's App is a service, and a service is where "it only calls out in this one
//! place" stops being true quietly. The split — decisions here, sockets in a
//! separate service layer that is not part of this workspace — is only worth
//! anything while it holds, and the way it erodes is one convenient
//! `ureq::get` inside a function that already had the URL.
//!
//! Three halves, in the shape `crates/attest/tests/no_network.rs` established:
//! the manifest names no HTTP or TLS crate, so the *compiler* is the first
//! control; no source opens a socket, which `std::net` would allow with no
//! dependency at all; and no service shell is a workspace member, so adding one
//! there cannot pull a TLS stack into the engine's build.

use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn sources() -> Vec<(String, String)> {
    let dir = manifest_dir().join("src");
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("src/").flatten() {
        let p = entry.path();
        if p.extension().is_some_and(|x| x == "rs") {
            out.push((
                p.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string(),
                std::fs::read_to_string(&p).expect("read"),
            ));
        }
    }
    out.sort();
    assert!(out.len() >= 4, "the walk found {} files", out.len());
    out
}

/// The load-bearing one: with no dependency, a `ureq::` path does not compile.
#[test]
fn the_manifest_names_no_http_or_tls_crate() {
    let manifest = std::fs::read_to_string(manifest_dir().join("Cargo.toml")).expect("Cargo.toml");
    for crate_name in [
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
        "octocrab",
        "jsonwebtoken",
    ] {
        assert!(
            !manifest.contains(crate_name),
            "crates/app-core depends on `{crate_name}`. The App's decisions are pure \
             functions of bytes that arrived from somewhere else; the sockets live in a \
             separate service layer, outside this workspace, so that a TLS stack \
             cannot reach the engine's build."
        );
    }
    // Positive control: the walk is reading a real manifest.
    assert!(manifest.contains("thruline-policy"), "{manifest}");
}

/// `std::net` needs no dependency, so the manifest walk cannot catch it.
#[test]
fn no_source_file_opens_a_socket() {
    for (name, text) in sources() {
        let running: String = text
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        for socket in [
            "std::net",
            "TcpStream",
            "TcpListener",
            "UdpSocket",
            "UnixStream",
            "ToSocketAddrs",
        ] {
            assert!(
                !running.contains(socket),
                "crates/app-core/src/{name} opens a socket (`{socket}`)"
            );
        }
    }
}

/// And no service shell is a workspace member, so its dependencies are not the
/// engine's.
#[test]
fn the_service_shell_is_not_a_workspace_member() {
    let root = manifest_dir()
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf();
    let ws = std::fs::read_to_string(root.join("Cargo.toml")).expect("workspace manifest");
    let members = ws
        .lines()
        .find(|l| l.starts_with("members = "))
        .expect("members is on one line, as the file says it must be");
    // Positive control: this is the real members line, so the check below is
    // not passing on an empty or misread one.
    assert!(
        members.contains("crates/app-core"),
        "the members line does not name this crate: {members}"
    );
    assert!(
        !members.contains("services/"),
        "a service shell became a workspace member, which puts its HTTP and TLS \
         dependencies into every engine build: {members}"
    );
}
