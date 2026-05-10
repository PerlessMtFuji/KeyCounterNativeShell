// Settings view — everything user-mutable in one panel.
//
// Layout: stacked "section" cards. Each section has a fixed body
// height computed from the sum of its rows. Knowing the height
// upfront lets us paint the filled card frame *before* the contents,
// so rows sit cleanly inside a contained surface (the earlier
// "stroke-only after content" approach left elements visually floating
// past the border whenever any row was a hair taller than budgeted).
//
// Row geometry is deliberate:
//   * label: 20 DIPs (Body)
//   * hint:  18 DIPs (Caption, single line — long hints get trimmed
//            at the call site so they don't wrap into the control row)
//   * control: 24-30 DIPs depending on type
//   * top/bottom inner pad: 4 each
// We pick a single-line hint instead of letting DirectWrite wrap
// because the natural wrap point depends on the column width, which
// varies between single- and two-column layouts.

use crate::app::AppState;
use crate::core::i18n::{self, Lang};
use crate::core::layouts::LayoutId;
use crate::core::theme::Theme;
use crate::ui::controls::{button, slider, toggle, ButtonStyle, InputState};
use crate::ui::render::primitives::{
    fill_rounded, stroke_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};

#[derive(Default)]
pub struct ViewOutput {
    pub picked_theme: Option<Theme>,
    pub settings_dirty: bool,
    pub reset_requested: bool,
    pub export_requested: bool,
    pub autostart_changed: bool,
}

// Row heights — see module comment.
const ROW_TOGGLE_H: f32 = 56.0;
const ROW_SEG_H: f32 = 76.0;
const ROW_SEG_NOHINT_H: f32 = 60.0;
const ROW_SLIDER_H: f32 = 76.0;
const ROW_DATA_H: f32 = 54.0;
const ROW_ABOUT_H: f32 = 22.0;

const SECTION_TITLE_H: f32 = 24.0;
const SECTION_PAD: f32 = 14.0;
const SECTION_GAP: f32 = 14.0;
const COL_GAP: f32 = 18.0;

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

    // 2-column when there's room, otherwise stack.
    let two_col = body.w > 760.0;
    let col_w = if two_col {
        (body.w - COL_GAP) * 0.5
    } else {
        body.w
    };
    let mut left_y = body.y;
    let mut right_y = body.y;

    let current = i18n::SETTINGS.read().clone();

    // ── Left column ──────────────────────────────────────────────
    let rec_h = SECTION_TITLE_H + SECTION_PAD * 2.0 + ROW_TOGGLE_H * 2.0;
    draw_recording_section(
        ctx,
        Rect::new(body.x, left_y, col_w, rec_h),
        state,
        &current,
        input,
    );
    left_y += rec_h + SECTION_GAP;

    let disp_h = SECTION_TITLE_H + SECTION_PAD * 2.0 + ROW_SEG_H + ROW_SEG_NOHINT_H * 2.0;
    draw_display_section(
        ctx,
        Rect::new(body.x, left_y, col_w, disp_h),
        &current,
        input,
    );
    left_y += disp_h + SECTION_GAP;

    // ── Right column ─────────────────────────────────────────────
    let right_x = if two_col { body.x + col_w + COL_GAP } else { body.x };
    let cursor = if two_col { &mut right_y } else { &mut left_y };

    let widget_h =
        SECTION_TITLE_H + SECTION_PAD * 2.0 + ROW_SEG_H + ROW_SLIDER_H + ROW_TOGGLE_H;
    draw_widget_section(
        ctx,
        Rect::new(right_x, *cursor, col_w, widget_h),
        &current,
        input,
    );
    *cursor += widget_h + SECTION_GAP;

    let data_h = SECTION_TITLE_H + SECTION_PAD * 2.0 + ROW_DATA_H * 3.0;
    draw_data_section(
        ctx,
        Rect::new(right_x, *cursor, col_w, data_h),
        state,
        input,
    );
    *cursor += data_h + SECTION_GAP;

    let about_h = SECTION_TITLE_H + SECTION_PAD * 2.0 + ROW_ABOUT_H * 3.0 + 4.0;
    draw_about_section(ctx, Rect::new(right_x, *cursor, col_w, about_h));

    out.picked_theme = OUT_THEME.with(|c| c.take());
    out.reset_requested = OUT_RESET.with(|c| c.take());
    out.export_requested = OUT_EXPORT.with(|c| c.take());
    out.autostart_changed = OUT_AUTOSTART.with(|c| c.take());
    out
}

