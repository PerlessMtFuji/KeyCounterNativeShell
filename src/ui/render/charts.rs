// Chart primitives.
//
// All charts here are flat data → Direct2D drawing. No intermediate
// state, no animation framework, no axis machinery — the data ranges
// in this app are bounded enough that the drawing helpers can pick
// reasonable defaults (max → top of chart, integer floor at 0).
//
// Animations that we *do* want (the live pulse, count-up on the big
// numbers) live in the per-element modules so they can keep their own
// timing state.

use super::primitives::{fill_rect, fill_rounded, line, text, HAlign, Rect, VAlign};
use super::{Brush, Font, RenderContext};

/// Horizontal bar chart for "top keys": a stacked column of [label,
/// bar, count] rows. `entries` are (label, value) and the longest
/// value defines the full bar width.
///
/// The chart owns its own row height and inter-row spacing; callers
/// just give it a containing rect and how many rows to draw.
pub fn top_keys(
    ctx: &RenderContext,
    rect: Rect,
    entries: &[(String, i64)],
    max_rows: usize,
    label_width: f32,
    value_width: f32,
) {
    let want = entries.len().min(max_rows);
    if want == 0 {
        text(
            ctx,
            rect,
            "—",
            Font::Body,
            Brush::TextDim,
            HAlign::Centre,
            VAlign::Centre,
        );
        return;
    }
    // Lay out rows as tall as the available height allows, with a
    // minimum readable height (≥ body font cap) and a comfortable max.
    // If even at the floor we'd overflow the card, render only as many
    // rows as fit — clipping mid-bar looks worse than truncating.
    // 22 DIPs minimum so the 14 px Body face (≈ 19 DIPs line height)
    // clears the row below; the old 18 DIPs let descenders kiss the
    // label of the next row in the top-20 list.
    let row_h = (rect.h / want as f32).clamp(22.0, 30.0);
    let n = ((rect.h / row_h).floor() as usize).clamp(1, want);
    let max_val = entries.iter().take(n).map(|(_, v)| *v).max().unwrap_or(1).max(1);
    let gap = 6.0;
    let bar_x = rect.x + label_width;
    let bar_max = (rect.w - label_width - value_width).max(20.0);

    for (i, (label, value)) in entries.iter().take(n).enumerate() {
        let y = rect.y + i as f32 * row_h;
        let row = Rect::new(rect.x, y, rect.w, row_h - gap);
        // label
        text(
            ctx,
            Rect::new(rect.x, y, label_width - 6.0, row.h),
            label,
            Font::Body,
            Brush::Text,
            HAlign::Leading,
            VAlign::Centre,
        );
        // bar
        let frac = (*value as f32 / max_val as f32).clamp(0.02, 1.0);
        let bar_h = (row.h - 6.0).max(4.0);
        let bar_y = y + (row.h - bar_h) * 0.5;
        // background track
        fill_rounded(
            ctx,
            Rect::new(bar_x, bar_y, bar_max, bar_h),
            bar_h * 0.5,
            Brush::SurfaceAlt,
        );
        // foreground
        fill_rounded(
            ctx,
            Rect::new(bar_x, bar_y, bar_max * frac, bar_h),
            bar_h * 0.5,
            Brush::Accent,
        );
        // value
        text(
            ctx,
            Rect::new(bar_x + bar_max + 6.0, y, value_width, row.h),
            &super::primitives::fmt_int(*value),
            Font::Body,
            Brush::TextDim,
            HAlign::Trailing,
            VAlign::Centre,
        );
    }
}

/// 24-hour vertical bar chart. `values[h]` is the count for hour h.
/// The y-axis auto-scales to the max non-zero value; if everything is
/// zero we still draw the empty grid so the layout doesn't pop.
pub fn hourly_bars(ctx: &RenderContext, rect: Rect, values: &[i64; 24]) {
    let max = (*values.iter().max().unwrap_or(&0)).max(1);
    let bar_w = rect.w / 24.0;
    let inner_pad = (bar_w * 0.18).max(1.0);

    // baseline
    line(
        ctx,
        rect.x,
        rect.bottom() - 14.0,
        rect.right(),
        rect.bottom() - 14.0,
        Brush::Border,
        1.0,
    );

    for (h, v) in values.iter().enumerate() {
        let x = rect.x + h as f32 * bar_w + inner_pad;
        let w = bar_w - 2.0 * inner_pad;
        // Reserve 14 px at the bottom for the hour label.
        let avail_h = rect.h - 18.0;
        let bar_h = (avail_h * (*v as f32 / max as f32)).max(0.0);
        let y = rect.bottom() - 14.0 - bar_h;
        let r = Rect::new(x, y, w, bar_h);
        let brush = if *v == 0 { Brush::SurfaceAlt } else { Brush::Accent };
        fill_rounded(ctx, r, 2.0, brush);
        // hour label every 4 hours (00, 04, 08, ...)
        if h % 4 == 0 {
            let lbl = format!("{:02}", h);
            text(
                ctx,
                Rect::new(x - inner_pad, rect.bottom() - 12.0, bar_w, 12.0),
                &lbl,
                Font::Caption,
                Brush::TextMuted,
                HAlign::Centre,
                VAlign::Top,
            );
        }
    }
}

/// Two-segment ratio bar — "X of Y" visualisation used for the
/// backspace-vs-typed ratio and the modifier mix.
pub fn ratio_bar(ctx: &RenderContext, rect: Rect, fraction: f32, accent: Brush) {
    let f = fraction.clamp(0.0, 1.0);
    fill_rounded(ctx, rect, rect.h * 0.5, Brush::SurfaceAlt);
    if f > 0.0 {
        let lit = Rect::new(rect.x, rect.y, rect.w * f, rect.h);
        fill_rounded(ctx, lit, rect.h * 0.5, accent);
    }
}

/// Stacked horizontal bar with N segments. Used for the modifier mix
/// (shift/ctrl/alt/meta share of total presses).
pub fn stacked_bar(ctx: &RenderContext, rect: Rect, segments: &[(f32, Brush)]) {
    let total: f32 = segments.iter().map(|(v, _)| *v).sum();
    if total <= 0.0 {
        fill_rounded(ctx, rect, rect.h * 0.5, Brush::SurfaceAlt);
        return;
    }
    let mut x = rect.x;
    let r = rect.h * 0.5;
    let n = segments.len();
    for (i, (value, brush)) in segments.iter().enumerate() {
        let w = rect.w * (value / total);
        // Outer corners are rounded; inner segments are plain rects so
        // they butt up cleanly. D2D doesn't have per-corner radii on
        // the rounded-rect primitive so we approximate with a plain
        // fill_rect for middle segments.
        let seg = Rect::new(x, rect.y, w, rect.h);
        if i == 0 || i == n - 1 {
            fill_rounded(ctx, seg, r, *brush);
        } else {
            fill_rect(ctx, seg, *brush);
        }
        x += w;
    }
}

/// Single big number with a small caption underneath. The "card" type
/// the React UI calls a Stat. Caller has already drawn the card frame.
pub fn stat_block(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    value: &str,
    accent: bool,
) {
    let pad = 18.0;
    let inner = rect.shrink(pad, pad);
    let (label_row, rest) = inner.split_top(16.0);
    let (value_row, _) = rest.split_top(rest.h);

    text(
        ctx,
        label_row,
        label,
        Font::CardLabel,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );
    text(
        ctx,
        value_row,
        value,
        Font::Display,
        if accent { Brush::AccentStrong } else { Brush::Text },
        HAlign::Leading,
        VAlign::Centre,
    );
}
