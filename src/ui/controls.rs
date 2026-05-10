// Immediate-mode controls.
//
// Each control is a `draw_xxx` function that takes its rect, the
// current input state, and an output flag for "was this clicked /
// changed". This style is far simpler than a retained-mode widget
// tree for our case (a few dozen interactive elements total) and
// keeps painting straight-line — no event dispatch tree, no widget
// allocations, the call site stays where the layout is.
//
// Hover / pressed state is tracked at the call site via the InputState
// struct, which the main window owns and updates from WM_MOUSEMOVE /
// WM_LBUTTONDOWN / WM_LBUTTONUP.

use crate::core::theme::Theme;
use crate::ui::render::primitives::{
    fill_circle, fill_rounded, stroke_rounded, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};

#[derive(Default, Debug)]
pub struct InputState {
    pub mouse_x: f32,
    pub mouse_y: f32,
    pub mouse_down: bool,
    /// Set true on the WM_LBUTTONUP that completed a click. The
    /// controls below consume this — exactly one control per frame can
    /// observe a click, and consumers reset it via `consume_click`.
    pub click_pending: bool,
}

impl InputState {
    pub fn hit(&self, r: Rect) -> bool {
        r.contains(self.mouse_x, self.mouse_y)
    }

    pub fn consume_click(&mut self, r: Rect) -> bool {
        if self.click_pending && r.contains(self.mouse_x, self.mouse_y) {
            self.click_pending = false;
            true
        } else {
            false
        }
    }
}

/// Outcome of a single hit-testable control. `clicked` fires once on
/// the matching mouse-up; `hovered` is the every-frame state that
/// drives visual highlighting.
#[derive(Debug, Default)]
pub struct ControlState {
    pub hovered: bool,
    pub clicked: bool,
}

#[derive(Copy, Clone, Debug)]
pub enum ButtonStyle {
    /// Filled accent — primary CTA.
    Primary,
    /// Subtle surface — secondary action.
    Secondary,
    /// Transparent — tertiary, fits inline with body text.
    Ghost,
    /// Warning — for destructive actions like Reset.
    Danger,
}

pub fn button(
    ctx: &RenderContext,
    rect: Rect,
    label: &str,
    style: ButtonStyle,
    input: &mut InputState,
) -> ControlState {
    let hovered = input.hit(rect);
    let clicked = input.consume_click(rect);

    let radius = (rect.h * 0.5).min(10.0);
    let (bg, fg, border) = match style {
        ButtonStyle::Primary => {
            let bg = if hovered { Brush::AccentStrong } else { Brush::Accent };
            (Some(bg), Brush::Surface, None)
        }
        ButtonStyle::Secondary => {
            let bg = if hovered { Brush::SurfaceHover } else { Brush::Surface };
            (Some(bg), Brush::Text, Some(Brush::Border))
        }
        ButtonStyle::Ghost => {
            let bg = if hovered { Some(Brush::SurfaceHover) } else { None };
            (bg, Brush::Text, None)
        }
        ButtonStyle::Danger => {
            let bg = if hovered { Brush::Warn } else { Brush::Surface };
            let fg = if hovered { Brush::Surface } else { Brush::Warn };
            (Some(bg), fg, Some(Brush::Warn))
        }
    };

    if let Some(b) = bg {
        fill_rounded(ctx, rect, radius, b);
    }
    if let Some(b) = border {
        stroke_rounded(ctx, rect, radius, b, 1.0);
    }
    text(
        ctx,
        rect,
        label,
        Font::BodyStrong,
        fg,
        HAlign::Centre,
        VAlign::Centre,
    );

    ControlState { hovered, clicked }
}

/// Toggle pill — tracks an on/off boolean. Returns the new value (the
/// caller writes it back to its state).
pub fn toggle(
    ctx: &RenderContext,
    rect: Rect,
    value: bool,
    input: &mut InputState,
) -> (bool, ControlState) {
    let hovered = input.hit(rect);
    let clicked = input.consume_click(rect);
    let new_value = if clicked { !value } else { value };

    let track_h = rect.h.min(20.0);
    let track = Rect::new(rect.x, rect.y + (rect.h - track_h) * 0.5, rect.w, track_h);
    let bg = if new_value {
        Brush::Accent
    } else if hovered {
        Brush::SurfaceHover
    } else {
        Brush::SurfaceAlt
    };
    fill_rounded(ctx, track, track_h * 0.5, bg);

    let knob_r = (track_h - 4.0) * 0.5;
    let knob_x = if new_value {
        track.right() - knob_r - 2.0
    } else {
        track.x + knob_r + 2.0
    };
    fill_circle(
        ctx,
        knob_x,
        track.y + track.h * 0.5,
        knob_r,
        Brush::Surface,
    );

    (new_value, ControlState { hovered, clicked })
}

