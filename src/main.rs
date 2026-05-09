// KeyCounter NativeShell — entry point.
//
// All real work lives in the `app` module. This file only:
//   1. initialises the tracing logger
//   2. installs a panic hook that surfaces panics as MessageBox so
//      release-built users actually see the failure instead of the
//      .exe vanishing silently
//   3. hands control to `app::run()`
//
// The `windows_subsystem` attribute hides the console window on
// release builds. Debug builds keep the console so `println!` and
// `env_logger` output are visible during development.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod core;
mod system;
mod ui;

use std::panic;

fn main() {
    init_logging();
    install_panic_hook();

    if let Err(e) = app::run() {
        // Last-ditch error surface. `app::run` should handle its own
        // recoverable failures; reaching this point means startup
        // itself collapsed (e.g. the database directory was not
        // creatable, or the window class registration failed).
        log::error!("KeyCounter exited with error: {e:#}");
        crate::system::message_box::error("KeyCounter — startup failure", &format!("{e:#}"));
        std::process::exit(1);
    }
}

fn init_logging() {
    // env_logger is gated behind RUST_LOG so a release-built user
    // doesn't pay the cost of formatting log lines that go nowhere.
    // For diagnostics in the field we instruct users to set
    // `RUST_LOG=keycounter=debug` and re-launch — see docs/antivirus.md.
    let _ = env_logger::try_init();
}

fn install_panic_hook() {
    // Default hook prints to stderr — useless under WINDOWS_SUBSYSTEM
    // where stderr is closed. We chain it (so debug builds still get
    // the console output) and additionally show a MessageBox.
    let prev = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        prev(info);
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "(non-string panic payload)".to_string()
        };
        let location = info
            .location()
            .map(|l| format!("{}:{}", l.file(), l.line()))
            .unwrap_or_else(|| "<unknown>".to_string());
        let msg = format!("KeyCounter panicked at {location}\n\n{payload}");
        crate::system::message_box::error("KeyCounter — internal error", &msg);
    }));
}
