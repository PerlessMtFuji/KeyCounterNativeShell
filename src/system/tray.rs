// Tray icon — Shell_NotifyIconW + popup menu.
//
// The tray icon's callback message arrives at the host HWND as
// `WM_TRAY_CALLBACK` (defined here, used by the main window). The
// lParam's low word is the mouse event the user produced; the high
// word is the icon id (we only ever use one, so it's redundant).
//
// Menu items are dispatched via WM_COMMAND on the host HWND. The IDs
// live in this module so the main window can match against them
// without redefining the constants.

use std::os::windows::ffi::OsStrExt;

use anyhow::{anyhow, Context, Result};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, POINT};
use windows::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NOTIFYICONDATAW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, DestroyMenu, GetCursorPos, LoadIconW, SetForegroundWindow,
    TrackPopupMenu, HMENU, IDI_APPLICATION, MF_CHECKED, MF_SEPARATOR, MF_STRING, MF_UNCHECKED,
    TPM_BOTTOMALIGN, TPM_RIGHTALIGN,
};

/// The Win32 user-defined message id we pick for the tray callback.
/// Win32 reserves WM_USER..0xBFFF for app-defined messages on a given
/// window — we only need one.
pub const WM_TRAY_CALLBACK: u32 = windows::Win32::UI::WindowsAndMessaging::WM_USER + 1;

/// Tray icon id. We register exactly one icon per process; the field is
/// part of `NOTIFYICONDATAW` so the OS can disambiguate multi-icon apps.
pub const TRAY_UID: u32 = 1;

// Menu item identifiers. Kept here so wnd_proc's WM_COMMAND switch can
// import the same names.
pub const ID_SHOW: u16 = 100;
pub const ID_TOGGLE_PAUSE: u16 = 101;
pub const ID_TOGGLE_WIDGET: u16 = 102;
pub const ID_QUIT: u16 = 199;

fn wide_z(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn build_data(hwnd: HWND, tip: &str) -> NOTIFYICONDATAW {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY_CALLBACK,
        ..Default::default()
    };
    // hIcon — fall back to the system "application" icon if our own
    // resource is absent (the .rc bundles one but the dev build may
    // skip resource compilation).
    let hicon = unsafe { LoadIconW(None, IDI_APPLICATION) }
        .unwrap_or_default();
    data.hIcon = hicon;

    // szTip is a fixed-size [u16; 128]. Truncate if needed.
    let tip_w = wide_z(tip);
    let n = tip_w.len().min(data.szTip.len());
    data.szTip[..n].copy_from_slice(&tip_w[..n]);
    data
}

pub fn install(hwnd: HWND, tip: &str) -> Result<()> {
    let mut data = build_data(hwnd, tip);
    let ok = unsafe { Shell_NotifyIconW(NIM_ADD, &mut data) };
    if !ok.as_bool() {
        return Err(anyhow!("Shell_NotifyIconW(NIM_ADD) returned FALSE"));
    }
    Ok(())
}

pub fn remove(hwnd: HWND) -> Result<()> {
    let mut data = NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    };
    let ok = unsafe { Shell_NotifyIconW(NIM_DELETE, &mut data) };
    if !ok.as_bool() {
        return Err(anyhow!("Shell_NotifyIconW(NIM_DELETE) returned FALSE"));
    }
    Ok(())
}

pub fn update_tip(hwnd: HWND, tip: &str) {
    let mut data = build_data(hwnd, tip);
    let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &mut data) };
}

/// Show the popup menu at the current cursor position. Selecting an
/// item posts a WM_COMMAND with the matching ID to `hwnd`.
pub fn show_menu(hwnd: HWND, paused: bool, widget_visible: bool) -> Result<()> {
    unsafe {
        let menu: HMENU = CreatePopupMenu().context("CreatePopupMenu")?;
        let show_label = wide_z("Show window");
        let pause_label = wide_z(if paused { "Resume counting" } else { "Pause counting" });
        let widget_label = wide_z(if widget_visible { "Hide widget" } else { "Show widget" });
        let quit_label = wide_z("Quit");

        let _ = AppendMenuW(menu, MF_STRING, ID_SHOW as usize, PCWSTR(show_label.as_ptr()));
        let _ = AppendMenuW(
            menu,
            MF_STRING | if paused { MF_CHECKED } else { MF_UNCHECKED },
            ID_TOGGLE_PAUSE as usize,
            PCWSTR(pause_label.as_ptr()),
        );
        let _ = AppendMenuW(
            menu,
            MF_STRING | if widget_visible { MF_CHECKED } else { MF_UNCHECKED },
            ID_TOGGLE_WIDGET as usize,
            PCWSTR(widget_label.as_ptr()),
        );
        let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        let _ = AppendMenuW(menu, MF_STRING, ID_QUIT as usize, PCWSTR(quit_label.as_ptr()));

        // Microsoft KB Q135788: the popup must be triggered with the
        // caller window foregrounded, otherwise the menu disappears
        // when the user clicks elsewhere instead of selecting an item.
        let _ = SetForegroundWindow(hwnd);

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = TrackPopupMenu(
            menu,
            TPM_BOTTOMALIGN | TPM_RIGHTALIGN,
            pt.x,
            pt.y,
            0,
            hwnd,
            None,
        );

        // Same KB note: post a dummy WM_NULL to flush the menu state.
        let _ = windows::Win32::UI::WindowsAndMessaging::PostMessageW(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_NULL,
            windows::Win32::Foundation::WPARAM(0),
            windows::Win32::Foundation::LPARAM(0),
        );

        let _ = DestroyMenu(menu);
    }
    Ok(())
}
