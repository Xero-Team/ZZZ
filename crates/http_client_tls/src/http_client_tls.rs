use anyhow::Context as _;

use rustls::{ClientConfig, crypto::CryptoProvider};
use rustls_platform_verifier::ConfigVerifierExt;

pub fn tls_config() -> anyhow::Result<ClientConfig> {
    ensure_default_crypto_provider()?;

    ClientConfig::with_platform_verifier().context("building platform TLS verifier")
}

fn ensure_default_crypto_provider() -> anyhow::Result<()> {
    if CryptoProvider::get_default().is_some() {
        return Ok(());
    }

    match rustls::crypto::aws_lc_rs::default_provider().install_default() {
        Ok(()) => Ok(()),
        Err(_) if CryptoProvider::get_default().is_some() => Ok(()),
        Err(_) => anyhow::bail!("failed to install the default rustls crypto provider"),
    }
}

#[cfg(test)]
mod tests {
    use super::tls_config;

    #[test]
    fn tls_config_builds_successfully() {
        tls_config().expect("TLS config should build");
    }

    #[test]
    fn tls_config_can_be_built_multiple_times() {
        tls_config().expect("TLS config should build");
        tls_config().expect("TLS config should build on subsequent calls");
    }
}
