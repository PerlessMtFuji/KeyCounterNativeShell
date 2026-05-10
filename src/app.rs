// Application bootstrap and shared state.
//
// `run()` wires the layers together:
//
//   1. Open the SQLite store under %LOCALAPPDATA%\KeyCounter.
//   2. Load persisted settings.
//   3. Spawn the storage writer thread (drains the hook channel).
//   4. Spawn the keyboard hook thread.
//   5. Take the first DataSnapshot so the UI has something to render
//      on its first paint.
//   6. Hand off to the UI layer (`ui::run_main_window`) which owns the
//      Win32 message loop and never returns until WM_QUIT.
//
// `AppState` is the one piece of shared mutable state. Cheap Arc clones
// hand it to background threads (hook, storage) and to the UI. Inside
// it we use parking_lot RwLock for the snapshot (read-many, write-rare)
// and atomics for the hot counters that the hook callback touches.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use crossbeam_channel::unbounded;
use parking_lot::RwLock;

use crate::core::hook::{self, KeyEvent};
use crate::core::stats::{DataSnapshot, PulseHistory};
use crate::core::store::Store;
use crate::system::paths;

/// Shared application state. Cloned into every thread + the message
/// loop. The Arcs add about 10 ns of indirection on each access — well
/// below anything we care about.
#[derive(Clone)]
pub struct AppState {
    pub store: Store,
    pub db_path: PathBuf,

    /// Pause toggle — read by the hook callback on every event. When
    /// true the callback observes the keystroke (so the live counter
    /// stays accurate for the visible "you pressed something" feedback)
    /// but skips the channel push.
    pub paused: Arc<AtomicBool>,

    /// Lifetime-of-process keystroke counter. The hook bumps it; the
    /// 500 ms tick reads it and computes the delta against the previous
    /// reading. We deliberately don't reset on day boundaries — the
    /// pulse history takes care of windowing.
    pub live_counter: Arc<AtomicI64>,

    /// Last-known data slice for the UI. Refreshed every 5 s while a
    /// window is visible.
    pub snapshot: Arc<RwLock<DataSnapshot>>,

    /// 60 s sliding window of keystroke deltas → instant KPM.
    pub pulse: Arc<RwLock<PulseHistory>>,
}

impl AppState {
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn set_paused(&self, value: bool) {
        self.paused.store(value, Ordering::Relaxed);
    }

    pub fn toggle_paused(&self) -> bool {
        let next = !self.is_paused();
        self.set_paused(next);
        next
    }

    /// Convenience: open a fresh read-only DB connection. Anyone
    /// holding the AppState can ask for one without having to know the
    /// path. Returns an error when the database is missing or
    /// inaccessible — callers typically just propagate.
    pub fn reader(&self) -> rusqlite::Result<rusqlite::Connection> {
        self.store.reader()
    }
}

/// Application entry point. Returns when WM_QUIT propagates out of the
/// message loop. The error case here is reserved for startup failures
/// the user can do something about — missing data dir, corrupt DB, etc.
pub fn run() -> Result<()> {
    let data_dir = paths::ensure_data_dir().context("create %LOCALAPPDATA%\\KeyCounter")?;
    log::info!("data dir: {}", data_dir.display());

    // Load persisted settings before opening the DB — the UI thread
    // reads SETTINGS the moment the main window appears.
    {
        let s = crate::system::settings_io::load();
        *crate::core::i18n::SETTINGS.write() = s;
    }

    let db_path = paths::db_path();
    let store = Store::open(&db_path).with_context(|| format!("open SQLite at {}", db_path.display()))?;

    let paused = Arc::new(AtomicBool::new(false));
    let live_counter = Arc::new(AtomicI64::new(0));

    // MPSC: hook → storage. Unbounded so the hook callback can never
    // block on the OS message thread, even under burst input. The
    // memory cost of an unbounded queue is bounded in practice by the
    // 3 s flush window — at 200 KPM that's ~10 events buffered, never
    // a runaway.
    let (tx, rx) = unbounded::<KeyEvent>();
    store.spawn_writer(rx);
    hook::spawn(tx, paused.clone(), live_counter.clone());

    // Take a first snapshot so the UI doesn't paint a flash of empty
    // cards before the first 5 s timer fires.
    let initial = match store.reader() {
        Ok(conn) => DataSnapshot::refresh_full(&conn),
        Err(e) => {
            log::warn!("initial snapshot read failed: {e}");
            DataSnapshot::default()
        }
    };

    let state = AppState {
        store,
        db_path,
        paused,
        live_counter,
        snapshot: Arc::new(RwLock::new(initial)),
        pulse: Arc::new(RwLock::new(PulseHistory::default())),
    };

    crate::ui::run_main_window(state)?;

    // Persist settings on clean exit. Failures here aren't fatal — the
    // user will see today's tweaks come back as defaults on next launch,
    // which is the worst-case behaviour we're already covering.
    let snapshot = crate::core::i18n::SETTINGS.read().clone();
    if let Err(e) = crate::system::settings_io::save(&snapshot) {
        log::warn!("settings_io::save failed: {e}");
    }
    Ok(())
}
