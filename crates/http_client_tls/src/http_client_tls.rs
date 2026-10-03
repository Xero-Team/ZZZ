use anyhow::Context as _;

use rustls::{ClientConfig, crypto::CryptoProvider};
use rustls_platform_verifier::ConfigVerifierExt;

pub fn tls_config() -> anyhow::Result<ClientConfig> {
    ensure_default_crypto_provider();

    ClientConfig::with_platform_verifier().context("building platform TLS verifier")
}

fn ensure_default_crypto_provider() {
    if CryptoProvider::get_default().is_some() {
        return;
    }

    if rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .is_err()
    {
        // Another thread can install the process-wide provider between the check and installation.
        debug_assert!(CryptoProvider::get_default().is_some());
    }
}

#[cfg(test)]
mod tests {
    use super::tls_config;

    #[test]
    fn tls_config_builds_successfully() {
        tls_config().unwrap();
    }

    #[test]
    fn tls_config_can_be_built_multiple_times() {
        tls_config().unwrap();
        tls_config().unwrap();
    }
}
