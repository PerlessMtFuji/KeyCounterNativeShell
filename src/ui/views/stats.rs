// Stats view — denser breakdown of what's in the database.
//
// Three columns of cards, give or take:
//   * Top-left: top-20 keys (extends the dashboard's top-5).
//   * Top-right: modifier mix today (stacked bar) + backspace ratio.
//   * Bottom: 7×24 punch card, full-width.
//   * Below the punch card: 365-day calendar.
//
// Most rendering here is just wiring existing chart primitives — the
// view's job is layout and i18n.

use crate::app::AppState;
use crate::core::i18n;
use crate::core::keycode::KeyCode;
use crate::ui::controls::InputState;
use crate::ui::render::calendar;
use crate::ui::render::charts::{ratio_bar, stacked_bar, top_keys};
use crate::ui::render::primitives::{
    card, fmt_int, text, HAlign, Rect, VAlign,
};
use crate::ui::render::punch_card;
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
        i18n::t("stats.title"),
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let (sub_row, body) = body.split_top(24.0);
    text(
        ctx,
        sub_row,
        i18n::t("stats.subtitle"),
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // Top row: top-20 keys (left, 2/3) + modifier mix & backspace (right, 1/3)
    let gap = 16.0;
    let top_h = 280.0;
    let (top_row, body) = body.split_top(top_h);
    let top_row = top_row.inset(0.0, 0.0, gap, 0.0);
    let left_w = (top_row.w - gap) * (2.0 / 3.0);
    let left_rect = Rect::new(top_row.x, top_row.y, left_w, top_row.h);
    let right_rect = Rect::new(
        left_rect.right() + gap,
        top_row.y,
        top_row.w - left_w - gap,
        top_row.h,
    );

    // Top-20 card
    card(ctx, left_rect, 14.0);
    let inner = left_rect.shrink(20.0, 16.0);
    let (tr_title, tr_body) = inner.split_top(22.0);
    text(
        ctx,
        tr_title,
        i18n::t("stats.top20"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let entries: Vec<(String, i64)> = snap
        .top_30d
        .iter()
        .take(20)
        .map(|kc| {
            (
                KeyCode::from_i32(kc.code).default_label().to_string(),
                kc.count,
            )
        })
        .collect();
    top_keys(ctx, tr_body, &entries, 20, 64.0, 72.0);

    // Modifier mix + backspace ratio card
    card(ctx, right_rect, 14.0);
    let inner = right_rect.shrink(20.0, 16.0);
    let (mod_title, inner) = inner.split_top(22.0);
    text(
        ctx,
        mod_title,
        i18n::t("stats.modifierMix"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );

    let mods = snap
        .today
        .as_ref()
        .map(|t| t.modifiers.clone())
        .unwrap_or_default();
    let total_today = snap.today.as_ref().map(|t| t.total).unwrap_or(0);
    let segments = [
        (mods.shift as f32, Brush::Accent),
        (mods.ctrl as f32, Brush::AccentStrong),
        (mods.alt as f32, Brush::Modifier),
        (mods.meta as f32, Brush::Pulse),
    ];
    let (bar_row, inner) = inner.split_top(28.0);
    let bar_rect = Rect::new(bar_row.x, bar_row.y + 6.0, bar_row.w, 16.0);
    stacked_bar(ctx, bar_rect, &segments);

    // Legend rows
    let mods_total: i64 = mods.shift + mods.ctrl + mods.alt + mods.meta;
    let pct = |n: i64| -> String {
        if mods_total <= 0 {
            "—".to_string()
        } else {
            format!("{:.1}%", n as f32 * 100.0 / mods_total as f32)
        }
    };
    let legend = [
        ("Shift", mods.shift, Brush::Accent),
        ("Ctrl", mods.ctrl, Brush::AccentStrong),
        ("Alt", mods.alt, Brush::Modifier),
        ("Meta", mods.meta, Brush::Pulse),
    ];
    let (legend_row, inner) = inner.split_top(80.0);
    let row_h = legend_row.h / legend.len() as f32;
    for (i, (name, n, brush)) in legend.iter().enumerate() {
        let y = legend_row.y + i as f32 * row_h;
        // colour dot
        let dot_r = 5.0;
        crate::ui::render::primitives::fill_circle(
            ctx,
            legend_row.x + 6.0,
            y + row_h * 0.5,
            dot_r,
            *brush,
        );
        text(
            ctx,
            Rect::new(legend_row.x + 18.0, y, 80.0, row_h),
            name,
            Font::Body,
            Brush::Text,
            HAlign::Leading,
            VAlign::Centre,
        );
        text(
            ctx,
            Rect::new(legend_row.x + 80.0, y, 60.0, row_h),
            &fmt_int(*n),
            Font::Body,
            Brush::TextDim,
            HAlign::Leading,
            VAlign::Centre,
        );
        text(
            ctx,
            Rect::new(legend_row.right() - 60.0, y, 60.0, row_h),
            &pct(*n),
            Font::Body,
            Brush::TextDim,
            HAlign::Trailing,
            VAlign::Centre,
        );
    }

    // Backspace ratio block beneath the legend.
    let (bs_title, inner) = inner.split_top(20.0);
    text(
        ctx,
        bs_title,
        i18n::t("stats.backspaceRatio"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let backspaces = snap
        .today
        .as_ref()
        .map(|t| {
            t.by_code
                .iter()
                .find(|kc| kc.code == KeyCode::Backspace.as_i32())
                .map(|kc| kc.count)
                .unwrap_or(0)
        })
        .unwrap_or(0);
    let ratio = if total_today > 0 {
        backspaces as f32 / total_today as f32
    } else {
        0.0
    };
    let (bs_bar_row, bs_hint_row) = inner.split_top(20.0);
    let bs_bar_rect = Rect::new(bs_bar_row.x, bs_bar_row.y + 4.0, bs_bar_row.w, 12.0);
    ratio_bar(ctx, bs_bar_rect, ratio, Brush::Warn);

    let hint_key = if total_today < 200 {
        "stats.backspaceWarmup"
    } else if ratio < 0.05 {
        "stats.backspaceLow"
    } else if ratio < 0.15 {
        "stats.backspaceMid"
    } else {
        "stats.backspaceHigh"
    };
    text(
        ctx,
        bs_hint_row,
        i18n::t(hint_key),
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Top,
    );

    // Punch card (full width)
    let punch_h = 220.0;
    let (punch_row, body) = body.split_top(punch_h);
    let punch_row = punch_row.inset(gap, 0.0, gap, 0.0);
    card(ctx, punch_row, 14.0);
    let inner = punch_row.shrink(20.0, 16.0);
    let (pc_title, pc_body) = inner.split_top(22.0);
    text(
        ctx,
        pc_title,
        i18n::t("heatmap.punchCard"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    punch_card::draw(ctx, pc_body, &snap.punch_card);

    // 365-day calendar (full width)
    let cal_h = body.h.max(120.0);
    let cal_row = Rect::new(body.x, body.y, body.w, cal_h);
    let cal_row = cal_row.inset(gap, 0.0, 0.0, 0.0);
    card(ctx, cal_row, 14.0);
    let inner = cal_row.shrink(20.0, 16.0);
    let (cal_title, cal_body) = inner.split_top(22.0);
    text(
        ctx,
        cal_title,
        i18n::t("heatmap.calendar"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let _ = calendar::draw(ctx, cal_body, &snap.calendar_365);
}
