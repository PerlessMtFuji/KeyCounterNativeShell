// Settings view — everything user-mutable in one panel.
//
// Layout: stacked "section" cards, each with a header + a list of
// rows. Every row is `label / hint` on the left and a control on the
// right, mirroring the Tauri Settings page. The settings global
// (`i18n::SETTINGS`) is the source of truth — we read it at the top of
// the function, hand each control its current value, then commit any
// changes back through `ViewOutput` so the main window can react
// (theme rebuild, autostart registry write, etc.).

use crate::app::AppState;
use crate::core::i18n::{self, Lang};
use crate::core::layouts::LayoutId;
use crate::core::theme::Theme;
use crate::ui::controls::{button, slider, toggle, ButtonStyle, InputState};
use crate::ui::render::primitives::{
    fill_rounded, stroke_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};

/// What the settings view wants the owning window to do after a paint.
/// Lives in the same module — settings is the only view that can
/// mutate cross-cutting state today.
#[derive(Default)]
pub struct ViewOutput {
    pub picked_theme: Option<Theme>,
    /// Settings that just need a re-paint / persistence pass — main
    /// window doesn't need to do anything special, just flush the
    /// settings file. Push 4 handles the actual write-through.
    pub settings_dirty: bool,
    pub reset_requested: bool,
    pub export_requested: bool,
}

pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    state: &AppState,
    input: &mut InputState,
) -> ViewOutput {
    let mut out = ViewOutput::default();
    let pad = 24.0;
    let body = rect.shrink(pad, pad);

    let (title_row, body) = body.split_top(34.0);
    text(
        ctx,
        title_row,
        i18n::t("settings.title"),
        Font::Heading,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let (sub_row, body) = body.split_top(24.0);
    text(
        ctx,
        sub_row,
        i18n::t("settings.subtitle"),
        Font::Body,
        Brush::TextDim,
        HAlign::Leading,
        VAlign::Top,
    );

    // We split the body into two columns on wide windows so it doesn't
    // turn into a long scroll-down list. The threshold (≈ 900 DIPs of
    // usable body width) lines up with the min-window clamp once you
    // subtract sidebar + padding.
    let two_col = body.w > 720.0;
    let col_gap = 18.0;
    let col_w = if two_col {
        (body.w - col_gap) * 0.5
    } else {
        body.w
    };
    let mut cursor_l = body.y;
    let mut cursor_r = body.y;

    // Read the live settings snapshot once. We mutate via the lock guard
    // only when a control fires — the read+write split keeps the
    // borrow scope tight.
    let current = { i18n::SETTINGS.read().clone() };

    // Section: Recording
    let section_h = section(
        ctx,
        Rect::new(body.x, cursor_l, col_w, 0.0),
        i18n::t("settings.recording"),
        |row_rect, input| {
            let mut h = 0.0;
            // Pause toggle
            h += row_with_toggle(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 56.0),
                i18n::t("settings.pauseLabel"),
                i18n::t("settings.pauseHint"),
                state.is_paused(),
                input,
                |new| {
                    if new != state.is_paused() {
                        state.set_paused(new);
                    }
                },
            );
            // Autostart toggle (no-op wiring yet — Push 4 lands the
            // registry write)
            h += row_with_toggle(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 56.0),
                i18n::t("settings.autostartLabel"),
                i18n::t("settings.autostartHint"),
                current.autostart,
                input,
                |new| {
                    if new != current.autostart {
                        i18n::SETTINGS.write().autostart = new;
                    }
                },
            );
            h
        },
        input,
    );
    cursor_l += section_h + 16.0;

    // Section: Display
    let section_h = section(
        ctx,
        Rect::new(body.x, cursor_l, col_w, 0.0),
        i18n::t("settings.display"),
        |row_rect, input| {
            let mut h = 0.0;
            // Layout selector — segmented control across four options.
            h += row_with_segmented(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 72.0),
                i18n::t("settings.layoutLabel"),
                i18n::t("settings.layoutHint"),
                input,
                &LayoutId::ALL.iter().map(|l| l.name()).collect::<Vec<_>>(),
                LayoutId::ALL.iter().position(|l| *l == current.layout).unwrap_or(0),
                |idx| {
                    let new = LayoutId::ALL[idx];
                    if new != current.layout {
                        i18n::SETTINGS.write().layout = new;
                    }
                },
            );

            // Theme — two-option segmented.
            let theme_idx = match current.theme {
                Theme::Dark => 0,
                Theme::Light => 1,
            };
            h += row_with_segmented(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 64.0),
                i18n::t("settings.themeLabel"),
                "",
                input,
                &[i18n::t("settings.themeDark"), i18n::t("settings.themeLight")],
                theme_idx,
                |idx| {
                    let new = if idx == 0 { Theme::Dark } else { Theme::Light };
                    if new != current.theme {
                        i18n::SETTINGS.write().theme = new;
                        // Pin this — main window picks it up after the
                        // paint to rebuild brushes.
                        OUT_THEME.with(|c| c.set(Some(new)));
                    }
                },
            );

            // Language — two-option segmented.
            let lang_idx = match current.lang {
                Lang::En => 0,
                Lang::Pl => 1,
            };
            h += row_with_segmented(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 64.0),
                i18n::t("settings.langLabel"),
                "",
                input,
                &[i18n::t("settings.langEn"), i18n::t("settings.langPl")],
                lang_idx,
                |idx| {
                    let new = if idx == 0 { Lang::En } else { Lang::Pl };
                    if new != current.lang {
                        i18n::SETTINGS.write().lang = new;
                    }
                },
            );
            h
        },
        input,
    );
    cursor_l += section_h + 16.0;

    // Right column — widget + data + about
    let col_r_x = if two_col { body.x + col_w + col_gap } else { body.x };
    let active_cursor = if two_col { &mut cursor_r } else { &mut cursor_l };

    // Section: Widget
    let section_h = section(
        ctx,
        Rect::new(col_r_x, *active_cursor, col_w, 0.0),
        i18n::t("nav.widget"),
        |row_rect, input| {
            let mut h = 0.0;
            let mode_idx = if current.widget_compact { 1 } else { 0 };
            h += row_with_segmented(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 72.0),
                i18n::t("widget.modeLabel"),
                i18n::t("widget.modeHint"),
                input,
                &[i18n::t("widget.modeFull"), i18n::t("widget.modeCompact")],
                mode_idx,
                |idx| {
                    let new = idx == 1;
                    if new != current.widget_compact {
                        i18n::SETTINGS.write().widget_compact = new;
                    }
                },
            );

            h += row_with_slider(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 72.0),
                i18n::t("widget.opacityLabel"),
                i18n::t("widget.opacityHint"),
                current.widget_opacity,
                10,
                100,
                input,
                |v| {
                    if v != current.widget_opacity {
                        i18n::SETTINGS.write().widget_opacity = v;
                    }
                },
            );

            h += row_with_toggle(
                ctx,
                Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 56.0),
                i18n::t("widget.snapLabel"),
                i18n::t("widget.snapHint"),
                current.widget_snap,
                input,
                |new| {
                    if new != current.widget_snap {
                        i18n::SETTINGS.write().widget_snap = new;
                    }
                },
            );
            h
        },
        input,
    );
    *active_cursor += section_h + 16.0;

    // Section: Data
    let section_h = section(
        ctx,
        Rect::new(col_r_x, *active_cursor, col_w, 0.0),
        i18n::t("settings.data"),
        |row_rect, input| {
            let mut h = 0.0;
            // DB path — read-only label
            let row = Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 50.0);
            h += row.h;
            text(
                ctx,
                Rect::new(row.x, row.y + 6.0, row.w, 18.0),
                i18n::t("settings.dbPath"),
                Font::Body,
                Brush::Text,
                HAlign::Leading,
                VAlign::Top,
            );
            let path = state.db_path.display().to_string();
            text(
                ctx,
                Rect::new(row.x, row.y + 26.0, row.w, 18.0),
                &path,
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Top,
            );

            // Export button
            let row = Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 52.0);
            h += row.h;
            text(
                ctx,
                Rect::new(row.x, row.y + 4.0, row.w * 0.55, 18.0),
                i18n::t("settings.exportLabel"),
                Font::Body,
                Brush::Text,
                HAlign::Leading,
                VAlign::Top,
            );
            text(
                ctx,
                Rect::new(row.x, row.y + 24.0, row.w * 0.55, 18.0),
                i18n::t("settings.exportHint"),
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Top,
            );
            let btn_w = 120.0;
            let btn_rect =
                Rect::new(row.right() - btn_w, row.y + 10.0, btn_w, 32.0);
            if button(
                ctx,
                btn_rect,
                i18n::t("settings.exportButton"),
                ButtonStyle::Secondary,
                input,
            )
            .clicked
            {
                OUT_EXPORT.with(|c| c.set(true));
            }

            // Reset button
            let row = Rect::new(row_rect.x, row_rect.y + h, row_rect.w, 52.0);
            h += row.h;
            text(
                ctx,
                Rect::new(row.x, row.y + 4.0, row.w * 0.55, 18.0),
                i18n::t("settings.resetLabel"),
                Font::Body,
                Brush::Text,
                HAlign::Leading,
                VAlign::Top,
            );
            text(
                ctx,
                Rect::new(row.x, row.y + 24.0, row.w * 0.55, 18.0),
                i18n::t("settings.resetHint"),
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Top,
            );
            let btn_w = 120.0;
            let btn_rect = Rect::new(row.right() - btn_w, row.y + 10.0, btn_w, 32.0);
            if button(
                ctx,
                btn_rect,
                i18n::t("settings.resetButton"),
                ButtonStyle::Danger,
                input,
            )
            .clicked
            {
                OUT_RESET.with(|c| c.set(true));
            }

            h
        },
        input,
    );
    *active_cursor += section_h + 16.0;

    // Section: About
    let section_h = section(
        ctx,
        Rect::new(col_r_x, *active_cursor, col_w, 0.0),
        i18n::t("settings.about"),
        |row_rect, _input| {
            let lines = [
                (i18n::t("settings.version"), env!("CARGO_PKG_VERSION")),
                (i18n::t("settings.license"), "MIT"),
                (i18n::t("settings.privacy"), i18n::t("settings.privacyValue")),
            ];
            let row_h = 24.0;
            for (i, (k, v)) in lines.iter().enumerate() {
                let y = row_rect.y + i as f32 * row_h;
                text(
                    ctx,
                    Rect::new(row_rect.x, y, row_rect.w * 0.4, row_h),
                    k,
                    Font::Body,
                    Brush::TextDim,
                    HAlign::Leading,
                    VAlign::Centre,
                );
                text(
                    ctx,
                    Rect::new(row_rect.x + row_rect.w * 0.4, y, row_rect.w * 0.6, row_h),
                    v,
                    Font::Body,
                    Brush::Text,
                    HAlign::Leading,
                    VAlign::Centre,
                );
            }
            lines.len() as f32 * row_h + 4.0
        },
        input,
    );
    *active_cursor += section_h + 16.0;

    // Drain the thread-local outputs into the ViewOutput. We use TLS
    // because the row closures already borrow `out` indirectly through
    // a chain of FnMut captures — punting cross-section state through
    // a Cell sidesteps the borrow checker without adding lifetimes
    // everywhere.
    out.picked_theme = OUT_THEME.with(|c| c.take());
    out.reset_requested = OUT_RESET.with(|c| c.take());
    out.export_requested = OUT_EXPORT.with(|c| c.take());
    out
}

