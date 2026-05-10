// JSON export — `Save As` dialog + write to disk.
//
// Uses the classic `GetSaveFileNameW` common dialog because IFileSaveDialog
// requires a working COM apartment plus a lot more boilerplate, and the
// classic dialog is still the default UX on Windows 10/11 for non-shell
// pickers. The default filename uses today's date — easy to keep
// multiple exports straight when the user runs this monthly.

use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::Utc;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Controls::Dialogs::{
    GetSaveFileNameW, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

use crate::app::AppState;

/// Show the Save dialog and, on confirm, write the export JSON to the
/// chosen path. Returns Ok(None) when the user cancels.
pub fn save_dialog(owner: HWND, state: &AppState) -> Result<Option<PathBuf>> {
    let default_name = format!(
        "keycounter-{}.json",
        Utc::now().format("%Y-%m-%d")
    );

    // The filename buffer must be large enough for any plausible path.
    // 1024 W-chars (~2 KB) is the usual size; the dialog truncates
    // longer paths but we won't hit that on a desktop.
    let mut buf: Vec<u16> = Vec::with_capacity(1024);
    buf.extend(default_name.encode_utf16());
    buf.resize(1024, 0);

    // `lpstrFilter` is a sequence of NUL-terminated strings, terminated
    // by an extra NUL — encode manually.
    let filter: Vec<u16> = "JSON (*.json)\0*.json\0All files (*.*)\0*.*\0\0"
        .encode_utf16()
        .collect();
    let default_ext: Vec<u16> = "json\0".encode_utf16().collect();
    let title: Vec<u16> = "Export KeyCounter data\0".encode_utf16().collect();

    let mut ofn = OPENFILENAMEW {
        lStructSize: std::mem::size_of::<OPENFILENAMEW>() as u32,
        hwndOwner: owner,
        lpstrFilter: PCWSTR(filter.as_ptr()),
        nFilterIndex: 1,
        lpstrFile: windows::core::PWSTR(buf.as_mut_ptr()),
        nMaxFile: buf.len() as u32,
        lpstrTitle: PCWSTR(title.as_ptr()),
        lpstrDefExt: PCWSTR(default_ext.as_ptr()),
        Flags: OFN_OVERWRITEPROMPT | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR,
        ..Default::default()
    };

    let ok = unsafe { GetSaveFileNameW(&mut ofn) };
    if !ok.as_bool() {
        // User cancelled — distinguish from real errors via
        // CommDlgExtendedError, but we don't need to surface that here.
        return Ok(None);
    }

    // Find the C-string terminator and slice.
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let path = String::from_utf16_lossy(&buf[..len]);
    let path = PathBuf::from(path);

    // Pull the JSON tree out of the store and write it.
    let conn = state.store.reader().context("open db reader for export")?;
    let json = crate::core::store::export_json(&conn).context("export_json")?;
    let serialized = serde_json::to_string_pretty(&json).context("serialize export json")?;
    std::fs::write(&path, serialized).with_context(|| format!("write {}", path.display()))?;
    Ok(Some(path))
}
