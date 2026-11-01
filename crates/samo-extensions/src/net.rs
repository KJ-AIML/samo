//! The store's HTTP client (R6, R12): the only code in Samo that goes online, and only with a click.
//!
//! Every fetch needs a [`UserAction`], which only the Extensions page and Settings may make (the network gate
//! checks), so nothing can fetch at launch or on a timer. HTTPS only, to `github.com` and `*.githubusercontent.com`,
//! redirects included; plain `http` only to this Mac, for tests. A fetch stops at its cap, and fails after 15 s
//! without data.

use std::fmt;
use std::io::Read;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;
use ureq::Agent;
use ureq::http::Uri;

/// Proof that a person clicked something. Not `Clone`: each click is spent on the work it started.
#[derive(Debug)]
pub struct UserAction {
    _private: (),
}

impl UserAction {
    /// Only in a click handler in `extensions_page.rs` or `settings_page.rs` (the network gate checks).
    pub fn clicked() -> Self {
        Self { _private: () }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NetError {
    /// The address isn't one Samo may visit.
    NotAllowed(String),
    /// The Mac is offline, or the server couldn't be reached.
    Offline(String),
    /// The server answered with something other than 200.
    Status(u16),
    TooBig {
        cap: u64,
    },
    TimedOut,
    TooManyRedirects,
}

impl fmt::Display for NetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetError::NotAllowed(url) => write!(f, "Samo doesn't go to {url}"),
            NetError::Offline(why) => write!(f, "couldn't reach the store ({why})"),
            NetError::Status(404) => write!(f, "the store doesn't have that file"),
            NetError::Status(code) => write!(f, "the store answered {code}"),
            NetError::TooBig { cap } => {
                write!(f, "the file is bigger than Samo allows ({cap} bytes)")
            }
            NetError::TimedOut => write!(f, "the store stopped answering"),
            NetError::TooManyRedirects => write!(f, "the store sent Samo around in circles"),
        }
    }
}

impl std::error::Error for NetError {}

const IDLE: Duration = Duration::from_secs(15);
/// The longest a whole download may take, so a stalled reader never outlives it by much.
const BODY_BUDGET: Duration = Duration::from_secs(30 * 60);
const MAX_REDIRECTS: usize = 5;
const CHUNK: usize = 64 * 1024;

/// Whether Samo may fetch `url`: HTTPS to GitHub's hosts, or plain HTTP to this Mac.
pub fn allowed(url: &str) -> bool {
    let Ok(uri) = url.parse::<Uri>() else {
        return false;
    };
    let host = uri.host().unwrap_or_default().to_ascii_lowercase();
    let loopback = host == "127.0.0.1" || host == "localhost";
    match uri.scheme_str() {
        Some("https") => {
            loopback || host == "github.com" || host.ends_with(".githubusercontent.com")
        }
        Some("http") => loopback,
        _ => false,
    }
}

/// Where a redirect points, against the address that sent it.
fn resolve_location(from: &str, location: &str) -> Option<String> {
    if location.contains("://") {
        return Some(location.to_string());
    }
    let uri = from.parse::<Uri>().ok()?;
    let origin = format!("{}://{}", uri.scheme_str()?, uri.authority()?);
    if location.starts_with('/') {
        return Some(format!("{origin}{location}"));
    }
    let path = uri.path();
    let dir = path.rsplit_once('/').map(|(d, _)| d).unwrap_or_default();
    Some(format!("{origin}{dir}/{location}"))
}

pub struct Net {
    agent: Agent,
    idle: Duration,
}

impl Default for Net {
    fn default() -> Self {
        Self::with_idle(IDLE)
    }
}

