//! No request may panic a handler (C38 §3).
//!
//! The router matched `starts_with("/api/changes/") && ends_with("/decisions")`
//! and then sliced `p["/api/changes/".len()..p.len() - "/decisions".len()]`.
//! For `/api/changes/decisions` the prefix and the suffix overlap, the slice is
//! `[13..12]`, and the handler panicked instead of refusing. Any request may be
//! refused (a 4xx); none may take a handler down.
//!
//! This sends every degenerate shape of every route in the router's source —
//! each `ends_with` / `strip_suffix` suffix and each `starts_with` /
//! `strip_prefix` prefix it names, found by reading the source, so a route added
//! later is covered without anyone remembering to list it — to a real
//! `thruline ui` over GET, POST, PUT and DELETE, and requires an answer every
//! time and a server that still answers afterwards. A second test pins that
//! the fixed-offset slicing pattern is gone from the router, so it cannot come
//! back without this suite saying so.
//!
//! A real store under a temporary directory; no real store is opened.

use std::collections::BTreeSet;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use thruline_core::ledger::Ledger;

const ROUTER_SOURCES: &[(&str, &str)] = &[
    ("uiserve.rs", include_str!("../src/uiserve.rs")),
    ("uiserve/hq.rs", include_str!("../src/uiserve/hq.rs")),
    (
        "uiserve/run_route.rs",
        include_str!("../src/uiserve/run_route.rs"),
    ),
    (
        "uiserve/install_route.rs",
        include_str!("../src/uiserve/install_route.rs"),
    ),
    (
        "uiserve/first_use.rs",
        include_str!("../src/uiserve/first_use.rs"),
    ),
    (
        "uiserve/decisions.rs",
        include_str!("../src/uiserve/decisions.rs"),
    ),
    (
        "uiserve/capture_holds.rs",
        include_str!("../src/uiserve/capture_holds.rs"),
    ),
];

struct Server {
    child: Child,
    port: u16,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Server {
    fn start(home: &Path, db: &Path) -> Server {
        let mut child = Command::new(env!("CARGO_BIN_EXE_thruline"))
            .env("HOME", home)
            .env("THRULINE_HOME", home.join(".thruline"))
            .env_remove("THRULINE_PAID_GATE_DEV")
            .env_remove("THRULINE_LICENCE_DEV_NO_BUILT_IN_KEYS")
            .env_remove("THRULINE_LICENCE_DEV_PUBLIC_KEY")
            .current_dir(home)
            .arg("--db")
            .arg(db)
            .args(["ui", "--no-open", "--port", "0"])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("thruline ui");
        let stdout = child.stdout.take().expect("stdout");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { return };
                if tx.send(line).is_err() {
                    return;
                }
            }
        });
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(left) {
                Ok(line) => {
                    if let Some(rest) = line.trim().strip_prefix("ThruLine is at http://127.0.0.1:")
                    {
                        let port: u16 = rest
                            .trim()
                            .split('/')
                            .next()
                            .expect("port")
                            .parse()
                            .expect("port in the printed URL");
                        return Server { child, port };
                    }
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let mut err = String::new();
                    if let Some(mut e) = child.stderr.take() {
                        let _ = e.read_to_string(&mut err);
                    }
                    panic!("`thruline ui` never printed a loopback URL.\n{err}");
                }
            }
        }
    }

    /// The status line's code, or 0 when the connection closed with no answer
    /// (what a panicking handler leaves behind).
    fn status(&self, method: &str, path: &str) -> u16 {
        let Ok(mut socket) = TcpStream::connect(("127.0.0.1", self.port)) else {
            return 0;
        };
        let _ = socket.set_read_timeout(Some(Duration::from_secs(30)));
        let body = if method == "POST" || method == "PUT" {
            "{}"
        } else {
            ""
        };
        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nOrigin: http://127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            self.port,
            self.port,
            body.len()
        );
        let _ = socket.write_all(request.as_bytes());
        let mut raw = Vec::new();
        let _ = socket.read_to_end(&mut raw);
        String::from_utf8_lossy(&raw)
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(0)
    }
}

