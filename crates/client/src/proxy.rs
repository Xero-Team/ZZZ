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
    use super::{is_supported_proxy_scheme, parse_proxy_type};
    use url::Url;

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
