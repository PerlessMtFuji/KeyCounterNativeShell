// Keyboard heatmap renderer.
//
// Layout is the physical QWERTY plate (rows of keys with offsets and
// widths). The active language layout (QWERTY/QWERTZ/Dvorak/Colemak)
// only changes the printed glyphs; positions stay fixed so a Dvorak
// typist still sees their actual physical hot zone.
//
// Each key is coloured by its share of the period total — the fill
// brush is interpolated between heat_min and heat_max. Zeroes get
// heat_min flat (no special "untouched" colour, the gradient already
// resolves to a neutral surface tone there).

use super::primitives::{fill_rounded, stroke_rounded, text, HAlign, Rect, VAlign};
use super::{create_solid_brush, heat, Brush, Font, RenderContext};
use crate::core::keycode::KeyCode;
use crate::core::layouts::{label_for, LayoutId};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// One physical key on the plate.
#[derive(Copy, Clone)]
struct PlateKey {
    code: KeyCode,
    /// Position relative to the plate origin (0..n unit widths).
    col: f32,
    row: u8,
    /// Width in units. 1 = standard square key.
    width: f32,
}

/// QWERTY-physical plate. 5 rows: number row, top alpha, home, bottom
/// alpha, thumb row. We trim to a 60 % / TKL-ish shape — the numpad
/// lives off-plate and isn't drawn here (rare in the keystroke
/// distribution and would dominate the visual area).
const PLATE: &[PlateKey] = &[
    // Row 0 — number row
    k(KeyCode::BackQuote, 0.0, 0, 1.0),
    k(KeyCode::Num1, 1.0, 0, 1.0),
    k(KeyCode::Num2, 2.0, 0, 1.0),
    k(KeyCode::Num3, 3.0, 0, 1.0),
    k(KeyCode::Num4, 4.0, 0, 1.0),
    k(KeyCode::Num5, 5.0, 0, 1.0),
    k(KeyCode::Num6, 6.0, 0, 1.0),
    k(KeyCode::Num7, 7.0, 0, 1.0),
    k(KeyCode::Num8, 8.0, 0, 1.0),
    k(KeyCode::Num9, 9.0, 0, 1.0),
    k(KeyCode::Num0, 10.0, 0, 1.0),
    k(KeyCode::Minus, 11.0, 0, 1.0),
    k(KeyCode::Equal, 12.0, 0, 1.0),
    k(KeyCode::Backspace, 13.0, 0, 2.0),
    // Row 1 — top alpha row
    k(KeyCode::Tab, 0.0, 1, 1.5),
    k(KeyCode::Q, 1.5, 1, 1.0),
    k(KeyCode::W, 2.5, 1, 1.0),
    k(KeyCode::E, 3.5, 1, 1.0),
    k(KeyCode::R, 4.5, 1, 1.0),
    k(KeyCode::T, 5.5, 1, 1.0),
    k(KeyCode::Y, 6.5, 1, 1.0),
    k(KeyCode::U, 7.5, 1, 1.0),
    k(KeyCode::I, 8.5, 1, 1.0),
    k(KeyCode::O, 9.5, 1, 1.0),
    k(KeyCode::P, 10.5, 1, 1.0),
    k(KeyCode::LeftBracket, 11.5, 1, 1.0),
    k(KeyCode::RightBracket, 12.5, 1, 1.0),
    k(KeyCode::BackSlash, 13.5, 1, 1.5),
    // Row 2 — home row
    k(KeyCode::CapsLock, 0.0, 2, 1.75),
    k(KeyCode::A, 1.75, 2, 1.0),
    k(KeyCode::S, 2.75, 2, 1.0),
    k(KeyCode::D, 3.75, 2, 1.0),
    k(KeyCode::F, 4.75, 2, 1.0),
    k(KeyCode::G, 5.75, 2, 1.0),
    k(KeyCode::H, 6.75, 2, 1.0),
    k(KeyCode::J, 7.75, 2, 1.0),
    k(KeyCode::K, 8.75, 2, 1.0),
    k(KeyCode::L, 9.75, 2, 1.0),
    k(KeyCode::Semicolon, 10.75, 2, 1.0),
    k(KeyCode::Quote, 11.75, 2, 1.0),
    k(KeyCode::Return, 12.75, 2, 2.25),
    // Row 3 — bottom alpha row
    k(KeyCode::ShiftLeft, 0.0, 3, 2.25),
    k(KeyCode::Z, 2.25, 3, 1.0),
    k(KeyCode::X, 3.25, 3, 1.0),
    k(KeyCode::C, 4.25, 3, 1.0),
    k(KeyCode::V, 5.25, 3, 1.0),
    k(KeyCode::B, 6.25, 3, 1.0),
    k(KeyCode::N, 7.25, 3, 1.0),
    k(KeyCode::M, 8.25, 3, 1.0),
    k(KeyCode::Comma, 9.25, 3, 1.0),
    k(KeyCode::Dot, 10.25, 3, 1.0),
    k(KeyCode::Slash, 11.25, 3, 1.0),
    k(KeyCode::ShiftRight, 12.25, 3, 2.75),
    // Row 4 — bottom row
    k(KeyCode::ControlLeft, 0.0, 4, 1.25),
    k(KeyCode::MetaLeft, 1.25, 4, 1.25),
    k(KeyCode::AltLeft, 2.5, 4, 1.25),
    k(KeyCode::Space, 3.75, 4, 6.25),
    k(KeyCode::AltRight, 10.0, 4, 1.25),
    k(KeyCode::MetaRight, 11.25, 4, 1.25),
    k(KeyCode::ControlRight, 12.5, 4, 2.5),
];

