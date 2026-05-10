// Dashboard view.
//
// Layout from top to bottom (matching the React version's grid):
//
//   subtitle
//   ┌──────────┬──────────┬──────────┬──────────┐
//   │ Today    │ KPM live │ Streak   │ Lifetime │
//   └──────────┴──────────┴──────────┴──────────┘
//   ┌────────────────────────┬───────────────────┐
//   │ Today by hour          │ Top 5 keys · 30d  │
//   └────────────────────────┴───────────────────┘
//   ┌────────────────────────────────────────────┐
//   │ 7-day trend                                │
//   └────────────────────────────────────────────┘
//
// We render in column-major order: clip the available rect to a
// padded inset, then chain split_top to peel off rows.

use crate::app::AppState;
use crate::core::i18n;
use crate::core::keycode::KeyCode;
use crate::ui::controls::InputState;
use crate::ui::render::charts::{hourly_bars, stat_block, top_keys};
use crate::ui::render::primitives::{card, fill_rounded, fmt_int, text, HAlign, Rect, VAlign};
use crate::ui::render::{Brush, Font, RenderContext};

pub fn draw(ctx: &RenderContext, rect: Rect, state: &AppState, _input: &mut InputState) {
    let snap = state.snapshot.read();
    let pad = 24.0;
    let body = rect.shrink(pad, pad);
    let kpm = state.pulse.read().kpm();

    // Title + subtitle
    let (title, body) = body.split_top(34.0);
    text(
        ctx,
        title,
        i18n::t("dashboard.title"),
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let subtitle_key = if snap.lifetime > 0 {
        "dashboard.subtitleNormal"
    } else {
        "dashboard.subtitleFresh"
    };
    let (subtitle, body) = body.split_top(28.0);
    text(
        ctx,
        subtitle,
        i18n::t(subtitle_key),
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // Top stat cards row — Today / KPM / Streak / Lifetime.
    let (cards_row, body) = body.split_top(120.0);
    let card_gap = 16.0;
    let card_w = (cards_row.w - card_gap * 3.0) * 0.25;
    let today_total = snap.today.as_ref().map(|t| t.total).unwrap_or(0);
    let streak_label = format!("{} {}", snap.streak, i18n::t("common.days"));
    let cards: [(String, String, bool); 4] = [
        ("Today".to_string(), fmt_int(today_total), true),
        (i18n::t("dashboard.kpmHint").to_string(), fmt_int(kpm), true),
        (i18n::t("dashboard.streak").to_string(), streak_label, false),
        (i18n::t("dashboard.lifetime").to_string(), fmt_int(snap.lifetime), false),
    ];
    let mut x = cards_row.x;
    for (label, value, accent) in &cards {
        let card_rect = Rect::new(x, cards_row.y, card_w, cards_row.h);
        card(ctx, card_rect, 14.0);
        stat_block(ctx, card_rect, label, value, *accent);
        x += card_w + card_gap;
    }

    // Lower row: hourly bars (2/3) + top keys (1/3)
    let (lower, body) = body.split_top(220.0);
    let lower = lower.inset(card_gap, 0.0, 0.0, 0.0);
    let hourly_w = (lower.w - card_gap) * (2.0 / 3.0);
    let hourly_rect = Rect::new(lower.x, lower.y, hourly_w, lower.h);
    let top_rect = Rect::new(
        hourly_rect.right() + card_gap,
        lower.y,
        lower.w - hourly_w - card_gap,
        lower.h,
    );

    // Hourly card
    card(ctx, hourly_rect, 14.0);
    let inner = hourly_rect.shrink(20.0, 18.0);
    let (h_title, h_body) = inner.split_top(22.0);
    text(
        ctx,
        h_title,
        i18n::t("dashboard.byHour"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    hourly_bars(ctx, h_body, &snap.hourly);

    // Top keys card
    card(ctx, top_rect, 14.0);
    let inner = top_rect.shrink(20.0, 18.0);
    let (t_title, t_body) = inner.split_top(22.0);
    text(
        ctx,
        t_title,
        i18n::t("dashboard.top5"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let entries: Vec<(String, i64)> = snap
        .top_30d
        .iter()
        .take(5)
        .map(|kc| {
            (
                KeyCode::from_i32(kc.code).default_label().to_string(),
                kc.count,
            )
        })
        .collect();
    top_keys(ctx, t_body, &entries, 5, 70.0, 60.0);

    // 7-day trend card
    let (trend_row, _rest) = body.split_top(140.0);
    let trend_row = trend_row.inset(card_gap, 0.0, 0.0, 0.0);
    card(ctx, trend_row, 14.0);
    let inner = trend_row.shrink(20.0, 18.0);
    let (tr_title, tr_body) = inner.split_top(22.0);
    text(
        ctx,
        tr_title,
        i18n::t("dashboard.trend7"),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if let Some(r7) = snap.range7.as_ref() {
        draw_trend(ctx, tr_body, r7);
    } else {
        text(
            ctx,
            tr_body,
            i18n::t("common.notEnough"),
            Font::Body,
            Brush::TextDim,
            HAlign::Centre,
            VAlign::Centre,
        );
    }
}

fn draw_trend(ctx: &RenderContext, rect: Rect, r7: &crate::core::store::RangeStats) {
    if r7.by_day.is_empty() {
        return;
    }
    let max = r7.by_day.iter().map(|d| d.total).max().unwrap_or(0).max(1);
    let n = r7.by_day.len();
    let bar_w = rect.w / n as f32;
    let pad_w = (bar_w * 0.22).max(2.0);
    for (i, day) in r7.by_day.iter().enumerate() {
        let frac = day.total as f32 / max as f32;
        let x = rect.x + i as f32 * bar_w + pad_w;
        let w = bar_w - 2.0 * pad_w;
        let avail = rect.h - 22.0;
        let h = avail * frac;
        let y = rect.bottom() - 22.0 - h;
        fill_rounded(ctx, Rect::new(x, y, w, h), 3.0, Brush::Accent);
        // Day label — last 3 chars of YYYY-MM-DD = "-DD"; show DD.
        let dd = day.day.split('-').last().unwrap_or("");
        text(
            ctx,
            Rect::new(rect.x + i as f32 * bar_w, rect.bottom() - 18.0, bar_w, 16.0),
            dd,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Centre,
            VAlign::Top,
        );
    }
}
