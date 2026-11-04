//! Typed error classification for database operations.
//!
//! Errors are classified by kind so the UI can select an i18n message by kind,
//! never by matching on the driver's error text (invariant: typed errors).

use thiserror::Error;

/// The category of a database failure. The UI maps each variant to a localized
/// message; driver-specific wording stays in the `message` field for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DatabaseErrorKind {
    /// The connection could not be established (file missing, host unreachable).
    Connection,
    /// Authentication was rejected (bad password or token).
    Authentication,
    /// The SQL text could not be parsed or prepared.
    Syntax,
    /// The statement referenced a missing object or column.
    NotFound,
    /// The account lacks permission for the operation.
    Permission,
    /// The operation was cancelled by the user.
    Cancelled,
    /// A constraint (unique, foreign key, not-null) was violated.
    Constraint,
    /// The operation timed out.
    Timeout,
    /// The driver or protocol misbehaved.
    Driver,
    /// Any error that does not fit a more specific category.
    Other,
}

/// A classified database error. `kind` drives UI messaging; `message` is a
/// human-readable diagnostic that must never contain credentials.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{kind:?}: {message}")]
pub struct DatabaseError {
    pub kind: DatabaseErrorKind,
    pub message: String,
}

impl DatabaseError {
    pub fn new(kind: DatabaseErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn connection(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Connection, message)
    }

    pub fn authentication(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Authentication, message)
    }

    pub fn syntax(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Syntax, message)
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::NotFound, message)
    }

    pub fn permission(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Permission, message)
    }

    pub fn cancelled() -> Self {
        Self::new(DatabaseErrorKind::Cancelled, "cancelled".to_owned())
    }

    pub fn constraint(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Constraint, message)
    }

    pub fn timeout(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Timeout, message)
    }

    pub fn driver(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Driver, message)
    }

    pub fn other(message: impl Into<String>) -> Self {
        Self::new(DatabaseErrorKind::Other, message)
    }

    pub fn is_cancelled(&self) -> bool {
        self.kind == DatabaseErrorKind::Cancelled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_are_stable_and_serializable() -> anyhow::Result<()> {
        let error = DatabaseError::authentication("bad password");
        assert_eq!(error.kind, DatabaseErrorKind::Authentication);
        assert!(!error.is_cancelled());
        let encoded = serde_json::to_string(&error.kind)?;
        assert_eq!(encoded, "\"authentication\"");
        Ok(())
    }

    #[test]
    fn cancelled_is_its_own_kind() {
        assert!(DatabaseError::cancelled().is_cancelled());
    }
}
