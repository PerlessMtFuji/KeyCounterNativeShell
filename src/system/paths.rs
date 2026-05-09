// Per-user storage paths. We follow the Windows Known Folder convention:
// user data lives under %LOCALAPPDATA%\KeyCounter, which is the same
// place the Tauri version writes to (modulo the `io.keycounter.app`
// vendor prefix). This is intentionally NOT the roaming AppData — the
// SQLite database is machine-local and never meant to sync.
//
// Using `std::env::var("LOCALAPPDATA")` instead of SHGetKnownFolderPath
// keeps us off the COM apartment dance for what is, in practice, a
// single environment variable that the OS guarantees to set for any
// interactive session.

use std::path::PathBuf;

pub const APP_DIR_NAME: &str = "KeyCounter";
pub const DB_FILENAME: &str = "keycounter.db";

pub fn data_dir() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        // Fallback: %USERPROFILE%\AppData\Local. Hit this only on
        // exotic / sandboxed sessions where LOCALAPPDATA is unset.
        .or_else(|_| {
            std::env::var("USERPROFILE").map(|p| PathBuf::from(p).join("AppData").join("Local"))
        })
        // Last-ditch: current directory. Better than panicking here
        // — the caller will surface the create_dir_all failure with
        // a meaningful path in the message.
        .unwrap_or_else(|_| PathBuf::from("."));

    base.join(APP_DIR_NAME)
}

pub fn db_path() -> PathBuf {
    data_dir().join(DB_FILENAME)
}

pub fn ensure_data_dir() -> std::io::Result<PathBuf> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
