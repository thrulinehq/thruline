//! A closed design-artboard server. It serves verified build assets only.
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;
pub const IDENTITY: &str = include_str!("visual_static.rs");
pub struct StaticDesignServer {
    pub origin: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}
impl Drop for StaticDesignServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
impl StaticDesignServer {
    pub fn start(dist: &Path, paths: &[String]) -> Result<Self, &'static str> {
        let root = dist
            .canonicalize()
            .map_err(|_| "design build assets are missing")?;
        let mut assets = BTreeMap::new();
        if paths.len() > 8192 {
            return Err("design asset list is too large");
        }
        for path in paths {
            if !path.starts_with('/')
                || path.starts_with("//")
                || path.contains("..")
                || path.contains('?')
                || path.contains('\\')
            {
                return Err("design asset path is invalid");
            }
            if path == "/favicon.ico" {
                assets.insert(path.clone(), (Vec::new(), "image/x-icon"));
                continue;
            }
            let file = root.join(if path == "/" {
                "index.html"
            } else {
                &path[1..]
            });
            let meta = std::fs::symlink_metadata(&file).map_err(|_| "design asset is missing")?;
            if !meta.is_file()
                || meta.file_type().is_symlink()
                || !file
                    .canonicalize()
                    .map_err(|_| "design asset missing")?
                    .starts_with(&root)
                || meta.len() > 32 * 1024 * 1024
            {
                return Err("design asset is invalid");
            }
            let body = std::fs::read(file).map_err(|_| "design asset is unreadable")?;
            let mime = if path == "/" || path.ends_with(".html") {
                "text/html; charset=utf-8"
            } else if path.ends_with(".js") {
                "text/javascript; charset=utf-8"
            } else if path.ends_with(".css") {
                "text/css; charset=utf-8"
            } else if path.ends_with(".woff2") {
                "font/woff2"
            } else if path.ends_with(".svg") {
                "image/svg+xml"
            } else if path.ends_with(".png") {
                "image/png"
            } else {
                "application/octet-stream"
            };
            if assets.insert(path.clone(), (body, mime)).is_some() {
                return Err("duplicate design asset path");
            }
        }
        if !assets.contains_key("/") {
            return Err("design index is missing");
        }
        let listener =
            TcpListener::bind(("127.0.0.1", 0)).map_err(|_| "design server is unavailable")?;
        listener
            .set_nonblocking(true)
            .map_err(|_| "design server is unavailable")?;
        let origin = format!(
            "http://{}",
            listener
                .local_addr()
                .map_err(|_| "design server is unavailable")?
        );
        let stop = Arc::new(AtomicBool::new(false));
        let stopped = stop.clone();
        let assets = Arc::new(assets);
        let active = Arc::new(AtomicUsize::new(0));
        let thread = std::thread::spawn(move || {
            while !stopped.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if active.fetch_add(1, Ordering::Relaxed) >= 64 {
                            active.fetch_sub(1, Ordering::Relaxed);
                            continue;
                        }
                        let assets = assets.clone();
                        let active = active.clone();
                        std::thread::spawn(move || {
                            serve(stream, &assets);
                            active.fetch_sub(1, Ordering::Relaxed);
                        });
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            origin,
            stop,
            thread: Some(thread),
        })
    }
}
fn serve(mut stream: TcpStream, assets: &BTreeMap<String, (Vec<u8>, &str)>) {
    // macOS accepted sockets can inherit O_NONBLOCK from the listener. A
    // successful response requires the complete advertised body to be sent.
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
    let mut bytes = Vec::new();
    let mut buf = [0u8; 2048];
    while bytes.len() < 16384 {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => {
                bytes.extend_from_slice(&buf[..n]);
                if bytes.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
        }
    }
    let Ok(request) = std::str::from_utf8(&bytes) else {
        return;
    };
    let mut line = request.lines().next().unwrap_or("").split_whitespace();
    let method = line.next().unwrap_or("");
    let path = line.next().unwrap_or("");
    let allowed = (method == "GET" || method == "HEAD")
        .then(|| assets.get(path))
        .flatten();
    let (status, body, mime) = match allowed {
        Some((body, mime)) => (
            if path == "/favicon.ico" {
                "204 No Content"
            } else {
                "200 OK"
            },
            body.as_slice(),
            *mime,
        ),
        None => ("404 Not Found", b"Not found".as_slice(), "text/plain"),
    };
    let header=format!("HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n",body.len());
    if stream.write_all(header.as_bytes()).is_err() {
        return;
    }
    if method != "HEAD" {
        // The connection closes on function exit, including a failed write.
        // No transport or capture success is cached by this server.
        let _ = stream.write_all(body);
    }
}