/// Every string literal passed to `needle(` in `source`.
fn literals(source: &str, needle: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = source;
    while let Some(at) = rest.find(needle) {
        let after = &rest[at + needle.len()..];
        if let Some(inner) = after.strip_prefix('"') {
            if let Some(end) = inner.find('"') {
                out.insert(inner[..end].to_string());
            }
        }
        rest = after;
    }
    out
}

fn route_parts() -> (BTreeSet<String>, BTreeSet<String>) {
    let mut prefixes = BTreeSet::new();
    let mut suffixes = BTreeSet::new();
    for (_, source) in ROUTER_SOURCES {
        for needle in ["starts_with(", "strip_prefix("] {
            prefixes.extend(literals(source, needle));
        }
        for needle in ["ends_with(", "strip_suffix("] {
            suffixes.extend(literals(source, needle));
        }
    }
    prefixes.retain(|p| p.starts_with("/api/"));
    suffixes.retain(|s| s.starts_with('/') && s.len() > 1);
    (prefixes, suffixes)
}

#[test]
fn every_degenerate_path_is_answered_and_none_takes_a_handler_down() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".thruline")).expect("home");
    let db = home.join(".thruline").join("ledger.db");
    drop(Ledger::open(&db).expect("open"));
    let server = Server::start(&home, &db);

    let (prefixes, suffixes) = route_parts();
    assert!(
        prefixes.contains("/api/changes/") && suffixes.contains("/decisions"),
        "the scan must see the routes this test exists for: {prefixes:?} {suffixes:?}"
    );

    let mut paths: BTreeSet<String> = BTreeSet::new();
    for prefix in &prefixes {
        let bare = prefix.trim_end_matches('/');
        paths.insert(prefix.clone());
        paths.insert(bare.to_string());
        for suffix in &suffixes {
            // The prefix and the suffix overlap, with nothing between them.
            paths.insert(format!("{prefix}{}", suffix.trim_start_matches('/')));
            paths.insert(format!("{bare}{suffix}"));
            // An empty id between them.
            paths.insert(format!("{prefix}{}", suffix));
            // A normal-looking id, a doubled slash, and a lone slash after.
            paths.insert(format!("{prefix}x{suffix}"));
            paths.insert(format!("{prefix}x{suffix}/"));
        }
    }
    // The cases the ruling named, spelled out so a failure reads plainly.
    for p in [
        "/api/changes/decisions",
        "/api/changes/decisions/withdraw",
        "/api/changes/notes",
        "/api/changes/links",
        "/api/changes/seconds",
        "/api/changes/seconds/withdraw",
        "/api/changes/associations",
        "/api/changes/scope/accept",
        "/api/changes/cancel",
        "/api/changes/standing",
        "/api/changes/export",
        "/api/changes/findings",
        "/api/changes/readiness",
        "/api/changes/coverage",
        "/api/sessions/changes",
        "/api/changes/",
        "/api/changes",
    ] {
        paths.insert(p.to_string());
    }

    let mut unanswered = Vec::new();
    for path in &paths {
        for method in ["GET", "POST", "PUT", "DELETE"] {
            let status = server.status(method, path);
            if status == 0 || status >= 500 {
                unanswered.push(format!("{method} {path} -> {status}"));
            }
        }
    }
    assert!(
        unanswered.is_empty(),
        "{} of {} requests got no answer or a 5xx (a handler panicked, hung or failed):\n{}",
        unanswered.len(),
        paths.len() * 4,
        unanswered.join("\n")
    );
    // The server is still serving after all of it.
    assert_ne!(server.status("GET", "/api/changes"), 0);
    assert_ne!(server.status("GET", "/"), 0);
}

#[test]
fn the_router_no_longer_slices_a_path_by_a_suffix_length() {
    // `p.len() - "/x".len()` is what made an overlapping prefix and suffix a
    // panic. The routes now take the id between them with `strip_prefix` and
    // `strip_suffix`, which answer None instead of panicking.
    for (name, source) in ROUTER_SOURCES {
        for (n, line) in source.lines().enumerate() {
            let code = line.split("//").next().unwrap_or("");
            assert!(
                !code.contains(".len() - \""),
                "{name}:{} slices a path by a suffix length: {}",
                n + 1,
                line.trim()
            );
        }
    }
}
