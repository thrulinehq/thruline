use std::io::{Read, Write};
use thruline_core::decisions::checks_visual::visual_static::StaticDesignServer;
fn get(origin: &str, path: &str) -> String {
    let mut stream = std::net::TcpStream::connect(origin.trim_start_matches("http://")).unwrap();
    stream
        .write_all(
            format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
                .as_bytes(),
        )
        .unwrap();
    let mut out = String::new();
    stream.read_to_string(&mut out).unwrap();
    out
}
#[test]
fn closed_server_serves_only_exact_allowed_assets_and_never_falls_back_for_record_reads() {
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("index.html"), "<html>artboard</html>").unwrap();
    std::fs::create_dir(d.path().join("assets")).unwrap();
    std::fs::write(d.path().join("assets/app.js"), "fixed demo").unwrap();
    std::fs::write(d.path().join("private.json"), "not allowlisted").unwrap();
    let server = StaticDesignServer::start(
        d.path(),
        &["/".into(), "/assets/app.js".into(), "/favicon.ico".into()],
    )
    .unwrap();
    assert!(get(&server.origin, "/").contains("<html>artboard</html>"));
    assert!(get(&server.origin, "/assets/app.js").contains("fixed demo"));
    assert!(get(&server.origin, "/favicon.ico").starts_with("HTTP/1.1 204"));
    assert!(get(&server.origin, "/favicon.ico?new=1").starts_with("HTTP/1.1 404"));
    for path in [
        "/api/decisions",
        "/fixtures/index.json",
        "/private.json",
        "/../private.json",
        "/assets/app.js?different=1",
    ] {
        let r = get(&server.origin, path);
        assert!(r.starts_with("HTTP/1.1 404"), "{path}: {r}");
        assert!(!r.contains("fixed demo"));
    }
}

#[test]
fn large_javascript_asset_reaches_a_slow_reader_in_full() {
    use std::net::TcpStream;
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("assets")).unwrap();
    std::fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
    let body = vec![b'x'; 2 * 1024 * 1024];
    std::fs::write(dir.path().join("assets/large.js"), &body).unwrap();
    let server =
        StaticDesignServer::start(dir.path(), &["/".into(), "/assets/large.js".into()]).unwrap();
    let mut stream = TcpStream::connect(server.origin.trim_start_matches("http://")).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(5)))
        .unwrap();
    stream
        .write_all(b"GET /assets/large.js HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    std::thread::sleep(std::time::Duration::from_millis(30));
    let mut received = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        let n = stream.read(&mut buf).unwrap();
        if n == 0 {
            break;
        }
        received.extend_from_slice(&buf[..n]);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let split = received.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    let headers = std::str::from_utf8(&received[..split]).unwrap();
    assert!(headers.contains("Content-Length: 2097152\r\n"));
    assert_eq!(
        received[split..].len(),
        body.len(),
        "HTTP200 and a Content-Length do not prove complete delivery"
    );
    assert!(received[split..] == body);
}
