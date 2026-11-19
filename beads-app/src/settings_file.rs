//! Where BunchABeads.ini lives, and loading / saving it.
//!
//! * If a `BunchABeads.ini` sits next to the program, that one is used
//!   ("portable" mode, like the 2013 version).
//! * Otherwise it lives in the normal per-user settings folder:
//!   Windows `%APPDATA%\BunchABeads\`, macOS `~/Library/Application Support/BunchABeads/`,
//!   Linux `~/.config/BunchABeads/`.
//! * On first run, an old 2013 `BaBPro.ini` is imported if one is found.

use beads_core::Settings;
use std::path::{Path, PathBuf};

pub const FILE_NAME: &str = "BunchABeads.ini";

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe().ok()?.parent().map(Path::to_path_buf)
}

#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn user_config_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let base = if cfg!(windows) {
        env("APPDATA")?
    } else if cfg!(target_os = "macos") {
        env("HOME")?.join("Library").join("Application Support")
    } else {
        env("XDG_CONFIG_HOME").or_else(|| env("HOME").map(|h| h.join(".config")))?
    };
    Some(base.join("BunchABeads"))
}

pub fn settings_path() -> PathBuf {
    if crate::platform::IS_WEB {
        // Kept in the browser's own storage.
        return PathBuf::from("/BunchABeads").join(FILE_NAME);
    }
    if let Some(p) = exe_dir().map(|d| d.join(FILE_NAME)).filter(|p| p.is_file()) {
        return p;
    }
    user_config_dir().or_else(exe_dir).unwrap_or_default().join(FILE_NAME)
}

/// Places an old 2013 BaBPro.ini might be.
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
fn legacy_candidates() -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = exe_dir().into_iter().map(|d| d.join("BaBPro.ini")).collect();
    if cfg!(windows) {
        v.push(PathBuf::from(r"C:\Bunch-a-BeadsPatternDesignPro\BaBPro.ini"));
    }
    v
}

/// Loads settings; returns them and a message for the status bar.
/// (Web version: from the browser's storage, with the built-in folders.)
#[cfg(target_arch = "wasm32")]
pub fn load(path: &Path) -> (Settings, String) {
    let mut s = crate::platform::read_to_string(path)
        .map(|t| Settings::from_text(&t))
        .unwrap_or_default();
    crate::platform::web_folders(&mut s);
    (
        s,
        "Welcome to the web version! Your designs and settings are kept in this browser.".into(),
    )
}

/// Loads settings; returns them and a message for the status bar.
#[cfg(not(target_arch = "wasm32"))]
pub fn load(path: &Path) -> (Settings, String) {
    if let Ok(text) = crate::platform::read_to_string(path) {
        return (Settings::from_text(&text), format!("Settings: {}", path.display()));
    }
    for old in legacy_candidates() {
        if let Ok(text) = crate::platform::read_to_string(&old) {
            return (
                Settings::from_text(&text),
                format!("Imported 2013 settings from {}", old.display()),
            );
        }
    }
    (
        Settings::default(),
        "Welcome! Choose your Beads folder to begin (Bead Folder… button).".into(),
    )
}

pub fn save(path: &Path, settings: &Settings) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        crate::platform::create_dir_all(dir)?;
    }
    crate::platform::write(path, settings.to_ini())
}
