// Top-bar live status: KPM, last-minute, last-hour, and a thin pulse
// indicator that flashes when a keystroke arrives.
//
// The pulse animation rides the 60 FPS `T_ANIM` timer the main window
// spins up on each keystroke. The glow shape is "solid 5 px dot + soft
// alpha halo that decays on an ease-out cubic" — same curve the
// original CSS version used. We build the halo brush on the fly from
// the palette's pulse colour rather than caching another brush slot,
// because the alpha varies smoothly across frames.

use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_POINT_2F};
use windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE;

use crate::core::stats::PulseHistory;
use crate::ui::render::primitives::{
    fill_circle, fill_rect, fill_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{create_solid_brush, Brush, Font, RenderContext};
use crate::core::i18n;
use crate::core::store::LiveSnapshot;

pub const HEIGHT: f32 = 56.0;

/// One ripple completes its travel from the centre dot out to MAX_R in
/// this many ms. Two ripples are spawned half a period out of phase so
/// the visual cadence stays continuous while the user types.
const RIPPLE_PERIOD_MS: f32 = 1100.0;
/// After this many ms of typing silence the ripples stop spawning. The
/// dot itself stays visible — only the wave halts.
const RIPPLE_TRAIL_MS: i64 = 1400;
const RIPPLE_INNER_R: f32 = 7.0;
const RIPPLE_OUTER_R: f32 = 22.0;

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

    // Wave-style pulse indicator.
    //
    // The previous version drew a bright orange halo that solid-filled
    // the dot's surroundings while the user typed; in steady-state
    // typing the halo never had a chance to decay and just sat as a
    // permanent ring around the dot. We replace it with two concentric
    // stroked ripples that travel outward on a continuous schedule. As
    // long as a keystroke landed within RIPPLE_TRAIL_MS the rings keep
    // expanding from the centre and fading; once the user stops typing
    // the rings finish their current pass and the dot rests alone.
    //
    // Colour: accent (the same blue/teal as the rest of the UI) instead
    // of pulse-orange — feels less alarmy and matches the KPM digits.
    let age = (now_ms - last_event_ms).max(0);
    let pad = 18.0;
    let dot_x = rect.x + pad + 8.0;
    let dot_y = rect.y + rect.h * 0.5;

    let active = age < RIPPLE_TRAIL_MS;
    if active {
        // Continuous time-based phase so the two rings stay locked to
        // their offsets even across keystrokes. We restart the clock at
        // the last event so the first ring after a long pause emerges
        // cleanly from the centre instead of mid-flight.
        let t = (age as f32 / RIPPLE_PERIOD_MS).max(0.0);
        // Fade the *spawn* amplitude as the trail nears its end so the
        // ripples don't pop off when the timer cuts.
        let trail_fade = {
            let f = 1.0 - (age as f32 / RIPPLE_TRAIL_MS as f32).clamp(0.0, 1.0);
            // Ease out cubic so the tail looks like a deliberate wind-down
            // rather than a linear ramp.
            1.0 - (1.0 - f).powi(3)
        };
        let p = ctx.palette().accent;
        for offset in [0.0_f32, 0.5] {
            let phase = (t + offset).fract();
            // Skip the very first sliver — it'd render a hairline
            // exactly on top of the dot.
            if phase < 0.02 {
                continue;
            }
            let radius = RIPPLE_INNER_R + (RIPPLE_OUTER_R - RIPPLE_INNER_R) * phase;
            // Quadratic alpha decay — gives a soft tail without
            // depending on D2D blur filters.
            let alpha = (1.0 - phase).powi(2) * 0.55 * trail_fade;
            if alpha < 0.02 {
                continue;
            }
            let color = D2D1_COLOR_F {
                r: p[0],
                g: p[1],
                b: p[2],
                a: alpha,
            };
            if let Ok(brush) = create_solid_brush(&ctx.target, color) {
                let ellipse = D2D1_ELLIPSE {
                    point: D2D_POINT_2F { x: dot_x, y: dot_y },
                    radiusX: radius,
                    radiusY: radius,
                };
                // 1.5-DIP stroke reads as a single clean line at every
                // DPI we render at; thicker rings look heavy at 200 %.
                unsafe {
                    ctx.target.DrawEllipse(&ellipse, &brush, 1.5, None);
                }
            }
        }
    }

    // Centre dot — accent-coloured. The fill swaps to accent_strong
    // while a keystroke is "fresh" (≤ 220 ms) for a subtle confirmation
    // flash, without the old harsh orange.
    let fresh = age < 220;
    fill_circle(
        ctx,
        dot_x,
        dot_y,
        4.5,
        if fresh { Brush::AccentStrong } else { Brush::Accent },
    );

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
