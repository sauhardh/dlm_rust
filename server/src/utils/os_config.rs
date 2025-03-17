use std::env;
use std::path::PathBuf;

#[allow(dead_code)]
/// Returns the download directory of the Linux OS.
///
/// Example: $HOME/Downloads
#[cfg(target_os = "linux")]
pub fn os_download_dir() -> PathBuf {
    env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join("Downloads"))
        .expect("Could not parse the root download directory")
}

/// Returns the download directory of the Window OS.
///
/// Example: USERPROFILE\Downloads
#[cfg(target_os = "windows")]
pub fn os_download_dir() -> PathBuf {
    env::var("USERPROFILE")
        .ok()
        .map(|home| PathBuf::from(home).join("Downloads"))
        .expect("Could not parse the root download directory")
}

/// Returns the download directory of the MacOS.
///
/// Example: $HOME/Downloads
#[cfg(target_os = "macos")]
pub fn os_download_dir() -> PathBuf {
    env::var("HOME")
        .ok()
        .map(|home| PathBuf::from(home).join("Downloads"))
        .expect("Could not parse the root download directory")
}
