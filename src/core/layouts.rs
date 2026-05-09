// Physical-remap keyboard layouts.
//
// Keys stay in the same physical position on the heatmap; only the
// printed glyph changes. This matches what users actually see when
// they switch their OS layout — a Dvorak user's `D` key is still in
// the QWERTY-`E` slot, the label just reads "E" with the Dvorak
// override applied.
//
// The override map is sparse: anything not listed falls through to
// `KeyCode::default_label()`.

use crate::core::keycode::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutId {
    Qwerty,
    Qwertz,
    Dvorak,
    Colemak,
}

impl LayoutId {
    pub fn name(self) -> &'static str {
        match self {
            Self::Qwerty => "QWERTY",
            Self::Qwertz => "QWERTZ (DE)",
            Self::Dvorak => "Dvorak",
            Self::Colemak => "Colemak",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::Qwerty => "qwerty",
            Self::Qwertz => "qwertz",
            Self::Dvorak => "dvorak",
            Self::Colemak => "colemak",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "qwertz" => Self::Qwertz,
            "dvorak" => Self::Dvorak,
            "colemak" => Self::Colemak,
            _ => Self::Qwerty,
        }
    }

    pub const ALL: [LayoutId; 4] = [Self::Qwerty, Self::Qwertz, Self::Dvorak, Self::Colemak];
}

/// Produces the printed label for a key under the given layout, taking
/// into account any per-layout overrides.
pub fn label_for(layout: LayoutId, code: KeyCode) -> &'static str {
    if let Some(over) = override_for(layout, code) {
        return over;
    }
    code.default_label()
}

fn override_for(layout: LayoutId, code: KeyCode) -> Option<&'static str> {
    use KeyCode::*;
    match layout {
        LayoutId::Qwerty => None,
        LayoutId::Qwertz => Some(match code {
            // German QWERTZ swaps Y and Z and rearranges punctuation;
            // we cover the headline difference here. The full mapping
            // (umlauts on `[`, `;`, `'`) we deliberately skip — those
            // glyphs need wider key cells than our heatmap allocates
            // and end up clipping unattractively.
            Y => "Z",
            Z => "Y",
            _ => return None,
        }),
        LayoutId::Dvorak => Some(match code {
            Q => "'", W => ",", E => ".", R => "P", T => "Y",
            Y => "F", U => "G", I => "C", O => "R", P => "L",
            LeftBracket => "/", RightBracket => "=",
            S => "O", D => "E", FKey => "U", G => "I",
            H => "D", J => "H", K => "T", L => "N",
            Semicolon => "S", Quote => "-",
            Z => ";", X => "Q", C => "J", V => "K", B => "X",
            N => "B",
            Comma => "W", Dot => "V", Slash => "Z",
            _ => return None,
        }),
        LayoutId::Colemak => Some(match code {
            E => "F", R => "P", T => "G",
            Y => "J", U => "L", I => "U", O => "Y", P => ";",
            S => "R", D => "S", FKey => "T", G => "D",
            J => "N", K => "E", L => "I",
            Semicolon => "O",
            _ => return None,
        }),
    }
}

// Local alias — `F` as a KeyCode discriminant is awkward to write inside
// a match arm because it shadows nothing but reads ambiguously next to
// the function-row F1..F12. Aliasing to `FKey` makes the override
// tables readable without renaming the whole enum.
#[allow(non_upper_case_globals)]
const FKey: KeyCode = KeyCode::F;
