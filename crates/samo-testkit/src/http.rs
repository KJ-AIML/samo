//! A small HTTP server on `127.0.0.1` for the store's tests: it serves a folder, answers 404, redirects and
//! stalls on demand, and remembers every path asked for, so a test can prove nothing was fetched.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[derive(Default)]
struct Routes {
    redirects: HashMap<String, String>,
    stalls: HashSet<String>,
    stall_bodies: HashSet<String>,
    hits: Vec<String>,
}

struct Shared {
    dir: PathBuf,
    routes: Mutex<Routes>,
    stop: AtomicBool,
}

impl Shared {
    fn routes(&self) -> MutexGuard<'_, Routes> {
        self.routes.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }

    /// Waits until the server stops, so a stalled client sees no more data.
    fn hang(&self) {
        while !self.stopped() {
            thread::sleep(Duration::from_millis(20));
        }
    }
}

pub struct TestServer {
    addr: SocketAddr,
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl TestServer {
    /// Serves the files in `dir` on a free port until stopped or dropped.
    pub fn serve(dir: &Path) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        listener.set_nonblocking(true)?;
        let addr = listener.local_addr()?;
        let shared = Arc::new(Shared {
            dir: dir.to_path_buf(),
            routes: Mutex::new(Routes::default()),
            stop: AtomicBool::new(false),
        });
        let s = Arc::clone(&shared);
        let thread = thread::spawn(move || {
            while !s.stopped() {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let s = Arc::clone(&s);
                        thread::spawn(move || {
                            let _ = answer(&s, stream);
                        });
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
                }
            }
        });
        Ok(Self {
            addr,
            shared,
            thread: Some(thread),
        })
    }

    /// `http://127.0.0.1:<port>`, with no trailing slash.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// `path` answers 302 to `to`.
    pub fn redirect(&self, path: &str, to: &str) {
        self.shared
            .routes()
            .redirects
            .insert(path.into(), to.into());
    }

    /// `path` is read but never answered.
    pub fn stall(&self, path: &str) {
        self.shared.routes().stalls.insert(path.into());
    }

    /// `path` answers with its headers and the first bytes of its body, then nothing more.
    pub fn stall_body(&self, path: &str) {
        self.shared.routes().stall_bodies.insert(path.into());
    }

    /// Every path asked for so far, in order.
    pub fn hits(&self) -> Vec<String> {
        self.shared.routes().hits.clone()
    }

    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown();
    }
}

fn answer(s: &Shared, mut stream: TcpStream) -> io::Result<()> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut request = Vec::new();
    let mut buf = [0u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf)?;
        if n == 0 || request.len() > 16 * 1024 {
            return Ok(());
        }
        request.extend_from_slice(buf.get(..n).unwrap_or_default());
    }
    let text = String::from_utf8_lossy(&request);
    let mut first = text.lines().next().unwrap_or_default().split_whitespace();
    let method = first.next().unwrap_or_default().to_string();
    let target = first.next().unwrap_or_default();
    let path = target
        .split(['?', '#'])
        .next()
        .unwrap_or_default()
        .to_string();

    let (redirect, stall, stall_body) = {
        let mut r = s.routes();
        r.hits.push(path.clone());
        (
            r.redirects.get(&path).cloned(),
            r.stalls.contains(&path),
            r.stall_bodies.contains(&path),
        )
    };
    if stall {
        s.hang();
        return Ok(());
    }
    if let Some(to) = redirect {
        return write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    }
    let file = path.trim_start_matches('/');
    let body = if method == "GET"
        && !file.is_empty()
        && !file.split('/').any(|p| p == ".." || p.is_empty())
    {
        fs::read(s.dir.join(file)).ok()
    } else {
        None
    };
    let Some(body) = body else {
        return write!(
            stream,
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        );
    };
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    )?;
    if stall_body {
        stream.write_all(body.get(..body.len().min(16)).unwrap_or_default())?;
        stream.flush()?;
        s.hang();
        return Ok(());
    }
    stream.write_all(&body)?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn get(url: &str) -> Result<(u32, String, Vec<u8>), String> {
        let rest = url.strip_prefix("http://").ok_or("not http")?;
        let (host, path) = rest.split_once('/').ok_or("no path")?;
        let mut stream = TcpStream::connect(host).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        write!(stream, "GET /{path} HTTP/1.1\r\nHost: {host}\r\n\r\n")
            .map_err(|e| e.to_string())?;
        let mut all = Vec::new();
        stream.read_to_end(&mut all).map_err(|e| e.to_string())?;
        let split = all
            .windows(4)
            .position(|w| w == b"\r\n\r\n")
            .ok_or("no headers")?;
        let head = String::from_utf8_lossy(all.get(..split).unwrap_or_default()).into_owned();
        let status = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or("no status")?;
        Ok((
            status,
            head,
            all.get(split + 4..).unwrap_or_default().to_vec(),
        ))
    }

    #[test]
    fn serves_files_404s_and_redirects() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        fs::write(dir.path().join("catalog.json"), b"{}").map_err(|e| e.to_string())?;
        let server = TestServer::serve(dir.path()).map_err(|e| e.to_string())?;
        let base = server.url();

        let (status, _, body) = get(&format!("{base}/catalog.json?x=1"))?;
        assert_eq!((status, body.as_slice()), (200, b"{}".as_slice()));
        assert_eq!(get(&format!("{base}/missing.png"))?.0, 404);
        assert_eq!(get(&format!("{base}/../etc/passwd"))?.0, 404);

        server.redirect("/old.json", &format!("{base}/catalog.json"));
        let (status, head, _) = get(&format!("{base}/old.json"))?;
        assert_eq!(status, 302);
        assert!(head.contains(&format!("Location: {base}/catalog.json")));

        // A stalled path sends nothing until the server stops.
        server.stall("/slow.json");
        let host = base.trim_start_matches("http://").to_string();
        let mut stream = TcpStream::connect(&host).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_millis(300)))
            .map_err(|e| e.to_string())?;
        write!(stream, "GET /slow.json HTTP/1.1\r\nHost: {host}\r\n\r\n")
            .map_err(|e| e.to_string())?;
        let mut one = [0u8; 1];
        assert!(
            stream.read(&mut one).is_err(),
            "a stalled path must not answer"
        );

        assert_eq!(
            server.hits(),
            [
                "/catalog.json",
                "/missing.png",
                "/../etc/passwd",
                "/old.json",
                "/slow.json"
            ]
        );
        server.stop();
        Ok(())
    }
}
