//! Watch error types.

use std::fmt;

#[derive(Debug, Eq, PartialEq)]
pub struct NoReceiverError;

impl fmt::Display for NoReceiverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "all receivers were dropped")
    }
}

impl std::error::Error for NoReceiverError {}

#[derive(Debug, Eq, PartialEq)]
pub struct NoSenderError;

impl fmt::Display for NoSenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "sender was dropped")
    }
}

impl std::error::Error for NoSenderError {}