const fn k(code: KeyCode, col: f32, row: u8, width: f32) -> PlateKey {
    PlateKey {
        code,
        col,
        row,
        width,
    }
}

const PLATE_COLS: f32 = 15.0;
const PLATE_ROWS: f32 = 5.0;
const KEY_GAP: f32 = 3.0;

/// Draw the plate inside `rect`, fitting the natural plate aspect.
/// `counts` is a closure mapping KeyCode → count for the active period
/// (caller decides what window — today, 7d, 30d, lifetime).
pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    layout: LayoutId,
    counts: impl Fn(KeyCode) -> i64,
) {
    let aspect = PLATE_COLS / PLATE_ROWS;
    let (w, h) = if rect.w / rect.h > aspect {
        (rect.h * aspect, rect.h)
    } else {
        (rect.w, rect.w / aspect)
    };
    let unit = (w / PLATE_COLS).floor();
    let plate_w = unit * PLATE_COLS;
    let plate_h = unit * PLATE_ROWS;
    let x0 = rect.x + (rect.w - plate_w) * 0.5;
    let y0 = rect.y + (rect.h - plate_h) * 0.5;

    // Pre-compute max for normalisation. We pre-walk PLATE so that
    // off-plate keys (numpad, function row) don't skew the gradient.
    let max = PLATE
        .iter()
        .map(|kp| counts(kp.code))
        .max()
        .unwrap_or(0)
        .max(1);

    let palette = ctx.palette();
    for kp in PLATE {
        let kx = x0 + kp.col * unit + KEY_GAP * 0.5;
        let ky = y0 + kp.row as f32 * unit + KEY_GAP * 0.5;
        let kw = kp.width * unit - KEY_GAP;
        let kh = unit - KEY_GAP;
        let value = counts(kp.code);
        let t = if max > 0 {
            (value as f32 / max as f32).powf(0.55) // gamma boost low values
        } else {
            0.0
        };
        let rgba = heat(palette, t);
        // Manual brush — allocate one-shot. We could pool but the key
        // count is 60-ish per paint and brushes are cheap.
        let key_rect = Rect::new(kx, ky, kw, kh);
        let color = D2D1_COLOR_F {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        };
        if let Ok(brush) = create_solid_brush(&ctx.target, color) {
            let rr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
                rect: key_rect.to_d2d(),
                radiusX: 4.0,
                radiusY: 4.0,
            };
            unsafe {
                ctx.target.FillRoundedRectangle(&rr, &brush);
            }
        }
        stroke_rounded(ctx, key_rect, 4.0, Brush::Border, 1.0);

        // Glyph — only on standard-width keys. Wide ones (Backspace,
        // Enter, Shift, Space) get a label from the keycode default.
        let lbl = label_for(layout, kp.code);
        let lbl_brush = if value == 0 {
            Brush::TextDim
        } else if t > 0.55 {
            Brush::Surface
        } else {
            Brush::Text
        };
        text(
            ctx,
            key_rect.shrink(2.0, 2.0),
            lbl,
            if kp.width >= 1.5 { Font::Caption } else { Font::Glyph },
            lbl_brush,
            HAlign::Centre,
            VAlign::Centre,
        );
    }
}

/// Small horizontal legend strip — gradient swatch + min / max
/// labels. Caller decides where to place it (typically below the
/// plate).
pub fn legend(ctx: &RenderContext, rect: Rect, max: i64) {
    let palette = ctx.palette();
    let steps = 24;
    let step_w = rect.w / steps as f32;
    for i in 0..steps {
        let t = i as f32 / (steps - 1) as f32;
        let rgba = heat(palette, t);
        let color = D2D1_COLOR_F {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        };
        if let Ok(brush) = create_solid_brush(&ctx.target, color) {
            let cell = Rect::new(rect.x + i as f32 * step_w, rect.y, step_w + 0.5, rect.h - 14.0);
            unsafe {
                ctx.target.FillRectangle(&cell.to_d2d(), &brush);
            }
        }
    }
    text(
        ctx,
        Rect::new(rect.x, rect.bottom() - 12.0, rect.w * 0.5, 12.0),
        "0",
        Font::Caption,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );
    text(
        ctx,
        Rect::new(
            rect.x + rect.w * 0.5,
            rect.bottom() - 12.0,
            rect.w * 0.5,
            12.0,
        ),
        &super::primitives::fmt_int(max),
        Font::Caption,
        Brush::TextDim,
        HAlign::Trailing,
        VAlign::Top,
    );
    fill_rounded(ctx, Rect::new(rect.x, rect.y, rect.w, 0.0), 0.0, Brush::Border);
}
