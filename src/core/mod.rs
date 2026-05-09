// Core domain — everything below the UI line.
//
// The cardinal rule: nothing in `core` may know about HWNDs, Direct2D,
// or the windows-rs UI modules. The keyboard hook is the one
// concession (it's an OS callback by necessity) but it's wrapped in a
// way the rest of the module can be unit-tested headlessly.

pub mod achievements;
pub mod finger_map;
pub mod hook;
pub mod i18n;
pub mod keycode;
pub mod layouts;
pub mod stats;
pub mod store;
pub mod theme;
