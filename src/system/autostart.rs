// Autostart via HKCU\Software\Microsoft\Windows\CurrentVersion\Run.
//
// HKCU (not HKLM) so we don't need admin to flip the setting, and the
// entry survives uninstall-as-other-user. The value name is the same
// as the brand ("KeyCounter") and the data is the full path to the
// running .exe in quotes — quotes matter when the path has spaces.

use anyhow::{anyhow, Context, Result};
use std::os::windows::ffi::OsStrExt;
use windows::core::PCWSTR;
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_SET_VALUE, REG_SZ,
};

const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const ENTRY_NAME: &str = "KeyCounter";

fn wide(s: &str) -> Vec<u16> {
    std::ffi::OsStr::new(s)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

fn open(access: windows::Win32::System::Registry::REG_SAM_FLAGS) -> Result<HKEY> {
    let key_w = wide(RUN_KEY);
    let mut hkey = HKEY::default();
    let st = unsafe {
        RegOpenKeyExW(
            HKEY_CURRENT_USER,
            PCWSTR(key_w.as_ptr()),
            0,
            access,
            &mut hkey,
        )
    };
    if st.is_err() {
        return Err(anyhow!("RegOpenKeyExW Run failed: {:?}", st));
    }
    Ok(hkey)
}

/// Returns true if the autostart entry exists. Doesn't validate the
/// path — the user could have deleted the binary and we'd still report
/// "enabled". That's a feature: we shouldn't silently flip the toggle
/// off behind the user's back on every launch.
pub fn is_enabled() -> bool {
    let hkey = match open(KEY_READ) {
        Ok(h) => h,
        Err(_) => return false,
    };
    let name_w = wide(ENTRY_NAME);
    let mut len: u32 = 0;
    let st = unsafe {
        RegQueryValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            None,
            None,
            None,
            Some(&mut len),
        )
    };
    let exists = !st.is_err() || st.0 != ERROR_FILE_NOT_FOUND.0;
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    exists && len > 0
}

/// Set the Run entry to launch the currently-running executable.
pub fn enable() -> Result<()> {
    let exe = std::env::current_exe().context("current_exe")?;
    // Wrap in quotes so spaces in the path don't truncate the value.
    let value = format!("\"{}\"", exe.display());
    let value_w: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    let hkey = open(KEY_SET_VALUE)?;
    let name_w = wide(ENTRY_NAME);
    // SAFETY: byte length includes the trailing NUL.
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(
            value_w.as_ptr() as *const u8,
            value_w.len() * std::mem::size_of::<u16>(),
        )
    };
    let st = unsafe {
        RegSetValueExW(
            hkey,
            PCWSTR(name_w.as_ptr()),
            0,
            REG_SZ,
            Some(bytes),
        )
    };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if st.is_err() {
        return Err(anyhow!("RegSetValueExW failed: {:?}", st));
    }
    Ok(())
}

/// Remove the Run entry. Missing entry is a success — we want the
/// toggle to be idempotent.
pub fn disable() -> Result<()> {
    let hkey = open(KEY_SET_VALUE)?;
    let name_w = wide(ENTRY_NAME);
    let st = unsafe { RegDeleteValueW(hkey, PCWSTR(name_w.as_ptr())) };
    unsafe {
        let _ = RegCloseKey(hkey);
    }
    if st.is_err() && st.0 != ERROR_FILE_NOT_FOUND.0 {
        return Err(anyhow!("RegDeleteValueW failed: {:?}", st));
    }
    Ok(())
}

pub fn apply(enabled: bool) -> Result<()> {
    if enabled {
        enable()
    } else {
        disable()
    }
}
