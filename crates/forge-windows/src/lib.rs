//! Windows API adapters.
//!
//! Every unsafe block in this crate is a narrow FFI boundary around a documented
//! Win32 function. Handles are wrapped immediately so early returns cannot leak.

#[cfg(windows)]
mod collector;

#[cfg(windows)]
pub use collector::WindowsCollector;

#[cfg(not(windows))]
#[derive(Debug)]
pub struct WindowsCollector;

#[cfg(not(windows))]
impl WindowsCollector {
    pub fn new(_process_limit: usize, _collect_paths: bool) -> forge_core::Result<Self> {
        Err(forge_core::ForgeError::Unsupported(
            "the initial collector requires Windows".to_owned(),
        ))
    }
}
