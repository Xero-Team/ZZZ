//! Typst package downloads over the application's rustls stack.
//!
//! `typst-kit` can download packages through `SystemDownloader`, but that
//! implementation links `native-tls` and therefore OpenSSL on Linux. OpenSSL is
//! a runtime dependency the editor must not have: when it is missing or a
//! different major version is installed, the dynamic loader refuses to start
//! the binary at all. Everything here keeps the download path inside the same
//! rustls configuration the rest of the application already uses.

use std::any::Any;
use std::io::{self, Read};
use std::sync::Arc;

use http_client::{Url, read_no_proxy_from_env, read_proxy_from_env};
use typst_kit::downloader::Downloader;

/// User agent advertised when fetching packages from the Typst registry.
const USER_AGENT: &str = "ZZZ Typst Preview";

/// A [`Downloader`] that reaches the Typst registry through rustls.
///
/// Two agents are prepared up front because proxies in `ureq` are configured
/// per agent while `NO_PROXY` has to be honoured per request: one agent talks to
/// the registry directly, the other goes through the proxy from the
/// environment.
pub struct PackageDownloader {
    direct: ureq::Agent,
    proxied: Option<ureq::Agent>,
    no_proxy: Option<String>,
}

impl PackageDownloader {
    /// Builds the downloader, or fails when the platform TLS verifier cannot be
    /// initialised.
    pub fn new() -> anyhow::Result<Self> {
        let tls_config = Arc::new(http_client_tls::tls_config()?);

        let direct = ureq::AgentBuilder::new()
            .user_agent(USER_AGENT)
            .tls_config(Arc::clone(&tls_config))
            .build();

        let proxied = read_proxy_from_env().and_then(|proxy_url| {
            match ureq::Proxy::new(proxy_url.as_str()) {
                Ok(proxy) => Some(
                    ureq::AgentBuilder::new()
                        .user_agent(USER_AGENT)
                        .tls_config(tls_config)
                        .proxy(proxy)
                        .build(),
                ),
                Err(error) => {
                    log::warn!("ignoring unusable proxy {proxy_url}: {error}");
                    None
                }
            }
        });

        Ok(Self {
            direct,
            proxied,
            no_proxy: read_no_proxy_from_env(),
        })
    }

    fn agent_for(&self, url: &str) -> &ureq::Agent {
        let Some(proxied) = self.proxied.as_ref() else {
            return &self.direct;
        };

        match self.no_proxy.as_deref() {
            Some(no_proxy) if is_excluded_from_proxy(url, no_proxy) => &self.direct,
            _ => proxied,
        }
    }
}

impl Downloader for PackageDownloader {
    fn stream(&self, _key: &dyn Any, url: &str) -> io::Result<(Option<usize>, Box<dyn Read>)> {
        let response = self
            .agent_for(url)
            .get(url)
            .call()
            .map_err(|error| match error {
                ureq::Error::Status(404, _) => io::Error::new(io::ErrorKind::NotFound, error),
                error => io::Error::other(error),
            })?;

        let content_length = response
            .header("Content-Length")
            .and_then(|value| value.parse::<usize>().ok());

        Ok((content_length, Box::new(response.into_reader())))
    }
}

/// Whether `NO_PROXY` asks for `url` to bypass the proxy.
fn is_excluded_from_proxy(url: &str, no_proxy: &str) -> bool {
    let Ok(url) = Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };

    no_proxy.split(',').any(|entry| {
        let entry = entry.trim();
        let entry = entry
            .rsplit_once(':')
            .filter(|(_host, port)| port.parse::<u16>().is_ok())
            .map_or(entry, |(host, _port)| host);

        if entry.is_empty() {
            return false;
        }
        if entry == "*" {
            return true;
        }

        let entry = entry.trim_start_matches('.');
        host == entry || host.ends_with(&format!(".{entry}"))
    })
}

#[cfg(test)]
mod tests {
    use super::is_excluded_from_proxy;

    #[test]
    fn wildcard_excludes_every_host() {
        assert!(is_excluded_from_proxy(
            "https://packages.typst.org/index.json",
            "*"
        ));
    }

    #[test]
    fn exact_host_and_suffix_entries_match() {
        let url = "https://packages.typst.org/index.json";

        assert!(is_excluded_from_proxy(url, "packages.typst.org"));
        assert!(is_excluded_from_proxy(url, "typst.org"));
        assert!(is_excluded_from_proxy(url, ".typst.org"));
        assert!(is_excluded_from_proxy(
            url,
            " example.com , packages.typst.org"
        ));
    }

    #[test]
    fn unrelated_entries_do_not_match() {
        let url = "https://packages.typst.org/index.json";

        assert!(!is_excluded_from_proxy(url, "example.com"));
        assert!(!is_excluded_from_proxy(url, "typst.org.example.com"));
        assert!(!is_excluded_from_proxy(url, ""));
    }
}
