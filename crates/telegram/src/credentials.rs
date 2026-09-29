use std::env;

const EMBEDDED_API_ID: Option<&str> = option_env!("ZZZ_TELEGRAM_API_ID");
const EMBEDDED_API_HASH: Option<&str> = option_env!("ZZZ_TELEGRAM_API_HASH");

/// The Telegram application credentials compiled into this build.
///
/// `api_id` and `api_hash` are application credentials, not user secrets. A
/// user's real credential is the encrypted session stored on disk. Runtime
/// environment variables take precedence over the embedded values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TelegramCredentials {
    pub api_id: i32,
    pub api_hash: String,
}

impl TelegramCredentials {
    /// Resolves credentials from the runtime environment or the values embedded
    /// at compile time. Returns `None` when neither is present, in which case the
    /// panel stays offline and reports that this build is not configured.
    pub fn resolve() -> Option<Self> {
        Self::resolve_from(
            env::var("ZZZ_TELEGRAM_API_ID").ok(),
            env::var("ZZZ_TELEGRAM_API_HASH").ok(),
        )
    }

    fn resolve_from(
        runtime_api_id: Option<String>,
        runtime_api_hash: Option<String>,
    ) -> Option<Self> {
        let api_id = runtime_api_id
            .or_else(|| EMBEDDED_API_ID.map(ToOwned::to_owned))
            .and_then(|value| value.trim().parse::<i32>().ok())?;
        let api_hash = runtime_api_hash
            .or_else(|| EMBEDDED_API_HASH.map(ToOwned::to_owned))
            .filter(|value| !value.trim().is_empty())?;
        Some(Self { api_id, api_hash })
    }
}

#[cfg(test)]
mod tests {
    use super::TelegramCredentials;

    #[test]
    fn runtime_values_are_used_when_present() {
        let credentials =
            TelegramCredentials::resolve_from(Some("12345".to_owned()), Some("abcdef".to_owned()))
                .expect("credentials");
        assert_eq!(credentials.api_id, 12345);
        assert_eq!(credentials.api_hash, "abcdef");
    }

    #[test]
    fn invalid_api_id_is_rejected() {
        assert!(
            TelegramCredentials::resolve_from(
                Some("not-a-number".to_owned()),
                Some("x".to_owned())
            )
            .is_none()
        );
    }

    #[test]
    fn empty_api_hash_is_rejected() {
        assert!(
            TelegramCredentials::resolve_from(Some("1".to_owned()), Some("  ".to_owned()))
                .is_none()
        );
    }
}
