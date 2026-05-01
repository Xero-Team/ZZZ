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
