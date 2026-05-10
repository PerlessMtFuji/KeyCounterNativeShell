// Top-bar live status: KPM, last-minute, last-hour, and a thin pulse
// indicator that flashes when a keystroke arrives.
//
// The pulse animation rides the 60 FPS `T_ANIM` timer the main window
// spins up on each keystroke. The glow shape is "solid 5 px dot + soft
// alpha halo that decays on an ease-out cubic" — same curve the
// original CSS version used. We build the halo brush on the fly from
// the palette's pulse colour rather than caching another brush slot,
// because the alpha varies smoothly across frames.

use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

use crate::core::stats::PulseHistory;
use crate::ui::render::primitives::{
    fill_circle, fill_rect, fill_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{create_solid_brush, Brush, Font, RenderContext};
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

    // Pulse dot — solid 5.5 px core + an alpha halo that breathes out
    // on every keystroke. The 900 ms window matches PULSE_GLOW_MS; we
    // shape the decay with an ease-out cubic (1-(1-t)^3) so the impact
    // hits fast and the tail falls off smoothly, the way the original
    // CSS animation did.
    let age = (now_ms - last_event_ms).max(0);
    let glow_window: i64 = 900;
    let t = (1.0 - (age as f32 / glow_window as f32)).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - t).powi(3);
    let pad = 18.0;
    let dot_x = rect.x + pad + 8.0;
    let dot_y = rect.y + rect.h * 0.5;
    // Soft halo first — one-shot brush at 35 % alpha × ease. Direct2D
    // brush creation is cheap (≈ µs); we'd cache if this loop ran 60
    // times per frame, but it doesn't.
    if ease > 0.02 {
        let p = ctx.palette().pulse;
        let halo_color = D2D1_COLOR_F {
            r: p[0],
            g: p[1],
            b: p[2],
            a: 0.35 * ease,
        };
        if let Ok(halo) = create_solid_brush(&ctx.target, halo_color) {
            let ellipse = windows::Win32::Graphics::Direct2D::D2D1_ELLIPSE {
                point: windows::Win32::Graphics::Direct2D::Common::D2D_POINT_2F {
                    x: dot_x,
                    y: dot_y,
                },
                radiusX: 8.0 + 14.0 * ease,
                radiusY: 8.0 + 14.0 * ease,
            };
            unsafe {
                ctx.target.FillEllipse(&ellipse, &halo);
            }
        }
    }
    fill_circle(ctx, dot_x, dot_y, 5.5, Brush::Pulse);

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
