//! client proxy

mod http_proxy;
mod socks_proxy;

use anyhow::{Context as _, Result};
use http_client::Url;
use http_proxy::{HttpProxyType, connect_http_proxy_stream, parse_http_proxy};
use socks_proxy::{SocksVersion, connect_socks_proxy_stream, parse_socks_proxy};

pub(crate) async fn connect_proxy_stream(
    proxy: &Url,
    rpc_host: (&str, u16),
) -> Result<Box<dyn AsyncReadWrite>> {
    let Some(((proxy_domain, proxy_port), proxy_type)) = parse_proxy_type(proxy) else {
        // If parsing the proxy URL fails, we must avoid falling back to an insecure connection.
        // SOCKS proxies are often used in contexts where security and privacy are critical,
        // so any fallback could expose users to significant risks.
        anyhow::bail!("Parsing proxy url failed");
    };

    // Connect to proxy and wrap protocol later
    let stream = tokio::net::TcpStream::connect((proxy_domain.as_str(), proxy_port))
        .await
        .context("Failed to connect to proxy")?;

    let proxy_stream = match proxy_type {
        ProxyType::SocksProxy(proxy) => connect_socks_proxy_stream(stream, proxy, rpc_host).await?,
        ProxyType::HttpProxy(proxy) => {
            connect_http_proxy_stream(stream, proxy, rpc_host, &proxy_domain).await?
        }
    };

    Ok(proxy_stream)
}

enum ProxyType<'t> {
    SocksProxy(SocksVersion<'t>),
    HttpProxy(HttpProxyType<'t>),
}

fn parse_proxy_type(proxy: &Url) -> Option<((String, u16), ProxyType<'_>)> {
    let scheme = proxy.scheme();
    if !is_supported_proxy_scheme(scheme) {
        return None;
    }
    let host = proxy.host()?.to_string();
    let port = proxy.port_or_known_default()?;
    let proxy_type = match scheme {
        "socks4" | "socks4a" | "socks5" | "socks5h" => {
            ProxyType::SocksProxy(parse_socks_proxy(scheme, proxy))
        }
        "http" | "https" => ProxyType::HttpProxy(parse_http_proxy(scheme, proxy)),
        _ => unreachable!("proxy scheme should have been validated"),
    };

    Some(((host, port), proxy_type))
}

pub(crate) fn is_supported_proxy_scheme(scheme: &str) -> bool {
    matches!(
        scheme,
        "http" | "https" | "socks4" | "socks4a" | "socks5" | "socks5h"
    )
}

/// Whether `NO_PROXY` in the environment excludes `host` from proxying,
/// matching the exclusions the HTTP client applies to its own requests.
pub(crate) fn excluded_from_proxy(host: &str) -> bool {
    http_client::read_no_proxy_from_env().is_some_and(|no_proxy| no_proxy_matches(&no_proxy, host))
}

/// Matches `host` against a `NO_PROXY`-style exclusion list: comma-separated
/// entries that are `*`, an exact host, or a domain suffix (bare, with a
/// leading dot, or with a leading `*.`). Entries may carry a `:port`, which is
/// ignored, since exclusion is by host. CIDR ranges are not supported.
pub(crate) fn no_proxy_matches(no_proxy: &str, host: &str) -> bool {
    no_proxy
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .any(|entry| {
            if entry == "*" {
                return true;
            }
            // Bare IPv6 literals also contain colons, so only strip a port when
            // there is exactly one colon and the suffix parses as one.
            let entry = match entry.rsplit_once(':') {
                Some((prefix, suffix))
                    if !prefix.contains(':') && suffix.parse::<u16>().is_ok() =>
                {
                    prefix
                }
                _ => entry,
            };
            let entry = entry.trim_start_matches("*.");
            let entry = entry.trim_start_matches('.');
            host.eq_ignore_ascii_case(entry)
                || (host.len() > entry.len()
                    && host.as_bytes()[host.len() - entry.len() - 1] == b'.'
                    && host[host.len() - entry.len()..].eq_ignore_ascii_case(entry))
        })
}

pub(crate) trait AsyncReadWrite:
    tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static
{
}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static> AsyncReadWrite
    for T
{
}

#[cfg(test)]
mod tests {
    use super::{is_supported_proxy_scheme, no_proxy_matches, parse_proxy_type};
    use url::Url;

    #[test]
    fn no_proxy_excludes_matching_hosts() {
        assert!(no_proxy_matches("example.com", "example.com"));
        assert!(no_proxy_matches("example.com", "sub.example.com"));
        assert!(no_proxy_matches(".example.com", "sub.example.com"));
        assert!(no_proxy_matches("*.example.com", "sub.example.com"));
        assert!(no_proxy_matches("other.org, example.com", "example.com"));
        assert!(no_proxy_matches("example.com:443", "example.com"));
        assert!(no_proxy_matches("*", "anything.at.all"));
        assert!(no_proxy_matches("EXAMPLE.com", "example.COM"));
        assert!(no_proxy_matches("::1", "::1"));

        assert!(!no_proxy_matches("example.com", "notexample.com"));
        assert!(!no_proxy_matches("example.com", "example.org"));
        assert!(!no_proxy_matches("", "example.com"));
        assert!(!no_proxy_matches("sub.example.com", "example.com"));
    }

    #[test]
    fn accepts_only_documented_proxy_schemes() {
        for scheme in ["http", "https", "socks4", "socks4a", "socks5", "socks5h"] {
            assert!(is_supported_proxy_scheme(scheme));
            let proxy = Url::parse(&format!("{scheme}://proxy.example.com:1080"))
                .expect("documented proxy URL should parse");
            assert!(
                parse_proxy_type(&proxy).is_some(),
                "scheme {scheme} should be supported"
            );
        }

        for scheme in ["httpx", "socks", "socks6", "ftp"] {
            assert!(!is_supported_proxy_scheme(scheme));
            let proxy = Url::parse(&format!("{scheme}://proxy.example.com:1080"))
                .expect("test proxy URL should parse");
            assert!(
                parse_proxy_type(&proxy).is_none(),
                "scheme {scheme} should be rejected"
            );
        }
    }
}
