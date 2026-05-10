// Global keyboard hook (WH_KEYBOARD_LL).
//
// Why a low-level hook (and why in its own thread)?
//
// Win32 routes WH_KEYBOARD_LL callbacks on the thread that registered
// them — and *only* if that thread is pumping messages. If the main UI
// thread were to install the hook and then enter `GetMessage`, every
// keystroke globally would be processed inside our paint pump, which
// would (a) starve UI redraws under fast typing and (b) make the
// callback latency observable as input lag. Instead we spawn a
// dedicated "kc-hook" thread whose entire job is `SetWindowsHookExW` +
// `GetMessage` loop. The callback is intentionally tiny: bump an
// atomic, push into a lock-free MPSC channel, return. Anything heavier
// (timestamp formatting, SQLite, JSON) belongs on the storage thread.
//
// The atomic counter is shared with the UI thread for live KPM
// (it reads a snapshot every 500 ms). The channel goes to the storage
// thread which does the actual aggregation + write.

use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;
use std::thread;

use chrono::Utc;
use crossbeam_channel::Sender;
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::*;

use crate::core::keycode::KeyCode;

/// One key-press event handed to the storage thread.
///
/// Repr: 24 bytes (timestamp + i32 + 4 padding). Cheap to clone, fits
/// in a cache line, and small enough that the channel buffer doesn't
/// dominate memory even under sustained 200+ KPM input.
#[derive(Debug, Clone, Copy)]
pub struct KeyEvent {
    pub timestamp_ms: i64,
    pub code: KeyCode,
}

/// Globals visible to the LL hook callback. The callback is a `static`
/// extern "system" fn with no closure environment — we can't pass our
/// own state in. Once the hook thread starts these are written exactly
/// once before SetWindowsHookExW returns, so the Relaxed ordering on
/// the loads is safe (no later thread sees the writes out of order
/// because the hook callback can't fire before the API call that
/// installs it returns).
static mut TX: Option<Sender<KeyEvent>> = None;
static mut PAUSED: Option<Arc<AtomicBool>> = None;
static mut LIVE_COUNTER: Option<Arc<AtomicI64>> = None;

/// Spawn the hook thread. Returns immediately; the thread runs until
/// the process exits.
pub fn spawn(tx: Sender<KeyEvent>, paused: Arc<AtomicBool>, live_counter: Arc<AtomicI64>) {
    thread::Builder::new()
        .name("kc-hook".into())
        .spawn(move || {
            // SAFETY: `spawn` is called exactly once during application
            // startup, before any keyboard events can possibly fire
            // through our hook (it's not even installed yet). The
            // statics are then read-only for the rest of the process
            // lifetime — the hook thread never touches them after
            // SetWindowsHookExW.
            unsafe {
                TX = Some(tx);
                PAUSED = Some(paused);
                LIVE_COUNTER = Some(live_counter);
            }

            unsafe {
                let hmod = GetModuleHandleW(None).expect("GetModuleHandle failed");
                let hhook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_callback), hmod, 0)
                    .expect("SetWindowsHookExW failed");

                // Standard message pump. The OS dispatches our hook
                // callback into this thread's message loop — we don't
                // need to handle any messages ourselves, but we must
                // not exit GetMessage or the hook stops firing.
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }

                // Best-effort cleanup; only reached if the message loop
                // exits, which currently never happens.
                let _ = UnhookWindowsHookEx(hhook);
            }
        })
        .expect("failed to spawn kc-hook thread");
}

/// The hook callback. Hot path: must stay branchless and allocation-
/// free. Anything more interesting belongs in the storage thread.
unsafe extern "system" fn ll_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    // HC_ACTION (0) is the only code we care about; HC_NOREMOVE etc
    // are filtered ones we should pass through untouched.
    if code != HC_ACTION as i32 {
        return CallNextHookEx(None, code, wparam, lparam);
    }

    // Only count key-down events. Up events would double everything.
    // SYSKEYDOWN fires for Alt-modified presses (Alt+F4 etc.) — we
    // count those too.
    let msg = wparam.0 as u32;
    if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
        // Pause toggle: we still observe the event (so the live counter
        // stays accurate for "did the system see this key?") but we
        // skip the storage push so it doesn't accrue in the database.
        let paused_now = PAUSED
            .as_ref()
            .map(|p| p.load(Ordering::Relaxed))
            .unwrap_or(false);

        if !paused_now {
            // KBDLLHOOKSTRUCT layout: { vkCode, scanCode, flags, time, dwExtraInfo }.
            let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
            let extended = (kb.flags.0 & LLKHF_EXTENDED.0) != 0;
            let kc = KeyCode::from_vk(kb.vkCode, extended);

            if let Some(counter) = LIVE_COUNTER.as_ref() {
                counter.fetch_add(1, Ordering::Relaxed);
            }

            if let Some(tx) = TX.as_ref() {
                // try_send — never block the OS hook thread on a slow
                // consumer. The channel is unbounded, so this only
                // fails if the receiver was dropped (process tearing
                // down). In that case we're about to die anyway.
                let ev = KeyEvent {
                    timestamp_ms: Utc::now().timestamp_millis(),
                    code: kc,
                };
                let _ = tx.try_send(ev);
            }
        }
    }

    // Always pass through. Returning LRESULT(1) here would *swallow*
    // the keystroke globally — a behaviour we never want.
    CallNextHookEx(None, code, wparam, lparam)
}
