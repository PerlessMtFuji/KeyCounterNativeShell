// Standard touch-typing finger assignment for the heatmap "finger
// load" overlay. Indexes left-to-right:
//
//   0 = L pinky    1 = L ring    2 = L middle   3 = L index   4 = L thumb
//   5 = R thumb    6 = R index   7 = R middle   8 = R ring    9 = R pinky
//
// The mapping is QWERTY-physical: a Dvorak typist still hits the same
// physical keys, so finger load is a property of physical layout, not
// of the typed glyph. (This matches the v1 Tauri implementation and
// matches what users expect when comparing Colemak finger reach to
// QWERTY.)

use crate::core::keycode::KeyCode;

pub const FINGER_COUNT: usize = 10;

pub const FINGER_KEYS: [&str; FINGER_COUNT] = [
    "finger.lPinky", "finger.lRing", "finger.lMiddle", "finger.lIndex", "finger.lThumb",
    "finger.rThumb", "finger.rIndex", "finger.rMiddle", "finger.rRing", "finger.rPinky",
];

/// Returns the finger index (0..=9) responsible for `code` under
/// standard touch typing, or `None` if the key isn't part of the
/// alpha / number / row punctuation block (function-row keys, navigation,
/// numpad — those don't have a single canonical finger).
pub fn finger_of(code: KeyCode) -> Option<usize> {
    use KeyCode::*;
    Some(match code {
        // Left pinky: ` 1 q a z, plus the leftmost modifier column.
        BackQuote | Num1 | Q | A | Z | Tab | CapsLock | ShiftLeft | ControlLeft | Escape => 0,

        // Left ring
        Num2 | W | S | X => 1,

        // Left middle
        Num3 | E | D | C => 2,

        // Left index — covers two columns (4/5 row, r/t/f/g/v/b)
        Num4 | Num5 | R | T | F | G | V | B | AltLeft | MetaLeft => 3,

        // Thumb (space). Allocated to L thumb in v1; could be split
        // 50/50 if we ever want symmetrical bars.
        Space => 4,

        // Right index
        Num6 | Num7 | Y | U | H | J | N | M | AltRight | MetaRight => 6,

        // Right middle
        Num8 | I | K | Comma => 7,

        // Right ring
        Num9 | O | L | Dot => 8,

        // Right pinky — wide column on the right edge of the alpha
        // block plus the right-side modifier column.
        Num0 | Minus | Equal | P | LeftBracket | RightBracket | BackSlash | Semicolon | Quote
        | Slash | Return | Backspace | ShiftRight | ControlRight => 9,

        _ => return None,
    })
}

/// Sum per-key counts into a 10-element finger load vector.
pub fn finger_load(counts: &[(i32, i64)]) -> [i64; FINGER_COUNT] {
    let mut load = [0i64; FINGER_COUNT];
    for &(code, c) in counts {
        let kc = KeyCode::from_i32(code);
        if let Some(f) = finger_of(kc) {
            load[f] += c;
        }
    }
    load
}
