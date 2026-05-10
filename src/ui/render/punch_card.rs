// 7×24 punch card — day-of-week × hour-of-day activity grid.
//
// Cell size is whatever fits in the rect; cells are square if there's
// room, otherwise the wider axis is squashed to keep both visible.
// Colour is the heat ramp against the per-grid maximum.

use super::primitives::{text, HAlign, Rect, VAlign};
use super::{heat, Brush, Font, RenderContext};
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

const DAY_NAMES: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];

pub fn draw(ctx: &RenderContext, rect: Rect, grid: &[[i64; 24]; 7]) {
    let label_w = 28.0;
    let label_h = 14.0;
    let inner = Rect::new(
        rect.x + label_w,
        rect.y + label_h,
        rect.w - label_w,
        rect.h - label_h,
    );

    let cell_w = (inner.w / 24.0).floor().max(8.0);
    let cell_h = (inner.h / 7.0).floor().max(8.0);
    let cell_size = cell_w.min(cell_h);
    let gap = (cell_size * 0.18).max(1.0);

    let max = grid
        .iter()
        .flatten()
        .copied()
        .max()
        .unwrap_or(0)
        .max(1);
    let palette = ctx.palette();

    // Hour labels every 4 hours
    for h in (0..24).step_by(4) {
        let x = inner.x + h as f32 * cell_size;
        let lbl = format!("{:02}", h);
        text(
            ctx,
            Rect::new(x, rect.y, cell_size * 2.0, label_h),
            &lbl,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    // Day labels
    for (row, name) in DAY_NAMES.iter().enumerate() {
        let y = inner.y + row as f32 * cell_size;
        text(
            ctx,
            Rect::new(rect.x, y, label_w - 4.0, cell_size),
            name,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Trailing,
            VAlign::Centre,
        );
    }

    for row in 0..7 {
        for col in 0..24 {
            let v = grid[row][col];
            let t = if v == 0 {
                0.0
            } else {
                ((v as f32 / max as f32).powf(0.5)).clamp(0.08, 1.0)
            };
            let rgba = if v == 0 { palette.surface_alt } else { heat(palette, t) };
            let cell = Rect::new(
                inner.x + col as f32 * cell_size + gap * 0.5,
                inner.y + row as f32 * cell_size + gap * 0.5,
                cell_size - gap,
                cell_size - gap,
            );
            draw_cell(ctx, cell, rgba);
        }
    }
}

fn draw_cell(ctx: &RenderContext, rect: Rect, rgba: [f32; 4]) {
    let color = D2D1_COLOR_F {
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
        a: rgba[3],
    };
    if let Ok(brush) = unsafe { ctx.target.CreateSolidColorBrush(&color, None) } {
        let rr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: rect.to_d2d(),
            radiusX: 2.0,
            radiusY: 2.0,
        };
        unsafe {
            ctx.target.FillRoundedRectangle(&rr, &brush);
        }
    }
}
