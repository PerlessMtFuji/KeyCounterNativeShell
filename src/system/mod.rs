// System integrations — anything that talks to the OS outside of the
// GUI message loop. Kept in one place so the UI layer can pretend the
// rest of Windows doesn't exist.

pub mod autostart;
pub mod export;
pub mod message_box;
pub mod paths;
pub mod settings_io;
pub mod tray;
