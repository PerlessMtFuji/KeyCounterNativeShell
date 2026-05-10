// Top-bar live status: KPM, last-minute, last-hour, and a thin pulse
// indicator that flashes when a keystroke arrives.
//
// The "pulse" is a per-frame interpolation against `last_event_ms`.
// We don't drive it on a tight rAF — the main window only paints on
// the 500 ms tick (or sooner when the snapshot updates), which is
// plenty for the pulse glow to feel responsive without spending CPU
// on idle redraws.

use crate::core::stats::PulseHistory;
use crate::ui::render::primitives::{
    fill_circle, fill_rect, fill_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};
use crate::core::i18n;
use crate::core::store::LiveSnapshot;

pub const HEIGHT: f32 = 56.0;

pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    pulse: &PulseHistory,
    live: &LiveSnapshot,
    lifetime: i64,
    last_event_ms: i64,
    now_ms: i64,
) {
    fill_rect(ctx, rect, Brush::Bg);
    crate::ui::render::primitives::line(
        ctx,
        rect.x,
        rect.bottom(),
        rect.right(),
        rect.bottom(),
        Brush::Border,
        1.0,
    );

    // Pulse dot — lit for ~700 ms after the last event, fading back to
    // text_dim. Distance keeps the dot visible without being noisy.
    let age = (now_ms - last_event_ms).max(0);
    let glow = if age < 700 {
        1.0 - (age as f32 / 700.0)
    } else {
        0.0
    };
    let pad = 18.0;
    let dot_x = rect.x + pad + 8.0;
    let dot_y = rect.y + rect.h * 0.5;
    fill_circle(ctx, dot_x, dot_y, 6.0 + 4.0 * glow, Brush::Pulse);

    // KPM block — rolling 60 s sliding window from PulseHistory.
    let kpm = pulse.kpm();
    let kpm_str = super::render::primitives::fmt_int(kpm);
    let kpm_label_rect = Rect::new(rect.x + pad + 26.0, rect.y, 200.0, rect.h);
    text(
        ctx,
        Rect::new(kpm_label_rect.x, kpm_label_rect.y + 8.0, 80.0, 14.0),
        i18n::t("common.kpm"),
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Top,
    );
    text(
        ctx,
        Rect::new(kpm_label_rect.x, kpm_label_rect.y + 22.0, 100.0, 28.0),
        &kpm_str,
        Font::LiveKpm,
        if kpm > 0 { Brush::AccentStrong } else { Brush::Text },
        HAlign::Leading,
        VAlign::Top,
    );

    // Stat trio: last minute / last 5 min / last hour.
    let trio_x = kpm_label_rect.right();
    let block_w = 130.0;
    // Plain English short labels for the live trio — they're time-unit
    // codes more than translatable strings (every locale tends to use
    // either "1 min" or "1m").
    let trio: [(&str, i64); 3] = [
        ("1 min", live.last_minute),
        ("5 min", live.last_5_minutes),
        ("1 h", live.last_hour),
    ];
    for (i, (label, val)) in trio.iter().enumerate() {
        let x = trio_x + i as f32 * block_w;
        let block = Rect::new(x, rect.y, block_w, rect.h);
        text(
            ctx,
            Rect::new(block.x, block.y + 10.0, block.w - 14.0, 14.0),
            label,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
        text(
            ctx,
            Rect::new(block.x, block.y + 24.0, block.w - 14.0, 24.0),
            &super::render::primitives::fmt_int(*val),
            Font::BodyStrong,
            Brush::Text,
            HAlign::Leading,
            VAlign::Top,
        );
    }

    // Right-aligned lifetime pill
    let life_label = i18n::t("dashboard.lifetime");
    let life_value = super::render::primitives::fmt_int(lifetime);
    let life_text = format!("{life_label}  {life_value}");
    let pill_w = (super::render::primitives::measure(ctx, &life_text, Font::BodyStrong, 600.0).0
        + 32.0)
        .min(rect.w - 40.0);
    let pill_rect = Rect::new(
        rect.right() - pill_w - 18.0,
        rect.y + (rect.h - 28.0) * 0.5,
        pill_w,
        28.0,
    );
    fill_rounded(ctx, pill_rect, 14.0, Brush::SurfaceAlt);
    text(
        ctx,
        pill_rect,
        &life_text,
        Font::BodyStrong,
        Brush::Text,
        HAlign::Centre,
        VAlign::Centre,
    );
}
