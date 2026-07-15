use std::path::Path;

const PROTECTED_EXECUTABLES: &[&str] = &[
    "csrss.exe",
    "dwm.exe",
    "fontdrvhost.exe",
    "lsass.exe",
    "lsaiso.exe",
    "services.exe",
    "smss.exe",
    "svchost.exe",
    "system",
    "wininit.exe",
    "winlogon.exe",
];

/// Conservative identity check used before any future process mutation. A matching
/// name is protected only when its canonical parent is inside the canonical Windows
/// directory; callers must reject targets whose path cannot be canonicalized.
#[must_use]
pub fn is_protected_windows_process(executable: &Path, windows_directory: &Path) -> bool {
    let Some(name) = executable.file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    if !PROTECTED_EXECUTABLES
        .iter()
        .any(|protected| name.eq_ignore_ascii_case(protected))
    {
        return false;
    }
    let executable = match executable.canonicalize() {
        Ok(value) => value,
        Err(_) => return true,
    };
    let windows_directory = match windows_directory.canonicalize() {
        Ok(value) => value,
        Err(_) => return true,
    };
    executable.starts_with(windows_directory)
}
