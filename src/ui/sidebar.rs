// Left rail — view selector + global pause / theme controls.
//
// Width is fixed at 220 DIPs. Inside it we stack five nav rows for
// the views, a divider, the pause button, the theme toggle, and the
// language selector (EN / PL toggle). The exact look mirrors the
// Tauri sidebar: subtle accents, no icons that depend on a font we
// don't bundle.

use crate::app::AppState;
use crate::core::i18n::{self, Lang};
use crate::core::theme::Theme;
use crate::ui::controls::{button, nav_row, ButtonStyle, InputState};
use crate::ui::render::primitives::{
    fill_rect, fill_rounded, line, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};
use crate::ui::views::View;

pub const WIDTH: f32 = 220.0;

/// Output of one sidebar paint — communicates user intent back to the
/// owning Window struct so it can update its state outside the render
/// pass.
#[derive(Default)]
pub struct SidebarOutput {
    pub picked_view: Option<View>,
    pub toggled_pause: bool,
    pub picked_theme: Option<Theme>,
    pub picked_lang: Option<Lang>,
}

pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    current_view: View,
    state: &AppState,
    input: &mut InputState,
) -> SidebarOutput {
    let mut out = SidebarOutput::default();

    // Background — slightly darker than the canvas to delineate.
    fill_rect(ctx, rect, Brush::Surface);
    line(
        ctx,
        rect.right(),
        rect.y,
        rect.right(),
        rect.bottom(),
        Brush::Border,
        1.0,
    );

    // Brand
    let brand = Rect::new(rect.x + 18.0, rect.y + 18.0, rect.w - 36.0, 28.0);
    text(
        ctx,
        brand,
        "KeyCounter",
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Centre,
    );

    // Nav block
    let mut y = rect.y + 64.0;
    let row_h = 36.0;
    let row_gap = 4.0;
    let nav = [
        (View::Dashboard, "▤", "nav.dashboard"),
        (View::Heatmap, "▦", "nav.heatmap"),
        (View::Stats, "≡", "nav.stats"),
        (View::Achievements, "★", "nav.achievements"),
        (View::Settings, "⚙", "nav.settings"),
    ];
    for (view, glyph, key) in nav {
        let row = Rect::new(rect.x, y, rect.w, row_h);
        let label = i18n::t(key);
        let clicked = nav_row(ctx, row, glyph, label, view == current_view, input);
        if clicked {
            out.picked_view = Some(view);
        }
        y += row_h + row_gap;
    }

    // Spacer divider above the bottom block.
    let footer_h = 200.0;
    let div_y = rect.bottom() - footer_h;
    line(
        ctx,
        rect.x + 16.0,
        div_y,
        rect.right() - 16.0,
        div_y,
        Brush::Border,
        1.0,
    );

    // Recording / paused indicator + Pause button
    let paused = state.is_paused();
    let badge = Rect::new(rect.x + 18.0, div_y + 16.0, rect.w - 36.0, 22.0);
    fill_rounded(ctx, badge, 11.0, if paused { Brush::SurfaceAlt } else { Brush::Surface });
    let dot_r = 5.0;
    let dot_x = badge.x + 12.0;
    let dot_y = badge.y + badge.h * 0.5;
    crate::ui::render::primitives::fill_circle(
        ctx,
        dot_x,
        dot_y,
        dot_r,
        if paused { Brush::TextDim } else { Brush::Success },
    );
    text(
        ctx,
        Rect::new(badge.x + 26.0, badge.y, badge.w - 30.0, badge.h),
        i18n::t(if paused { "nav.paused" } else { "nav.recording" }),
        Font::Caption,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Centre,
    );

    let pause_rect = Rect::new(rect.x + 18.0, div_y + 50.0, rect.w - 36.0, 32.0);
    let pause_label = i18n::t(if paused { "nav.resume" } else { "nav.pause" });
    let pause_state = button(
        ctx,
        pause_rect,
        pause_label,
        if paused { ButtonStyle::Primary } else { ButtonStyle::Secondary },
        input,
    );
    if pause_state.clicked {
        out.toggled_pause = true;
    }

    // Theme / lang row
    let small_y = div_y + 96.0;
    let half_w = (rect.w - 36.0 - 8.0) * 0.5;
    let theme_rect = Rect::new(rect.x + 18.0, small_y, half_w, 28.0);
    let lang_rect = Rect::new(theme_rect.right() + 8.0, small_y, half_w, 28.0);
    let theme = i18n::SETTINGS.read().theme;
    let theme_label = match theme {
        Theme::Dark => "Dark",
        Theme::Light => "Light",
    };
    if button(ctx, theme_rect, theme_label, ButtonStyle::Secondary, input).clicked {
        out.picked_theme = Some(match theme {
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::Dark,
        });
    }
    let lang = i18n::SETTINGS.read().lang;
    let lang_label = match lang {
        Lang::En => "EN",
        Lang::Pl => "PL",
    };
    if button(ctx, lang_rect, lang_label, ButtonStyle::Secondary, input).clicked {
        out.picked_lang = Some(match lang {
            Lang::En => Lang::Pl,
            Lang::Pl => Lang::En,
        });
    }

    // Tiny version footer
    let ver = Rect::new(rect.x + 18.0, rect.bottom() - 24.0, rect.w - 36.0, 16.0);
    text(
        ctx,
        ver,
        concat!("v", env!("CARGO_PKG_VERSION")),
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Centre,
    );

    out
}
