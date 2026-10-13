//! The issuance crate opens no socket, and cannot: it is a workspace member so
//! the gate tests it, and the HTTP shell that does open one is
//! `tools/licence-issuer`, excluded from the workspace.

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
        "tiny_http",
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
            "crates/licence-issue depends on `{name}`; the socket belongs in tools/licence-issuer"
        );
    }
    assert!(deps.contains("hmac"), "the walk is reading a real manifest");
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
    assert!(seen >= 6, "the walk found {seen} source files");
}

#[test]
fn the_workspace_excludes_the_http_shell_and_includes_this_crate() {
    let root = std::fs::read_to_string(manifest_dir().join("../../Cargo.toml")).expect("root");
    let members = root
        .lines()
        .find(|l| l.starts_with("members = ["))
        .expect("members on one line");
    let exclude = root
        .lines()
        .find(|l| l.starts_with("exclude = ["))
        .expect("exclude on one line");
    assert!(members.contains("\"crates/licence-issue\""), "{members}");
    assert!(!members.contains("tools/licence-issuer"), "{members}");
    assert!(exclude.contains("\"tools/licence-issuer\""), "{exclude}");
}
