// UI layer — Win32 + Direct2D.
//
// This module owns everything visible: the main window, its WNDPROC,
// the immediate-mode controls, and the per-view renderers. Everything
// upstream of this module is stateless render data (snapshots, palette,
// settings) — the UI just paints whatever the current AppState looks
// like at this very instant.

pub mod controls;
pub mod floating_widget;
pub mod live_pulse;
pub mod main_window;
pub mod render;
pub mod shared;
pub mod sidebar;
pub mod views;

use anyhow::Result;

use crate::app::AppState;

/// Entry point invoked by `app::run()`. Hands off to the main window
/// loop and returns when WM_QUIT propagates out of GetMessageW.
pub fn run_main_window(state: AppState) -> Result<()> {
    main_window::run(state)
}
