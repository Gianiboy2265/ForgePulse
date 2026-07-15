use std::path::PathBuf;

use thiserror::Error;

/// A structured error that can cross crate boundaries without losing context.
#[derive(Debug, Error)]
pub enum ForgeError {
    #[error("configuration is invalid: {0}")]
    InvalidConfiguration(String),
    #[error("input is invalid: {0}")]
    InvalidInput(String),
    #[error("operation is unsupported on this system: {0}")]
    Unsupported(String),
    #[error("permission denied while {operation}: {details}")]
    PermissionDenied { operation: String, details: String },
    #[error("I/O failure at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("storage failed: {0}")]
    Storage(String),
    #[error("collector {collector} failed: {details}")]
    Collector { collector: String, details: String },
    #[error("IPC authentication failed: {0}")]
    Authentication(String),
    #[error("IPC protocol failed: {0}")]
    Protocol(String),
    #[error("service is unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("internal invariant failed: {0}")]
    Invariant(String),
}

impl ForgeError {
    #[must_use]
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }
}

pub type Result<T> = std::result::Result<T, ForgeError>;
