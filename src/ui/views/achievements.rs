// Achievements view.
//
// We list the entire catalogue as a responsive 2-column grid of cards.
// Earned achievements light up the accent; locked ones show a progress
// bar against their threshold. The earn-state map is computed once from
// the snapshot — the catalogue is fixed.

use crate::app::AppState;
use crate::core::achievements::{catalog, is_satisfied, progress, Threshold};
use crate::core::i18n;
use crate::ui::controls::InputState;
use crate::ui::render::charts::ratio_bar;
use crate::ui::render::primitives::{
    card, fill_rounded, fmt_int, pill, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};

pub fn draw(ctx: &RenderContext, rect: Rect, state: &AppState, _input: &mut InputState) {
    let snap = state.snapshot.read();
    let lifetime = snap.lifetime;
    let streak = snap.streak;
    let earned_ids: std::collections::HashSet<String> = snap
        .earned_achievements
        .iter()
        .map(|(id, _)| id.clone())
        .collect();

    let pad = 24.0;
    let body = rect.shrink(pad, pad);

    let (title_row, body) = body.split_top(34.0);
    text(
        ctx,
        title_row,
        i18n::t("ach.title"),
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );

    // Summary line: X of Y unlocked
    let total = catalog().len() as i64;
    let unlocked = earned_ids.len() as i64;
    let summary = format!(
        "{} {} {} · {}",
        unlocked,
        i18n::t("common.of"),
        total,
        i18n::t("common.unlocked"),
    );
    let (sub_row, body) = body.split_top(24.0);
    text(
        ctx,
        sub_row,
        &summary,
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // Grid layout: aim for a target column width near 320 DIPs so wider
    // windows give us three or four columns and narrower ones stay at
    // two. Single-column fallback is implicit when body.w drops under
    // ~360.
    let target_card_w = 320.0;
    let gap = 14.0;
    let cols = ((body.w + gap) / (target_card_w + gap)).floor().max(1.0) as usize;
    let card_w = (body.w - gap * (cols as f32 - 1.0)) / cols as f32;
    let card_h = 112.0;

    for (i, def) in catalog().iter().enumerate() {
        let col = i % cols;
        let row = i / cols;
        let x = body.x + col as f32 * (card_w + gap);
        let y = body.y + row as f32 * (card_h + gap);
        let r = Rect::new(x, y, card_w, card_h);
        draw_achievement(ctx, r, def, &earned_ids, lifetime, streak);
    }
}

fn draw_achievement(
    ctx: &RenderContext,
    rect: Rect,
    def: &crate::core::achievements::AchievementDef,
    earned: &std::collections::HashSet<String>,
    lifetime: i64,
    streak: i64,
) {
    let earned_now = earned.contains(def.id) || is_satisfied(def, lifetime, streak);
    card(ctx, rect, 14.0);
    if earned_now {
        // Accent tint behind the card frame — subtle, matches the Tauri
        // version where unlocked cards get a 12 % overlay.
        let tint = rect.shrink(1.0, 1.0);
        fill_rounded(ctx, tint, 13.0, Brush::SurfaceHover);
    }
    let inner = rect.shrink(16.0, 14.0);

    // Title row + status pill
    let (title_row, inner) = inner.split_top(22.0);
    let pill_w = 76.0;
    let pill_rect = Rect::new(
        title_row.right() - pill_w,
        title_row.y + 1.0,
        pill_w,
        title_row.h - 2.0,
    );
    pill(
        ctx,
        pill_rect,
        if earned_now { Brush::Success } else { Brush::SurfaceAlt },
    );
    text(
        ctx,
        pill_rect,
        if earned_now {
            i18n::t("common.unlocked")
        } else {
            i18n::t("common.target")
        },
        Font::Caption,
        if earned_now { Brush::Surface } else { Brush::TextDim },
        HAlign::Centre,
        VAlign::Centre,
    );
    text(
        ctx,
        Rect::new(title_row.x, title_row.y, title_row.w - pill_w - 8.0, title_row.h),
        i18n::t(def.title_key),
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Centre,
    );

    // Description
    let (desc_row, inner) = inner.split_top(18.0);
    text(
        ctx,
        desc_row,
        i18n::t(def.desc_key),
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // Progress bar + numeric breakdown
    let (current, target) = progress(def, lifetime, streak);
    let frac = (current as f32 / target as f32).clamp(0.0, 1.0);
    let (bar_row, num_row) = inner.split_top(16.0);
    let bar_rect = Rect::new(bar_row.x, bar_row.y + 6.0, bar_row.w, 6.0);
    ratio_bar(
        ctx,
        bar_rect,
        frac,
        if earned_now { Brush::Success } else { Brush::Accent },
    );
    let unit_key = match def.threshold {
        Threshold::Lifetime(_) => "common.keystrokes",
        Threshold::Streak(_) => "common.days",
    };
    let label = format!("{} / {} {}", fmt_int(current), fmt_int(target), i18n::t(unit_key));
    text(
        ctx,
        num_row,
        &label,
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Centre,
    );
}
