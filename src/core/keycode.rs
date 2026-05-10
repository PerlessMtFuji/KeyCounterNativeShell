// Stable, cross-version key codes. Same i32 enum values as the Tauri
// version, by design — anyone migrating will need different schema
// (we picked a different DB file format), but the *meaning* of code 14
// being "E" is preserved so the existing layout / heatmap / finger-map
// code in the original repo can be ported across without remapping.
//
// We translate Win32 virtual-key codes (VK_*) into this enum at the
// hook boundary. Anything we don't recognise becomes Other (999) so
// it's at least counted in the lifetime total without polluting per-
// key tables with platform-specific noise.

use serde::{Deserialize, Serialize};
use windows::Win32::UI::Input::KeyboardAndMouse::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(i32)]
pub enum KeyCode {
    A = 10, B = 11, C = 12, D = 13, E = 14, F = 15, G = 16, H = 17, I = 18,
    J = 19, K = 20, L = 21, M = 22, N = 23, O = 24, P = 25, Q = 26, R = 27,
    S = 28, T = 29, U = 30, V = 31, W = 32, X = 33, Y = 34, Z = 35,

    Num0 = 40, Num1 = 41, Num2 = 42, Num3 = 43, Num4 = 44,
    Num5 = 45, Num6 = 46, Num7 = 47, Num8 = 48, Num9 = 49,

    F1 = 50, F2 = 51, F3 = 52, F4 = 53, F5 = 54, F6 = 55,
    F7 = 56, F8 = 57, F9 = 58, F10 = 59, F11 = 60, F12 = 61,

    ShiftLeft = 80, ShiftRight = 81,
    ControlLeft = 82, ControlRight = 83,
    AltLeft = 84, AltRight = 85,
    MetaLeft = 86, MetaRight = 87,
    CapsLock = 88, FunctionKey = 89,

    Up = 90, Down = 91, Left = 92, Right = 93,
    Home = 94, End = 95, PageUp = 96, PageDown = 97,
    Insert = 98, Delete = 99,

    Backspace = 100, Tab = 101, Return = 102, Space = 103, Escape = 104,

    BackQuote = 110, Minus = 111, Equal = 112,
    LeftBracket = 113, RightBracket = 114, BackSlash = 115,
    Semicolon = 116, Quote = 117, Comma = 118, Dot = 119, Slash = 120,
    IntlBackslash = 121, IntlRo = 122,

    Kp0 = 130, Kp1 = 131, Kp2 = 132, Kp3 = 133, Kp4 = 134,
    Kp5 = 135, Kp6 = 136, Kp7 = 137, Kp8 = 138, Kp9 = 139,
    KpMinus = 140, KpPlus = 141, KpMultiply = 142, KpDivide = 143,
    KpReturn = 144, KpDelete = 145,

    PrintScreen = 150, ScrollLock = 151, Pause = 152, NumLock = 153,

    Other = 999,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModifierGroup {
    Shift,
    Ctrl,
    Alt,
    Meta,
}

impl ModifierGroup {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Shift => "shift",
            Self::Ctrl => "ctrl",
            Self::Alt => "alt",
            Self::Meta => "meta",
        }
    }
}

