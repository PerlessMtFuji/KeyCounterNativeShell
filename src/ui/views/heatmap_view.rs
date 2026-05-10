// Heatmap view — physical keyboard plate + finger load.
//
// The plate renderer in `render::heatmap` already does the heavy lifting:
// it lays out the 60-key TKL shape, gradients every cell, and draws the
// layout-aware label. This view stitches the plate together with the
// legend strip and a ten-bar finger-load chart so the eye can land on
// either the spatial picture or the per-finger numbers without a click.
//
// Counts are sourced from `snap.top_30d`: that's the 30-day key tally
// the storage layer keeps warm; we project it into a `KeyCode → i64`
// closure for the plate and into a `[i64; 10]` for the finger bars.

use crate::app::AppState;
use crate::core::finger_map::{finger_load, FINGER_KEYS};
use crate::core::i18n;
use crate::core::keycode::KeyCode;
use crate::ui::controls::InputState;
use crate::ui::render::heatmap;
use crate::ui::render::primitives::{card, fill_rounded, fmt_int, text, HAlign, Rect, VAlign};
use crate::ui::render::{Brush, Font, RenderContext};

pub fn draw(ctx: &RenderContext, rect: Rect, state: &AppState, _input: &mut InputState) {
    let snap = state.snapshot.read();
    let pad = 24.0;
    let body = rect.shrink(pad, pad);

    // Title + subtitle
    let (title_row, body) = body.split_top(34.0);
    text(
        ctx,
        title_row,
        i18n::t("heatmap.title"),
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let (sub_row, body) = body.split_top(24.0);
    text(
        ctx,
        sub_row,
        i18n::t("heatmap.subtitle"),
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // Build a count lookup. The 30-day window covers the typical
    // "what's hot lately" question better than lifetime (a years-old
    // habit shouldn't outshine the last month) and better than today
    // (too noisy when the user hasn't typed yet).
    let counts_vec: Vec<(i32, i64)> = snap.top_30d.iter().map(|kc| (kc.code, kc.count)).collect();
    let count_for = |code: KeyCode| -> i64 {
        let c = code.as_i32();
        counts_vec
            .iter()
            .find_map(|(k, v)| if *k == c { Some(*v) } else { None })
            .unwrap_or(0)
    };

    // Split: top 60% for the plate card, bottom 40% for finger load.
    let plate_h = (body.h * 0.60).max(260.0);
    let (plate_row, fingers_row) = body.split_top(plate_h);
    let plate_row = plate_row.inset(0.0, 0.0, 16.0, 0.0);

    // Plate card
    card(ctx, plate_row, 14.0);
    let inner = plate_row.shrink(16.0, 14.0);
    let (legend_band, plate_area) = inner.split_top(0.0); // placeholder
    let _ = legend_band;
    let plate_area = plate_area.inset(0.0, 0.0, 28.0, 0.0);
    let layout = i18n::SETTINGS.read().layout;
    heatmap::draw(ctx, plate_area, layout, count_for);
    // Legend strip across the bottom of the plate card.
    let legend_rect = Rect::new(
        plate_area.x + plate_area.w * 0.25,
        plate_area.bottom() + 6.0,
        plate_area.w * 0.5,
        20.0,
    );
    let plate_max = counts_vec.iter().map(|(_, v)| *v).max().unwrap_or(0);
    heatmap::legend(ctx, legend_rect, plate_max);

    // Finger load card
    card(ctx, fingers_row, 14.0);
    let inner = fingers_row.shrink(20.0, 16.0);
    let title_text = i18n::t_var("heatmap.fingerLoad", &[("layout", layout.name())]);
    let (title_row, inner) = inner.split_top(22.0);
    text(
        ctx,
        title_row,
        &title_text,
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let (hint_row, bars_row) = inner.split_top(18.0);
    text(
        ctx,
        hint_row,
        i18n::t("heatmap.fingerLoadHint"),
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Top,
    );

    let loads = finger_load(&counts_vec);
    draw_finger_bars(ctx, bars_row, &loads);
}

fn draw_finger_bars(ctx: &RenderContext, rect: Rect, loads: &[i64; 10]) {
    let max = (*loads.iter().max().unwrap_or(&0)).max(1);
    let n = loads.len();
    let label_w = 86.0;
    let value_w = 70.0;
    let bar_x = rect.x + label_w + 6.0;
    let bar_max = (rect.w - label_w - value_w - 12.0).max(20.0);
    let row_h = (rect.h / n as f32).clamp(18.0, 26.0);

    for (i, value) in loads.iter().enumerate() {
        let y = rect.y + i as f32 * row_h;
        let row = Rect::new(rect.x, y, rect.w, row_h - 4.0);
        text(
            ctx,
            Rect::new(rect.x, y, label_w, row.h),
            i18n::t(FINGER_KEYS[i]),
            Font::Body,
            Brush::TextDim,
            HAlign::Leading,
            VAlign::Centre,
        );
        let bar_h = (row.h - 8.0).max(4.0);
        let bar_y = y + (row.h - bar_h) * 0.5;
        fill_rounded(
            ctx,
            Rect::new(bar_x, bar_y, bar_max, bar_h),
            bar_h * 0.5,
            Brush::SurfaceAlt,
        );
        let frac = (*value as f32 / max as f32).clamp(0.0, 1.0);
        if frac > 0.0 {
            fill_rounded(
                ctx,
                Rect::new(bar_x, bar_y, bar_max * frac, bar_h),
                bar_h * 0.5,
                // Thumbs get the accent_strong; the rest the accent
                // gradient endpoint so the eye can immediately see which
                // bar is a thumb vs a finger. Indices 4, 5 = thumbs.
                if i == 4 || i == 5 {
                    Brush::Modifier
                } else {
                    Brush::Accent
                },
            );
        }
        text(
            ctx,
            Rect::new(bar_x + bar_max + 6.0, y, value_w, row.h),
            &fmt_int(*value),
            Font::Body,
            Brush::TextDim,
            HAlign::Trailing,
            VAlign::Centre,
        );
    }
}
