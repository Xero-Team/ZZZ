//! Normalizes a configured proxy URL for grammers, which only speaks SOCKS5.
//!
//! HTTP and HTTPS proxies cannot be used by Telegram here; they are ignored so
//! the engine connects directly. Callers surface that limitation in the UI.

/// Returns a grammers-compatible `socks5://` URL, or `None` for anything else.
pub fn socks5_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let (scheme, rest) = trimmed.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if scheme != "socks5" && scheme != "socks5h" {
        return None;
    }
    if rest.is_empty() {
        return None;
    }
    Some(format!("socks5://{rest}"))
}

#[cfg(test)]
mod tests {
    use super::socks5_url;

    #[test]
    fn accepts_socks5() {
        assert_eq!(
            socks5_url("socks5://127.0.0.1:1080"),
            Some("socks5://127.0.0.1:1080".to_owned())
        );
    }

    #[test]
    fn normalizes_socks5h_and_is_case_insensitive() {
        assert_eq!(
            socks5_url("SOCKS5H://user:pass@host:1080"),
            Some("socks5://user:pass@host:1080".to_owned())
        );
    }

    #[test]
    fn rejects_http_and_garbage() {
        assert_eq!(socks5_url("http://127.0.0.1:8080"), None);
        assert_eq!(socks5_url("https://proxy.example"), None);
        assert_eq!(socks5_url("socks5://"), None);
        assert_eq!(socks5_url(""), None);
        assert_eq!(socks5_url("not a url"), None);
    }
}