impl KeyCode {
    /// Map a Win32 VK_* code (plus the LL hook scancode hint for
    /// disambiguating the two halves of modifier keys) to our stable
    /// enum.
    ///
    /// Why both `vk` and `extended`? Win32 represents Left vs. Right
    /// modifier sides via the LLKHF_EXTENDED bit on the hook struct,
    /// not via separate VK codes — VK_SHIFT is shared and only the
    /// "extended" flag tells us it was the right shift. The keyboard
    /// hook callback already has both pieces of information so we
    /// might as well use them and produce a stable code.
    pub fn from_vk(vk: u32, extended: bool) -> Self {
        // Letters & digits — VK codes are ASCII for these.
        if (b'A' as u32..=b'Z' as u32).contains(&vk) {
            // SAFETY: the math below stays inside the contiguous A..=Z
            // discriminants we declared above.
            unsafe { return std::mem::transmute::<i32, Self>(10 + (vk - b'A' as u32) as i32); }
        }
        if (b'0' as u32..=b'9' as u32).contains(&vk) {
            unsafe { return std::mem::transmute::<i32, Self>(40 + (vk - b'0' as u32) as i32); }
        }

        let v = VIRTUAL_KEY(vk as u16);
        match v {
            VK_F1 => Self::F1, VK_F2 => Self::F2, VK_F3 => Self::F3, VK_F4 => Self::F4,
            VK_F5 => Self::F5, VK_F6 => Self::F6, VK_F7 => Self::F7, VK_F8 => Self::F8,
            VK_F9 => Self::F9, VK_F10 => Self::F10, VK_F11 => Self::F11, VK_F12 => Self::F12,

            // Modifier sides — both the "neutral" VK and the explicit
            // L/R variants map to the L/R discriminants. The neutral
            // VKs only appear with synthetic events; the LL hook gives
            // us VK_LSHIFT / VK_RSHIFT directly.
            VK_SHIFT | VK_LSHIFT => Self::ShiftLeft,
            VK_RSHIFT => Self::ShiftRight,
            VK_CONTROL | VK_LCONTROL => Self::ControlLeft,
            VK_RCONTROL => Self::ControlRight,
            VK_MENU | VK_LMENU => Self::AltLeft,
            VK_RMENU => Self::AltRight,
            VK_LWIN => Self::MetaLeft,
            VK_RWIN => Self::MetaRight,
            VK_CAPITAL => Self::CapsLock,

            VK_UP => Self::Up, VK_DOWN => Self::Down,
            VK_LEFT => Self::Left, VK_RIGHT => Self::Right,
            VK_HOME => Self::Home, VK_END => Self::End,
            VK_PRIOR => Self::PageUp, VK_NEXT => Self::PageDown,
            VK_INSERT => Self::Insert,
            VK_DELETE => {
                // The numpad Delete (when NumLock is off) shares VK_DELETE
                // with the dedicated Delete key. The extended flag is the
                // tie-breaker: extended = dedicated key, non-extended =
                // numpad. Same idea applies to a handful of other
                // navigation keys but Delete is the most user-visible.
                if extended { Self::Delete } else { Self::KpDelete }
            }

            VK_BACK => Self::Backspace,
            VK_TAB => Self::Tab,
            VK_RETURN => if extended { Self::KpReturn } else { Self::Return },
            VK_SPACE => Self::Space,
            VK_ESCAPE => Self::Escape,

            VK_OEM_3 => Self::BackQuote,
            VK_OEM_MINUS => Self::Minus,
            VK_OEM_PLUS => Self::Equal,
            VK_OEM_4 => Self::LeftBracket,
            VK_OEM_6 => Self::RightBracket,
            VK_OEM_5 => Self::BackSlash,
            VK_OEM_1 => Self::Semicolon,
            VK_OEM_7 => Self::Quote,
            VK_OEM_COMMA => Self::Comma,
            VK_OEM_PERIOD => Self::Dot,
            VK_OEM_2 => Self::Slash,
            VK_OEM_102 => Self::IntlBackslash,

            VK_NUMPAD0 => Self::Kp0, VK_NUMPAD1 => Self::Kp1, VK_NUMPAD2 => Self::Kp2,
            VK_NUMPAD3 => Self::Kp3, VK_NUMPAD4 => Self::Kp4, VK_NUMPAD5 => Self::Kp5,
            VK_NUMPAD6 => Self::Kp6, VK_NUMPAD7 => Self::Kp7, VK_NUMPAD8 => Self::Kp8,
            VK_NUMPAD9 => Self::Kp9,
            VK_SUBTRACT => Self::KpMinus, VK_ADD => Self::KpPlus,
            VK_MULTIPLY => Self::KpMultiply, VK_DIVIDE => Self::KpDivide,

            VK_SNAPSHOT => Self::PrintScreen,
            VK_SCROLL => Self::ScrollLock,
            VK_PAUSE => Self::Pause,
            VK_NUMLOCK => Self::NumLock,

            _ => Self::Other,
        }
    }

