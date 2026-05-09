// Thin wrapper over MessageBoxW. The panic hook and the early-startup
// failure paths use this — both run before any of our window machinery
// exists, so there's no parent HWND and we deliberately use NULL.
//
// Keeping this in its own file (rather than inlined into main) means
// it stays callable from contexts that don't yet have an `app` state,
// which is exactly when surfacing errors matters most.

use windows::core::PCWSTR;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONINFORMATION, MB_OK, MB_TOPMOST,
};

fn to_utf16(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn error(title: &str, body: &str) {
    let title_w = to_utf16(title);
    let body_w = to_utf16(body);
    unsafe {
        // MB_TOPMOST so the dialog isn't hidden behind a fullscreen
        // game window — without it, a panic during startup with a game
        // in foreground would look like a silent hang.
        let _ = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(body_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_OK | MB_ICONERROR | MB_TOPMOST,
        );
    }
}

#[allow(dead_code)]
pub fn info(title: &str, body: &str) {
    let title_w = to_utf16(title);
    let body_w = to_utf16(body);
    unsafe {
        let _ = MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(body_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}
