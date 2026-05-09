// UI layer — Win32 + Direct2D.
//
// Stub for the scaffold push. The full implementation (main window,
// sidebar, render context, all five views, floating widget) lands in
// the follow-up commits — this placeholder keeps the project building
// and exercises the wire-up between `app::run()` and the UI entry
// point so the dependency chain is verified end to end.

use anyhow::Result;
use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONINFORMATION, MB_OK,
};

use crate::app::AppState;

/// Temporary stand-in for the full main window. Currently it shows a
/// placeholder MessageBox confirming the core layer is wired and exits;
/// this lets us verify in CI that the keyboard hook attaches, the
/// storage thread starts, and the SQLite database is created — without
/// blocking on the full Direct2D renderer being ready.
///
/// Replaced by a real WNDPROC + message loop in the next push.
pub fn run_main_window(state: AppState) -> Result<()> {
    log::info!("UI stub: lifetime so far = {} keystrokes",
               state.snapshot.read().lifetime);

    let title: Vec<u16> = "KeyCounter — scaffold\0".encode_utf16().collect();
    let body: Vec<u16> = format!(
        "KeyCounter native scaffold is running.\n\n\
         Database: {}\n\n\
         The core (hook + storage) is active. \
         The full Direct2D UI ships in the next commit.",
        state.db_path.display()
    )
    .encode_utf16()
    .chain(std::iter::once(0))
    .collect();

    unsafe {
        let _ = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(body.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
    Ok(())
}