/// Theme toggle — same as toggle() but with sun / moon glyphs and the
/// theme-aware label. Convenience because this exact pattern appears
/// on the sidebar.
pub fn theme_toggle(
    ctx: &RenderContext,
    rect: Rect,
    current: Theme,
    input: &mut InputState,
) -> Theme {
    let value = matches!(current, Theme::Light);
    let (new_value, _) = toggle(ctx, rect, value, input);
    if new_value {
        Theme::Light
    } else {
        Theme::Dark
    }
}

/// Slider for the widget opacity setting. Returns the new u8 value
/// (0..=100). Track width is rect.w; thumb is a 12-DIP circle.
pub fn slider(
    ctx: &RenderContext,
    rect: Rect,
    value: u8,
    min: u8,
    max: u8,
    input: &mut InputState,
) -> (u8, ControlState) {
    let hovered = input.hit(rect);
    let span = (max - min).max(1) as f32;
    let mut new_value = value;

    // Drag-or-click semantics: while the mouse is held *and* the
    // initial press landed inside the track, every frame we recompute
    // the value from the cursor position. The press-tracking state
    // belongs to the caller (we keep this control state-free) so we
    // approximate by reacting to mouse_down + hovered. Good enough
    // for low-precision sliders like opacity.
    if input.mouse_down && hovered {
        let frac = ((input.mouse_x - rect.x) / rect.w.max(1.0)).clamp(0.0, 1.0);
        new_value = (min as f32 + frac * span).round() as u8;
    }
    let clicked = input.consume_click(rect);
    if clicked && !input.mouse_down {
        // bare click without drag — snap to clicked position
        let frac = ((input.mouse_x - rect.x) / rect.w.max(1.0)).clamp(0.0, 1.0);
        new_value = (min as f32 + frac * span).round() as u8;
    }

    let track_h = 4.0;
    let track = Rect::new(
        rect.x,
        rect.y + (rect.h - track_h) * 0.5,
        rect.w,
        track_h,
    );
    fill_rounded(ctx, track, track_h * 0.5, Brush::SurfaceAlt);
    let frac = (new_value - min) as f32 / span;
    let lit = Rect::new(track.x, track.y, track.w * frac, track.h);
    fill_rounded(ctx, lit, track_h * 0.5, Brush::Accent);
    let thumb_r = 7.0;
    let thumb_x = track.x + track.w * frac;
    fill_circle(
        ctx,
        thumb_x,
        track.y + track.h * 0.5,
        thumb_r,
        Brush::Accent,
    );

    (new_value, ControlState { hovered, clicked })
}

/// Sidebar nav row — icon/glyph + label + active-state indicator.
/// Returns clicked.
pub fn nav_row(
    ctx: &RenderContext,
    rect: Rect,
    glyph: &str,
    label: &str,
    active: bool,
    input: &mut InputState,
) -> bool {
    let hovered = input.hit(rect);
    let clicked = input.consume_click(rect);

    // Active indicator — vertical accent stripe on the left.
    if active {
        let bar = Rect::new(rect.x, rect.y + 6.0, 3.0, rect.h - 12.0);
        fill_rounded(ctx, bar, 1.5, Brush::Accent);
    }
    if hovered || active {
        let bg = Rect::new(rect.x + 4.0, rect.y + 2.0, rect.w - 8.0, rect.h - 4.0);
        fill_rounded(
            ctx,
            bg,
            8.0,
            if active { Brush::SurfaceAlt } else { Brush::SurfaceHover },
        );
    }
    text(
        ctx,
        Rect::new(rect.x + 14.0, rect.y, 22.0, rect.h),
        glyph,
        Font::Nav,
        if active { Brush::AccentStrong } else { Brush::TextDim },
        HAlign::Centre,
        VAlign::Centre,
    );
    text(
        ctx,
        Rect::new(rect.x + 40.0, rect.y, rect.w - 48.0, rect.h),
        label,
        Font::Nav,
        if active { Brush::Text } else { Brush::TextDim },
        HAlign::Leading,
        VAlign::Centre,
    );
    clicked
}
