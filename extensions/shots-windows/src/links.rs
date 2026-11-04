//! The one file in Shots that turns text into a link (R16, checked by the network gate): links are handed to Windows,
//! which opens them in the browser. Shots itself never goes online. Only web links found in a shot can be opened, and
//! only on a click; anything else (`javascript:`, `file:`, an app's own scheme) can only be copied. Ported from the
//! Mac's `Links.swift`.

/// A web link, as found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebLink {
    pub url: String,
    pub host: String,
}

/// `text` as a web link (`http` or `https`, with a host), or `None`.
pub fn web(text: &str) -> Option<WebLink> {
    let url = text.trim();
    if url.chars().any(char::is_whitespace) {
        return None;
    }
    let (scheme, rest) = url.split_once("://")?;
    if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // user:pass@host:port, keeping the host.
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = match host_port.rsplit_once(':') {
        Some((host, port)) if port.chars().all(|c| c.is_ascii_digit()) => host,
        _ => host_port,
    };
    (!host.is_empty()).then(|| WebLink {
        url: url.to_string(),
        host: host.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_web_links_open() {
        assert_eq!(
            web("https://github.com/KJ-AIML/samo").map(|l| l.host),
            Some("github.com".to_string())
        );
        assert!(web("  HTTP://example.com/a?b=c  ").is_some());
        assert_eq!(
            web("https://example.com:8080/x").map(|l| l.host),
            Some("example.com".to_string())
        );
        for text in [
            "javascript:alert(1)",
            "file:///C:/Windows/win.ini",
            "ms-settings:privacy",
            "ftp://example.com",
            "http://",
            "hello world",
            "https://exa mple.com",
            "mailto:kj@example.com",
            "samo://open",
            "https://@/",
        ] {
            assert!(web(text).is_none(), "{text}");
        }
    }
}
