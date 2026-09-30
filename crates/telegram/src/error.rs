//! Structured engine errors.
//!
//! The engine never builds user-facing English strings. It reports a
//! [`EngineError`] variant and lets the panel render localized text.

use grammers_client::InvocationError;

/// A failure the engine can report to the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    NotConnected,
    NoCredentials,
    InvalidPhone,
    RequestCodeFailed,
    InvalidCode,
    SignUpRequired,
    InvalidPassword,
    SignInFailed,
    QrFailed,
    QrTooManyMigrations,
    NoPendingPassword,
    ChatUnavailable,
    ChatUnavailableOffline,
    SendFailed,
    LoadChatsFailed,
    LoadMessagesFailed,
    DeleteFailed,
    ForwardFailed,
    PinFailed,
    SearchFailed,
    /// Telegram asked the client to wait before retrying.
    FloodWait {
        seconds: u64,
    },
    Other,
}

impl EngineError {
    /// Maps a grammers invocation failure, preserving `FLOOD_WAIT` seconds and
    /// the recognizable account errors.
    pub fn from_invocation(error: &InvocationError) -> Self {
        Self::from_invocation_with(error, Self::Other)
    }

    /// Maps a grammers invocation failure, falling back to `fallback` for RPC
    /// errors that have no dedicated variant.
    pub fn from_invocation_with(error: &InvocationError, fallback: Self) -> Self {
        if let Some(seconds) = flood_wait_seconds(error) {
            return Self::FloodWait { seconds };
        }
        match error {
            InvocationError::Rpc(rpc) => match rpc.name.as_str() {
                "PHONE_NUMBER_INVALID" => Self::InvalidPhone,
                "PHONE_CODE_INVALID" | "PHONE_CODE_EMPTY" | "PHONE_CODE_EXPIRED" => {
                    Self::InvalidCode
                }
                "PASSWORD_HASH_INVALID" => Self::InvalidPassword,
                _ => fallback,
            },
            _ => fallback,
        }
    }
}

fn flood_wait_seconds(error: &InvocationError) -> Option<u64> {
    match error {
        InvocationError::Rpc(rpc) if rpc.is("FLOOD_WAIT") => Some(rpc.value.map_or(0, u64::from)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::EngineError;
    use grammers_client::InvocationError;
    use grammers_client::sender::RpcError;
    use std::io;

    fn rpc(name: &str, value: Option<u32>) -> InvocationError {
        InvocationError::Rpc(RpcError {
            code: 420,
            name: name.to_owned(),
            value,
            caused_by: None,
        })
    }

    #[test]
    fn flood_wait_is_preserved() {
        assert_eq!(
            EngineError::from_invocation(&rpc("FLOOD_WAIT", Some(31))),
            EngineError::FloodWait { seconds: 31 }
        );
    }

    #[test]
    fn account_errors_map_to_specific_variants() {
        assert_eq!(
            EngineError::from_invocation(&rpc("PHONE_NUMBER_INVALID", None)),
            EngineError::InvalidPhone
        );
        assert_eq!(
            EngineError::from_invocation(&rpc("PHONE_CODE_INVALID", None)),
            EngineError::InvalidCode
        );
        assert_eq!(
            EngineError::from_invocation(&rpc("PASSWORD_HASH_INVALID", None)),
            EngineError::InvalidPassword
        );
    }

    #[test]
    fn unknown_rpc_errors_use_the_fallback() {
        assert_eq!(
            EngineError::from_invocation(&rpc("PEER_ID_INVALID", None)),
            EngineError::Other
        );
        assert_eq!(
            EngineError::from_invocation_with(
                &rpc("PEER_ID_INVALID", None),
                EngineError::SearchFailed
            ),
            EngineError::SearchFailed
        );
        assert_eq!(
            EngineError::from_invocation(&InvocationError::Io(io::Error::other("boom"))),
            EngineError::Other
        );
    }
}