thread_local! {
    static OUT_THEME: std::cell::Cell<Option<Theme>> = const { std::cell::Cell::new(None) };
    static OUT_RESET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static OUT_EXPORT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Draw a section frame (heading + body) and return its total height.
/// `body_fn` receives the inner content rect and reports the height it
/// actually consumed.
fn section<F>(
    ctx: &RenderContext,
    placeholder: Rect,
    title: &str,
    mut body_fn: F,
    input: &mut InputState,
) -> f32
where
    F: FnMut(Rect, &mut InputState) -> f32,
{
    // First pass — measure: we render the body to a "dry" location so
    // we know how tall the actual card needs to be. To avoid drawing
    // twice we just budget a typical maximum (380 DIPs) and clip; if
    // the body returns less, we trim the card frame on a second draw
    // pass over the same coordinates. In practice the controls return
    // their actual heights so the trim is exact.
    let title_h = 26.0;
    let inner_pad_x = 18.0;
    let inner_pad_y = 14.0;

    // Reserve a generous slab to draw into. The card frame stays
    // invisible (alpha 0) until we know the real height — which is
    // fine because every primitive uses Direct2D's painter's algorithm.
    // We draw the frame first as transparent (no-op via SurfaceAlt) and
    // overlay once we know the size.
    let body_x = placeholder.x + inner_pad_x;
    let body_y = placeholder.y + title_h + inner_pad_y;
    let body_w = placeholder.w - 2.0 * inner_pad_x;
    let body_rect = Rect::new(body_x, body_y, body_w, 400.0);

    let body_h = body_fn(body_rect, input);
    let total_h = title_h + inner_pad_y * 2.0 + body_h;
    let card_rect = Rect::new(placeholder.x, placeholder.y, placeholder.w, total_h);

    // Draw a faint outlined card behind everything we already painted.
    // D2D is painter's order, so this would obscure the contents. We
    // work around that by stroking only the border (no fill).
    stroke_rounded(ctx, card_rect, 14.0, Brush::Border, 1.0);

    // Title sits inside the top inset.
    text(
        ctx,
        Rect::new(card_rect.x + inner_pad_x, card_rect.y + 8.0, card_rect.w, title_h),
        title,
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );

    total_h
}

fn row_with_toggle<F: FnOnce(bool)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    value: bool,
    input: &mut InputState,
    on_change: F,
) -> f32 {
    let label_w = rect.w * 0.6;
    text(
        ctx,
        Rect::new(rect.x, rect.y + 6.0, label_w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 26.0, label_w, 22.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let toggle_w = 44.0;
    let toggle_rect = Rect::new(rect.right() - toggle_w, rect.y + 16.0, toggle_w, 24.0);
    let (new_value, _) = toggle(ctx, toggle_rect, value, input);
    if new_value != value {
        on_change(new_value);
    }
    rect.h
}

fn row_with_slider<F: FnOnce(u8)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    value: u8,
    min: u8,
    max: u8,
    input: &mut InputState,
    on_change: F,
) -> f32 {
    text(
        ctx,
        Rect::new(rect.x, rect.y + 6.0, rect.w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 26.0, rect.w * 0.6, 22.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let slider_w = (rect.w * 0.5).max(160.0);
    let slider_rect = Rect::new(rect.right() - slider_w, rect.y + 36.0, slider_w - 56.0, 24.0);
    let (new_value, _) = slider(ctx, slider_rect, value, min, max, input);
    if new_value != value {
        on_change(new_value);
    }
    text(
        ctx,
        Rect::new(slider_rect.right() + 8.0, slider_rect.y, 48.0, slider_rect.h),
        &format!("{}%", new_value),
        Font::Body,
        Brush::TextDim,
        HAlign::Trailing,
        VAlign::Centre,
    );
    rect.h
}

fn row_with_segmented<F: FnOnce(usize)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    input: &mut InputState,
    options: &[&str],
    current: usize,
    on_change: F,
) -> f32 {
    text(
        ctx,
        Rect::new(rect.x, rect.y + 6.0, rect.w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 26.0, rect.w, 30.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }

    // The segmented selector lives along the bottom of the row.
    let seg_h = 30.0;
    let seg_rect = Rect::new(rect.x, rect.bottom() - seg_h - 4.0, rect.w, seg_h);
    let n = options.len().max(1);
    let cell_w = seg_rect.w / n as f32;

    // Background track + outline
    fill_rounded(ctx, seg_rect, 8.0, Brush::SurfaceAlt);
    for (i, opt) in options.iter().enumerate() {
        let cell = Rect::new(seg_rect.x + i as f32 * cell_w, seg_rect.y, cell_w, seg_rect.h);
        let active = i == current;
        if active {
            fill_rounded(ctx, cell.shrink(2.0, 2.0), 6.0, Brush::Accent);
        } else if input.hit(cell) {
            fill_rounded(ctx, cell.shrink(2.0, 2.0), 6.0, Brush::SurfaceHover);
        }
        text(
            ctx,
            cell,
            opt,
            Font::Body,
            if active { Brush::Surface } else { Brush::Text },
            HAlign::Centre,
            VAlign::Centre,
        );
        if input.consume_click(cell) && i != current {
            // We can't call on_change from inside the loop without
            // turning it into FnMut; stash the index in TLS and apply
            // after the loop.
            PENDING_SEG.with(|c| c.set(Some(i)));
        }
    }

    if let Some(idx) = PENDING_SEG.with(|c| c.take()) {
        on_change(idx);
    }

    rect.h
}

thread_local! {
    static PENDING_SEG: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