    pub fn modifier_group(self) -> Option<ModifierGroup> {
        match self {
            Self::ShiftLeft | Self::ShiftRight => Some(ModifierGroup::Shift),
            Self::ControlLeft | Self::ControlRight => Some(ModifierGroup::Ctrl),
            Self::AltLeft | Self::AltRight => Some(ModifierGroup::Alt),
            Self::MetaLeft | Self::MetaRight => Some(ModifierGroup::Meta),
            _ => None,
        }
    }

    #[inline]
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    pub fn from_i32(v: i32) -> Self {
        // We know every discriminant we ship; an unknown number on the
        // way back from the DB is most likely a future-version key we
        // don't yet name — surface it as Other rather than panic.
        match v {
            10..=35 | 40..=49 | 50..=61 | 80..=89 | 90..=99 | 100..=104
            | 110..=122 | 130..=145 | 150..=153 => unsafe { std::mem::transmute::<i32, Self>(v) },
            _ => Self::Other,
        }
    }

    /// Default English label for the QWERTY layout. Used by the heatmap
    /// renderer when no per-layout override applies.
    pub fn default_label(self) -> &'static str {
        use KeyCode::*;
        match self {
            A => "A", B => "B", C => "C", D => "D", E => "E",
            F => "F", G => "G", H => "H", I => "I", J => "J",
            K => "K", L => "L", M => "M", N => "N", O => "O",
            P => "P", Q => "Q", R => "R", S => "S", T => "T",
            U => "U", V => "V", W => "W", X => "X", Y => "Y", Z => "Z",
            Num0 => "0", Num1 => "1", Num2 => "2", Num3 => "3", Num4 => "4",
            Num5 => "5", Num6 => "6", Num7 => "7", Num8 => "8", Num9 => "9",
            F1 => "F1", F2 => "F2", F3 => "F3", F4 => "F4", F5 => "F5",
            F6 => "F6", F7 => "F7", F8 => "F8", F9 => "F9", F10 => "F10",
            F11 => "F11", F12 => "F12",
            ShiftLeft | ShiftRight => "Shift",
            ControlLeft | ControlRight => "Ctrl",
            AltLeft | AltRight => "Alt",
            MetaLeft | MetaRight => "Win",
            CapsLock => "Caps",
            FunctionKey => "Fn",
            Up => "↑", Down => "↓", Left => "←", Right => "→",
            Home => "Home", End => "End",
            PageUp => "PgUp", PageDown => "PgDn",
            Insert => "Ins", Delete => "Del",
            Backspace => "Backspace", Tab => "Tab", Return => "Enter",
            Space => "Space", Escape => "Esc",
            BackQuote => "`", Minus => "-", Equal => "=",
            LeftBracket => "[", RightBracket => "]", BackSlash => "\\",
            Semicolon => ";", Quote => "'", Comma => ",", Dot => ".",
            Slash => "/", IntlBackslash => "<>", IntlRo => "ろ",
            Kp0 => "0", Kp1 => "1", Kp2 => "2", Kp3 => "3", Kp4 => "4",
            Kp5 => "5", Kp6 => "6", Kp7 => "7", Kp8 => "8", Kp9 => "9",
            KpMinus => "−", KpPlus => "+", KpMultiply => "×", KpDivide => "÷",
            KpReturn => "Enter", KpDelete => ".",
            PrintScreen => "PrtSc", ScrollLock => "ScrLk", Pause => "Pause",
            NumLock => "NumLk", Other => "·",
        }
    }
}
