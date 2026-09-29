//! Structured engine errors.
//!
//! The engine never builds user-facing English strings. It reports a
//! [`EngineError`] variant and lets the panel render localized text.

use grammers_client::InvocationError;

/// A failure the engine can report to the panel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EngineError {
    NotConfigured,
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
    /// Maps a grammers invocation failure, preserving `FLOOD_WAIT` seconds.
    pub fn from_invocation(error: &InvocationError) -> Self {
        match flood_wait_seconds(error) {
            Some(seconds) => Self::FloodWait { seconds },
            None => Self::Other,
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
    fn other_rpc_errors_are_generic() {
        assert_eq!(
            EngineError::from_invocation(&rpc("PHONE_CODE_INVALID", None)),
            EngineError::Other
        );
        assert_eq!(
            EngineError::from_invocation(&InvocationError::Io(io::Error::other("boom"))),
            EngineError::Other
        );
    }
}