thread_local! {
    static OUT_THEME: std::cell::Cell<Option<Theme>> = const { std::cell::Cell::new(None) };
    static OUT_RESET: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static OUT_EXPORT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static OUT_AUTOSTART: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

// ---------------------------------------------------------------------------
// Section frame + helpers
// ---------------------------------------------------------------------------

/// Draw the card backdrop (fill + 1 px border) and the title, return
/// the inner content rect for rows.
fn section_frame(ctx: &RenderContext, rect: Rect, title: &str) -> Rect {
    fill_rounded(ctx, rect, 14.0, Brush::Surface);
    stroke_rounded(ctx, rect, 14.0, Brush::Border, 1.0);
    text(
        ctx,
        Rect::new(rect.x + SECTION_PAD + 2.0, rect.y + 10.0, rect.w, SECTION_TITLE_H),
        title,
        Font::BodyStrong,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    Rect::new(
        rect.x + SECTION_PAD,
        rect.y + SECTION_TITLE_H + SECTION_PAD,
        rect.w - SECTION_PAD * 2.0,
        rect.h - SECTION_TITLE_H - SECTION_PAD * 2.0,
    )
}

fn draw_recording_section(
    ctx: &RenderContext,
    rect: Rect,
    state: &AppState,
    current: &i18n::Settings,
    input: &mut InputState,
) {
    let inner = section_frame(ctx, rect, i18n::t("settings.recording"));
    let r1 = Rect::new(inner.x, inner.y, inner.w, ROW_TOGGLE_H);
    let r2 = Rect::new(inner.x, inner.y + ROW_TOGGLE_H, inner.w, ROW_TOGGLE_H);

    row_toggle(
        ctx,
        r1,
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
    row_toggle(
        ctx,
        r2,
        i18n::t("settings.autostartLabel"),
        i18n::t("settings.autostartHint"),
        current.autostart,
        input,
        |new| {
            if new != current.autostart {
                i18n::SETTINGS.write().autostart = new;
                OUT_AUTOSTART.with(|c| c.set(true));
            }
        },
    );
}

fn draw_display_section(
    ctx: &RenderContext,
    rect: Rect,
    current: &i18n::Settings,
    input: &mut InputState,
) {
    let inner = section_frame(ctx, rect, i18n::t("settings.display"));
    let mut y = inner.y;

    let r = Rect::new(inner.x, y, inner.w, ROW_SEG_H);
    let opts: Vec<&str> = LayoutId::ALL.iter().map(|l| l.name()).collect();
    let cur = LayoutId::ALL.iter().position(|l| *l == current.layout).unwrap_or(0);
    row_segmented(
        ctx,
        r,
        i18n::t("settings.layoutLabel"),
        i18n::t("settings.layoutHint"),
        &opts,
        cur,
        input,
        |idx| {
            let new = LayoutId::ALL[idx];
            if new != current.layout {
                i18n::SETTINGS.write().layout = new;
            }
        },
    );
    y += ROW_SEG_H;

    let theme_idx = match current.theme { Theme::Dark => 0, Theme::Light => 1 };
    let r = Rect::new(inner.x, y, inner.w, ROW_SEG_NOHINT_H);
    row_segmented(
        ctx,
        r,
        i18n::t("settings.themeLabel"),
        "",
        &[i18n::t("settings.themeDark"), i18n::t("settings.themeLight")],
        theme_idx,
        input,
        |idx| {
            let new = if idx == 0 { Theme::Dark } else { Theme::Light };
            if new != current.theme {
                i18n::SETTINGS.write().theme = new;
                OUT_THEME.with(|c| c.set(Some(new)));
            }
        },
    );
    y += ROW_SEG_NOHINT_H;

    let lang_idx = match current.lang { Lang::En => 0, Lang::Pl => 1 };
    let r = Rect::new(inner.x, y, inner.w, ROW_SEG_NOHINT_H);
    row_segmented(
        ctx,
        r,
        i18n::t("settings.langLabel"),
        "",
        &[i18n::t("settings.langEn"), i18n::t("settings.langPl")],
        lang_idx,
        input,
        |idx| {
            let new = if idx == 0 { Lang::En } else { Lang::Pl };
            if new != current.lang {
                i18n::SETTINGS.write().lang = new;
            }
        },
    );
}

fn draw_widget_section(
    ctx: &RenderContext,
    rect: Rect,
    current: &i18n::Settings,
    input: &mut InputState,
) {
    let inner = section_frame(ctx, rect, i18n::t("nav.widget"));
    let mut y = inner.y;

    let mode_idx = if current.widget_compact { 1 } else { 0 };
    let r = Rect::new(inner.x, y, inner.w, ROW_SEG_H);
    row_segmented(
        ctx,
        r,
        i18n::t("widget.modeLabel"),
        i18n::t("widget.modeHint"),
        &[i18n::t("widget.modeFull"), i18n::t("widget.modeCompact")],
        mode_idx,
        input,
        |idx| {
            let new = idx == 1;
            if new != current.widget_compact {
                i18n::SETTINGS.write().widget_compact = new;
            }
        },
    );
    y += ROW_SEG_H;

    let r = Rect::new(inner.x, y, inner.w, ROW_SLIDER_H);
    row_slider(
        ctx,
        r,
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
    y += ROW_SLIDER_H;

    let r = Rect::new(inner.x, y, inner.w, ROW_TOGGLE_H);
    row_toggle(
        ctx,
        r,
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
}

fn draw_data_section(
    ctx: &RenderContext,
    rect: Rect,
    state: &AppState,
    input: &mut InputState,
) {
    let inner = section_frame(ctx, rect, i18n::t("settings.data"));
    let mut y = inner.y;

    // DB path row — read-only, two-line: label + truncated path
    let r = Rect::new(inner.x, y, inner.w, ROW_DATA_H);
    text(
        ctx,
        Rect::new(r.x, r.y + 6.0, r.w, 18.0),
        i18n::t("settings.dbPath"),
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    let path = state.db_path.display().to_string();
    text(
        ctx,
        Rect::new(r.x, r.y + 26.0, r.w, 22.0),
        &path,
        Font::Caption,
        Brush::TextMuted,
        HAlign::Leading,
        VAlign::Top,
    );
    y += ROW_DATA_H;

    // Export row
    let r = Rect::new(inner.x, y, inner.w, ROW_DATA_H);
    row_button(
        ctx,
        r,
        i18n::t("settings.exportLabel"),
        i18n::t("settings.exportHint"),
        i18n::t("settings.exportButton"),
        ButtonStyle::Secondary,
        input,
        || OUT_EXPORT.with(|c| c.set(true)),
    );
    y += ROW_DATA_H;

    // Reset row
    let r = Rect::new(inner.x, y, inner.w, ROW_DATA_H);
    row_button(
        ctx,
        r,
        i18n::t("settings.resetLabel"),
        i18n::t("settings.resetHint"),
        i18n::t("settings.resetButton"),
        ButtonStyle::Danger,
        input,
        || OUT_RESET.with(|c| c.set(true)),
    );
}

fn draw_about_section(ctx: &RenderContext, rect: Rect) {
    let inner = section_frame(ctx, rect, i18n::t("settings.about"));
    let lines = [
        (i18n::t("settings.version"), env!("CARGO_PKG_VERSION")),
        (i18n::t("settings.license"), "MIT"),
        (i18n::t("settings.privacy"), i18n::t("settings.privacyValue")),
    ];
    for (i, (k, v)) in lines.iter().enumerate() {
        let y = inner.y + i as f32 * ROW_ABOUT_H;
        text(
            ctx,
            Rect::new(inner.x, y, inner.w * 0.4, ROW_ABOUT_H),
            k,
            Font::Body,
            Brush::TextDim,
            HAlign::Leading,
            VAlign::Centre,
        );
        text(
            ctx,
            Rect::new(inner.x + inner.w * 0.4, y, inner.w * 0.6, ROW_ABOUT_H),
            v,
            Font::Body,
            Brush::Text,
            HAlign::Leading,
            VAlign::Centre,
        );
    }
}

// ---------------------------------------------------------------------------
// Row primitives
// ---------------------------------------------------------------------------

fn row_toggle<F: FnOnce(bool)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    value: bool,
    input: &mut InputState,
    on_change: F,
) {
    let toggle_w = 44.0;
    let text_w = rect.w - toggle_w - 12.0;
    text(
        ctx,
        Rect::new(rect.x, rect.y + 6.0, text_w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 26.0, text_w, 22.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let toggle_rect = Rect::new(rect.right() - toggle_w, rect.y + (rect.h - 22.0) * 0.5, toggle_w, 22.0);
    let (new_value, _) = toggle(ctx, toggle_rect, value, input);
    if new_value != value {
        on_change(new_value);
    }
}

fn row_slider<F: FnOnce(u8)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    value: u8,
    min: u8,
    max: u8,
    input: &mut InputState,
    on_change: F,
) {
    text(
        ctx,
        Rect::new(rect.x, rect.y + 4.0, rect.w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 22.0, rect.w, 18.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let track_y = rect.bottom() - 22.0;
    let value_w = 54.0;
    let track_rect = Rect::new(rect.x, track_y, rect.w - value_w, 20.0);
    let (new_value, _) = slider(ctx, track_rect, value, min, max, input);
    if new_value != value {
        on_change(new_value);
    }
    text(
        ctx,
        Rect::new(rect.right() - value_w, track_y, value_w, 20.0),
        &format!("{}%", new_value),
        Font::Body,
        Brush::TextDim,
        HAlign::Trailing,
        VAlign::Centre,
    );
}

fn row_segmented<F: FnOnce(usize)>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    options: &[&str],
    current: usize,
    input: &mut InputState,
    on_change: F,
) {
    text(
        ctx,
        Rect::new(rect.x, rect.y + 2.0, rect.w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 20.0, rect.w, 18.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let seg_h = 30.0;
    let seg_rect = Rect::new(rect.x, rect.bottom() - seg_h - 2.0, rect.w, seg_h);
    let n = options.len().max(1);
    let cell_w = seg_rect.w / n as f32;
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
            PENDING_SEG.with(|c| c.set(Some(i)));
        }
    }
    if let Some(idx) = PENDING_SEG.with(|c| c.take()) {
        on_change(idx);
    }
}

thread_local! {
    static PENDING_SEG: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

fn row_button<F: FnOnce()>(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    hint: &str,
    btn_label: &str,
    style: ButtonStyle,
    input: &mut InputState,
    on_click: F,
) {
    let btn_w = 116.0;
    let text_w = rect.w - btn_w - 12.0;
    text(
        ctx,
        Rect::new(rect.x, rect.y + 6.0, text_w, 18.0),
        label,
        Font::Body,
        Brush::Text,
        HAlign::Leading,
        VAlign::Top,
    );
    if !hint.is_empty() {
        text(
            ctx,
            Rect::new(rect.x, rect.y + 26.0, text_w, 22.0),
            hint,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Leading,
            VAlign::Top,
        );
    }
    let btn_rect = Rect::new(rect.right() - btn_w, rect.y + (rect.h - 30.0) * 0.5, btn_w, 30.0);
    if button(ctx, btn_rect, btn_label, style, input).clicked {
        on_click();
    }
}
