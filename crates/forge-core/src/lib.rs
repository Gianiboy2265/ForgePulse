//! Stable domain types shared by every ForgePulse process.

pub mod config;
pub mod error;
pub mod ipc;
pub mod metrics;
pub mod safety;

pub use error::{ForgeError, Result};

/// Version of the signed local IPC protocol.
pub const IPC_PROTOCOL_VERSION: u16 = 1;
/// Maximum accepted IPC payload, excluding its four-byte length prefix.
pub const MAX_IPC_FRAME_BYTES: usize = 1024 * 1024;
/// Stable local named-pipe address for protocol version one.
pub const DEFAULT_PIPE_NAME: &str = r"\\.\pipe\LOCAL\forgepulse-v1";
