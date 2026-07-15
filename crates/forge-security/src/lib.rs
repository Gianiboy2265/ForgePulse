mod authentication;
mod framing;
mod protected;

pub use authentication::{AuthenticationGuard, RequestSigner, SecretKey};
pub use framing::{read_frame, write_frame};
pub use protected::is_protected_windows_process;

#[cfg(windows)]
mod client;
#[cfg(windows)]
pub use client::AuthenticatedClient;
