//! Where the game keeps its files.

use std::path::PathBuf;

const APP: &str = "beyond-the-stars";

/// Per-user data directory: `$BTS_HOME`, else the platform's usual place.
pub fn data_dir() -> PathBuf {
    if let Some(p) = std::env::var_os("BTS_HOME") {
        return PathBuf::from(p);
    }
    let base = if cfg!(target_os = "windows") {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Application Support"))
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
    };
    base.unwrap_or_else(|| PathBuf::from(".")).join(APP)
}

pub fn saves_dir() -> PathBuf {
    data_dir().join("saves")
}

pub fn settings_file() -> PathBuf {
    data_dir().join("settings.json")
}

/// The main-menu background page: next to the working directory's `assets/`, or beside the
/// executable.
pub fn menu_background() -> Option<PathBuf> {
    let rel = PathBuf::from("assets/menu/background.html");
    let mut candidates = vec![rel.clone()];
    if let Ok(exe) = std::env::current_exe()
        && let Some(dir) = exe.parent()
    {
        candidates.push(dir.join(&rel));
        // `target/debug/game` -> project root.
        if let Some(root) = dir.parent().and_then(|p| p.parent()) {
            candidates.push(root.join(&rel));
        }
    }
    candidates.into_iter().find(|p| p.is_file())
}
