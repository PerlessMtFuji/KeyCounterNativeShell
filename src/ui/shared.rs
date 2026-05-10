// Thread-local handles shared across the per-window WNDPROCs.
//
// Both the main window and the floating widget live on the UI thread,
// so we cheaply share their HWNDs through TLS instead of plumbing
// pointers between modules. The fields are populated as each window
// is created and read by the other when it needs to ask "is the widget
// visible right now?" / "show me the main window" etc.

use std::cell::Cell;

use windows::Win32::Foundation::HWND;

thread_local! {
    /// HWND of the live floating widget, if any.
    pub static WIDGET_HWND: Cell<Option<isize>> = const { Cell::new(None) };
    /// Cached visibility of the widget — `IsWindowVisible` would also
    /// work but TLS is cheaper inside the WM_COMMAND switch and we're
    /// the only writers.
    pub static WIDGET_VISIBLE: Cell<bool> = const { Cell::new(true) };
}

pub fn set_widget_hwnd(hwnd: HWND) {
    WIDGET_HWND.with(|c| c.set(Some(hwnd.0 as isize)));
}

pub fn widget_hwnd() -> Option<HWND> {
    WIDGET_HWND
        .with(|c| c.get())
        .map(|i| HWND(i as *mut core::ffi::c_void))
}

pub fn is_widget_visible() -> bool {
    WIDGET_VISIBLE.with(|c| c.get())
}

pub fn set_widget_visible(v: bool) {
    WIDGET_VISIBLE.with(|c| c.set(v));
}
