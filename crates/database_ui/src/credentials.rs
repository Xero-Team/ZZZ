//! Bridge between the database explorer and ZZZ's keychain.
//!
//! Passwords and tokens have a single source: the `credentials_provider` on the
//! global `Client`. They are resolved here, passed to `ConnectionManager::connect`
//! for the duration of that call, and never written into a config file, log,
//! error message, or query history (D-09, invariant 3).

use std::sync::Arc;

use anyhow::Result;
use client::Client;
use credentials_provider::CredentialsProvider;
use gpui::{App, AsyncApp};

const SERVICE_USERNAME: &str = "zzz-database";

/// Resolves database secrets through the platform keychain. Holds only the
/// `Send + Sync` provider so it can live in the app-global `DatabaseState`;
/// callers supply the `AsyncApp` for the current task.
pub struct KeychainCredentials {
    provider: Arc<dyn CredentialsProvider>,
}

impl KeychainCredentials {
    pub fn new(cx: &App) -> Self {
        Self {
            provider: Client::global(cx).credentials_provider(),
        }
    }

    /// Read the secret for `key`, or `None` when nothing is stored.
    pub async fn read(&self, key: &str, cx: &AsyncApp) -> Result<Option<Vec<u8>>> {
        let result = self.provider.read_credentials(key, cx).await?;
        Ok(result.map(|(_username, secret)| secret))
    }

    pub async fn write(&self, key: &str, secret: &[u8], cx: &AsyncApp) -> Result<()> {
        self.provider
            .write_credentials(key, SERVICE_USERNAME, secret, cx)
            .await
    }

    pub async fn delete(&self, key: &str, cx: &AsyncApp) -> Result<()> {
        self.provider.delete_credentials(key, cx).await
    }
}
