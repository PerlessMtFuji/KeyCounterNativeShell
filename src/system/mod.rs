// System integrations — anything that talks to the OS outside of the
// GUI message loop. Kept in one place so the UI layer can pretend the
// rest of Windows doesn't exist.

pub mod message_box;
pub mod paths;
// Wired in later pushes:
// pub mod tray;
// pub mod autostart;
// pub mod export;
