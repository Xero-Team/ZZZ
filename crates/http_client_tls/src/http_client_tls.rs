use anyhow::Context as _;

use rustls::ClientConfig;
use rustls_platform_verifier::ConfigVerifierExt;

pub fn tls_config() -> anyhow::Result<ClientConfig> {
    // rustls uses the `aws_lc_rs` provider by default.
    // This only errors if the default provider has already been installed.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .ok();

    ClientConfig::with_platform_verifier().context("building platform TLS verifier")
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