impl Net {
    /// A client whose fetches fail after `idle` without data (15 s outside tests).
    pub fn with_idle(idle: Duration) -> Self {
        let agent: Agent = Agent::config_builder()
            .max_redirects(0)
            .http_status_as_error(false)
            .timeout_connect(Some(idle))
            .timeout_send_request(Some(idle))
            .timeout_recv_response(Some(idle))
            .timeout_recv_body(Some(BODY_BUDGET))
            .user_agent(format!("Samo/{}", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent, idle }
    }

    /// GETs `url`, following redirects only to allowed hosts, and refusing more than `cap` bytes.
    pub fn fetch(&self, url: &str, cap: u64, _action: &UserAction) -> Result<Vec<u8>, NetError> {
        let mut url = url.to_string();
        for _ in 0..=MAX_REDIRECTS {
            if !allowed(&url) {
                return Err(NetError::NotAllowed(url));
            }
            let response = self.agent.get(&url).call().map_err(|e| match e {
                ureq::Error::Timeout(_) => NetError::TimedOut,
                other => NetError::Offline(other.to_string()),
            })?;
            let status = response.status().as_u16();
            if (300..400).contains(&status) {
                let location = response
                    .headers()
                    .get("location")
                    .and_then(|v| v.to_str().ok())
                    .ok_or(NetError::Status(status))?;
                url = resolve_location(&url, location).ok_or(NetError::Status(status))?;
                continue;
            }
            if status != 200 {
                return Err(NetError::Status(status));
            }
            if response.body().content_length().is_some_and(|n| n > cap) {
                return Err(NetError::TooBig { cap });
            }
            return self.read_body(response.into_body().into_reader(), cap);
        }
        Err(NetError::TooManyRedirects)
    }

    /// Reads on its own thread, so a server that goes quiet mid-download fails after `idle`, not at the end of
    /// the whole download's budget.
    fn read_body(
        &self,
        mut reader: impl Read + Send + 'static,
        cap: u64,
    ) -> Result<Vec<u8>, NetError> {
        let (tx, rx) = mpsc::sync_channel::<Result<Vec<u8>, String>>(4);
        thread::spawn(move || {
            let mut buf = vec![0u8; CHUNK];
            loop {
                let chunk = match reader.read(&mut buf) {
                    Ok(0) => return,
                    Ok(n) => Ok(buf.get(..n).unwrap_or_default().to_vec()),
                    Err(e) => Err(e.to_string()),
                };
                let failed = chunk.is_err();
                if tx.send(chunk).is_err() || failed {
                    return;
                }
            }
        });
        let mut body = Vec::new();
        loop {
            match rx.recv_timeout(self.idle) {
                Ok(Ok(chunk)) => {
                    body.extend_from_slice(&chunk);
                    if u64::try_from(body.len()).unwrap_or(u64::MAX) > cap {
                        return Err(NetError::TooBig { cap });
                    }
                }
                Ok(Err(why)) if why.contains("imed out") => return Err(NetError::TimedOut),
                Ok(Err(why)) => return Err(NetError::Offline(why)),
                Err(mpsc::RecvTimeoutError::Timeout) => return Err(NetError::TimedOut),
                Err(mpsc::RecvTimeoutError::Disconnected) => return Ok(body),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use samo_testkit::http::TestServer;
    use std::time::Instant;

    fn serve(files: &[(&str, &[u8])]) -> Result<(tempfile::TempDir, TestServer), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        for (name, bytes) in files {
            std::fs::write(dir.path().join(name), bytes).map_err(|e| e.to_string())?;
        }
        let server = TestServer::serve(dir.path()).map_err(|e| e.to_string())?;
        Ok((dir, server))
    }

    #[test]
    fn fetch_respects_cap() -> Result<(), String> {
        let big = vec![7u8; 300 * 1024];
        let (_dir, server) = serve(&[("a.bin", b"hello"), ("big.bin", &big)])?;
        let net = Net::default();
        let click = UserAction::clicked();
        assert_eq!(
            net.fetch(&format!("{}/a.bin", server.url()), 5, &click),
            Ok(b"hello".to_vec())
        );
        assert_eq!(
            net.fetch(&format!("{}/a.bin", server.url()), 4, &click),
            Err(NetError::TooBig { cap: 4 })
        );
        assert_eq!(
            net.fetch(&format!("{}/big.bin", server.url()), 200 * 1024, &click),
            Err(NetError::TooBig { cap: 200 * 1024 })
        );
        assert_eq!(
            net.fetch(&format!("{}/nope", server.url()), 5, &click),
            Err(NetError::Status(404))
        );
        Ok(())
    }

    #[test]
    fn redirect_only_to_allowed_hosts() -> Result<(), String> {
        let (_dir, server) = serve(&[("catalog.json", b"{}")])?;
        let base = server.url();
        server.redirect("/here", &format!("{base}/catalog.json"));
        server.redirect("/relative", "catalog.json");
        server.redirect("/away", "https://evil.example/catalog.json");
        server.redirect("/sneaky", "https://github.com@evil.example/catalog.json");
        server.redirect("/loop", &format!("{base}/loop"));
        let net = Net::default();
        let click = UserAction::clicked();
        assert_eq!(
            net.fetch(&format!("{base}/here"), 100, &click),
            Ok(b"{}".to_vec())
        );
        assert_eq!(
            net.fetch(&format!("{base}/relative"), 100, &click),
            Ok(b"{}".to_vec())
        );
        assert!(matches!(
            net.fetch(&format!("{base}/away"), 100, &click),
            Err(NetError::NotAllowed(_))
        ));
        assert!(matches!(
            net.fetch(&format!("{base}/sneaky"), 100, &click),
            Err(NetError::NotAllowed(_))
        ));
        assert_eq!(
            net.fetch(&format!("{base}/loop"), 100, &click),
            Err(NetError::TooManyRedirects)
        );

        assert!(allowed(
            "https://github.com/KJ-AIML/samo-extensions/releases/download/store/catalog.json"
        ));
        assert!(allowed("https://release-assets.githubusercontent.com/x"));
        assert!(allowed("https://objects.githubusercontent.com/x"));
        assert!(!allowed("https://githubusercontent.com.evil.example/x"));
        assert!(!allowed("https://evilgithub.com/x"));
        assert!(!allowed("ftp://github.com/x"));
        Ok(())
    }

    #[test]
    fn plain_http_only_on_loopback() -> Result<(), String> {
        assert!(allowed("http://127.0.0.1:8080/catalog.json"));
        assert!(allowed("http://localhost:8080/catalog.json"));
        assert!(!allowed("http://github.com/catalog.json"));
        assert!(!allowed("http://192.168.1.2/catalog.json"));
        assert!(!allowed("http://127.0.0.1.evil.example/catalog.json"));
        let net = Net::default();
        assert!(matches!(
            net.fetch("http://github.com/x", 10, &UserAction::clicked()),
            Err(NetError::NotAllowed(_))
        ));
        Ok(())
    }

    #[test]
    fn stalled_request_times_out() -> Result<(), String> {
        let (_dir, server) = serve(&[("slow.json", &[1u8; 4096]), ("half.json", &[2u8; 4096])])?;
        server.stall("/slow.json");
        server.stall_body("/half.json");
        let net = Net::with_idle(Duration::from_millis(300));
        let click = UserAction::clicked();
        for path in ["/slow.json", "/half.json"] {
            let started = Instant::now();
            assert_eq!(
                net.fetch(&format!("{}{path}", server.url()), 1 << 20, &click),
                Err(NetError::TimedOut),
                "{path}"
            );
            assert!(
                started.elapsed() < Duration::from_secs(3),
                "{path} took {:?}",
                started.elapsed()
            );
        }
        Ok(())
    }
}
