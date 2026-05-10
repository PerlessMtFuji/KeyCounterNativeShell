// Drawing primitives.
//
// Thin wrappers over the windows-rs Direct2D API. The point isn't
// abstraction for its own sake — D2D's ergonomics are awkward when
// called inline (D2D1_RECT_F constructors, raw pointer slices for
// strings) and these helpers keep the call sites readable.
//
// Everything takes RenderContext + already-resolved DIPs. No DPI
// scaling happens in here; that's the caller's job (it has the
// dpi_scale on the context already).

use windows::core::PCWSTR;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct2D::Common::{D2D1_COLOR_F, D2D_POINT_2F, D2D_RECT_F};
use windows::Win32::Graphics::Direct2D::{
    ID2D1SolidColorBrush, D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_MEASURING_MODE_NATURAL, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_FAR, DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
    DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING, DWRITE_TEXT_METRICS,
};

use super::{Brush, Font, RenderContext};

/// Plain f32 rect — easier to pass around than D2D_RECT_F. All UI
/// layout works in this; conversion to D2D types happens at the draw
/// site.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_xywh(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    pub fn from_win32(r: RECT) -> Self {
        Self {
            x: r.left as f32,
            y: r.top as f32,
            w: (r.right - r.left).max(0) as f32,
            h: (r.bottom - r.top).max(0) as f32,
        }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }
    pub fn centre(&self) -> (f32, f32) {
        (self.x + self.w * 0.5, self.y + self.h * 0.5)
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && py >= self.y && px < self.right() && py < self.bottom()
    }

    pub fn shrink(&self, dx: f32, dy: f32) -> Self {
        Self {
            x: self.x + dx,
            y: self.y + dy,
            w: (self.w - 2.0 * dx).max(0.0),
            h: (self.h - 2.0 * dy).max(0.0),
        }
    }

    pub fn inset(&self, top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            x: self.x + left,
            y: self.y + top,
            w: (self.w - left - right).max(0.0),
            h: (self.h - top - bottom).max(0.0),
        }
    }

    /// Take the leftmost `width` DIPs and return (taken, remainder).
    pub fn split_left(&self, width: f32) -> (Rect, Rect) {
        let w = width.min(self.w);
        (
            Rect::new(self.x, self.y, w, self.h),
            Rect::new(self.x + w, self.y, self.w - w, self.h),
        )
    }

    /// Take the topmost `height` DIPs and return (taken, remainder).
    pub fn split_top(&self, height: f32) -> (Rect, Rect) {
        let h = height.min(self.h);
        (
            Rect::new(self.x, self.y, self.w, h),
            Rect::new(self.x, self.y + h, self.w, self.h - h),
        )
    }

    pub fn to_d2d(self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.x + self.w,
            bottom: self.y + self.h,
        }
    }
}

#[derive(Copy, Clone)]
pub enum HAlign {
    Leading,
    Centre,
    Trailing,
}

#[derive(Copy, Clone)]
pub enum VAlign {
    Top,
    Centre,
    Bottom,
}

/// Fill the whole render target with one colour. Cheaper than the
/// generic fill_rect for the canvas clear — D2D fast-paths Clear.
pub fn clear(ctx: &RenderContext, b: Brush) {
    let p = ctx.palette();
    let rgba = palette_lookup(p, b);
    unsafe {
        ctx.target.Clear(Some(&D2D1_COLOR_F {
            r: rgba[0],
            g: rgba[1],
            b: rgba[2],
            a: rgba[3],
        }));
    }
}

pub fn fill_rect(ctx: &RenderContext, rect: Rect, b: Brush) {
    let r = rect.to_d2d();
    unsafe {
        ctx.target.FillRectangle(&r, ctx.brush(b));
    }
}

pub fn stroke_rect(ctx: &RenderContext, rect: Rect, b: Brush, width: f32) {
    let r = rect.to_d2d();
    unsafe {
        ctx.target.DrawRectangle(&r, ctx.brush(b), width, None);
    }
}

pub fn fill_rounded(ctx: &RenderContext, rect: Rect, radius: f32, b: Brush) {
    let rr = D2D1_ROUNDED_RECT {
        rect: rect.to_d2d(),
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        ctx.target.FillRoundedRectangle(&rr, ctx.brush(b));
    }
}

pub fn stroke_rounded(ctx: &RenderContext, rect: Rect, radius: f32, b: Brush, width: f32) {
    let rr = D2D1_ROUNDED_RECT {
        rect: rect.to_d2d(),
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        ctx.target.DrawRoundedRectangle(&rr, ctx.brush(b), width, None);
    }
}

/// Card surface: filled rounded rect + 1 px hairline border. Matches
/// the visual pattern used in the React version's `<Card>` component.
pub fn card(ctx: &RenderContext, rect: Rect, radius: f32) {
    fill_rounded(ctx, rect, radius, Brush::Surface);
    stroke_rounded(ctx, rect, radius, Brush::Border, 1.0);
}

pub fn fill_circle(ctx: &RenderContext, cx: f32, cy: f32, r: f32, b: Brush) {
    let ellipse = windows::Win32::Graphics::Direct2D::Common::D2D1_ELLIPSE {
        point: D2D_POINT_2F { x: cx, y: cy },
        radiusX: r,
        radiusY: r,
    };
    unsafe {
        ctx.target.FillEllipse(&ellipse, ctx.brush(b));
    }
}

pub fn stroke_circle(ctx: &RenderContext, cx: f32, cy: f32, r: f32, b: Brush, width: f32) {
    let ellipse = windows::Win32::Graphics::Direct2D::Common::D2D1_ELLIPSE {
        point: D2D_POINT_2F { x: cx, y: cy },
        radiusX: r,
        radiusY: r,
    };
    unsafe {
        ctx.target.DrawEllipse(&ellipse, ctx.brush(b), width, None);
    }
}

pub fn line(ctx: &RenderContext, x1: f32, y1: f32, x2: f32, y2: f32, b: Brush, width: f32) {
    unsafe {
        ctx.target.DrawLine(
            D2D_POINT_2F { x: x1, y: y1 },
            D2D_POINT_2F { x: x2, y: y2 },
            ctx.brush(b),
            width,
            None,
        );
    }
}

/// Draw a single text run with the given font slot, brush and
/// alignment. Unicode is encoded UTF-16 into a stack scratch buffer
/// for short strings (everything in our UI fits comfortably).
pub fn text(
    ctx: &RenderContext,
    rect: Rect,
    s: &str,
    font: Font,
    brush: Brush,
    h: HAlign,
    v: VAlign,
) {
    let format = ctx.font(font);
    unsafe {
        let _ = format.SetTextAlignment(match h {
            HAlign::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
            HAlign::Centre => DWRITE_TEXT_ALIGNMENT_CENTER,
            HAlign::Trailing => DWRITE_TEXT_ALIGNMENT_TRAILING,
        });
        let _ = format.SetParagraphAlignment(match v {
            VAlign::Top => DWRITE_PARAGRAPH_ALIGNMENT_NEAR,
            VAlign::Centre => DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
            VAlign::Bottom => DWRITE_PARAGRAPH_ALIGNMENT_FAR,
        });
    }

    let buf: Vec<u16> = s.encode_utf16().collect();
    if buf.is_empty() {
        return;
    }
    unsafe {
        ctx.target.DrawText(
            &buf,
            format,
            &rect.to_d2d(),
            ctx.brush(brush),
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

/// Measure a single-line text run — caller usually wants width to
/// place neighbouring elements (e.g. an icon next to a label). Returns
/// (width, height) in DIPs.
pub fn measure(ctx: &RenderContext, s: &str, font: Font, max_width: f32) -> (f32, f32) {
    let format = ctx.font(font);
    let buf: Vec<u16> = s.encode_utf16().collect();
    if buf.is_empty() {
        return (0.0, 0.0);
    }
    let layout = unsafe {
        ctx.dwrite
            .CreateTextLayout(&buf, format, max_width.max(1.0), 4096.0)
    };
    let Ok(layout) = layout else {
        return (0.0, 0.0);
    };
    let mut metrics = DWRITE_TEXT_METRICS::default();
    let _ = unsafe { layout.GetMetrics(&mut metrics) };
    (metrics.widthIncludingTrailingWhitespace, metrics.height)
}

/// Filled "pill" — rounded rect where the radius equals half the
/// height. Used for badges, achievement chips, the live KPM container.
pub fn pill(ctx: &RenderContext, rect: Rect, b: Brush) {
    let r = rect.h * 0.5;
    fill_rounded(ctx, rect, r, b);
}

/// Same but with a stroke (used for outlined chips).
pub fn pill_outlined(ctx: &RenderContext, rect: Rect, b: Brush, stroke: f32) {
    let r = rect.h * 0.5;
    stroke_rounded(ctx, rect, r, b, stroke);
}

/// Centred text inside a pill. Wrap convenience: the pill +
/// label sequence shows up everywhere (sidebar nav, badges, view
/// titles).
pub fn pill_with_label(
    ctx: &RenderContext,
    rect: Rect,
    bg: Brush,
    fg: Brush,
    font: Font,
    label: &str,
) {
    pill(ctx, rect, bg);
    text(ctx, rect, label, font, fg, HAlign::Centre, VAlign::Centre);
}

/// Format a large integer with thin spaces — same style the Tauri UI
/// used (1 234 567 not 1,234,567 — culturally neutral, looks clean
/// on both PL and EN).
pub fn fmt_int(n: i64) -> String {
    if n.abs() < 1000 {
        return n.to_string();
    }
    let neg = n < 0;
    let mut digits = n.unsigned_abs().to_string().into_bytes();
    let mut out = Vec::with_capacity(digits.len() + digits.len() / 3 + 1);
    if neg {
        out.push(b'-');
    }
    let len = digits.len();
    for (i, ch) in digits.drain(..).enumerate() {
        let from_end = len - i;
        out.push(ch);
        if from_end > 1 && from_end % 3 == 1 {
            // U+202F NARROW NO-BREAK SPACE — but ASCII space is fine
            // for our font and saves the UTF-8 bytes.
            out.push(b' ');
        }
    }
    String::from_utf8(out).unwrap()
}

/// PCWSTR helper — encode a string into a vec<u16> with terminator.
/// Caller keeps the vec alive for the duration of the FFI call.
pub fn wstr(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

#[allow(dead_code)]
pub fn pcwstr(buf: &[u16]) -> PCWSTR {
    PCWSTR(buf.as_ptr())
}

/// Lookup a brush colour by enum without going through the cache —
/// used by `clear()` since Clear takes a colour, not a brush. Mirrors
/// the order in build_brushes().
fn palette_lookup(p: &crate::core::theme::Palette, b: Brush) -> [f32; 4] {
    match b {
        Brush::Bg => p.bg,
        Brush::Surface => p.surface,
        Brush::SurfaceAlt => p.surface_alt,
        Brush::Border => p.border,
        Brush::Text => p.text,
        Brush::TextDim => p.text_dim,
        Brush::Accent => p.accent,
        Brush::AccentStrong => p.accent_strong,
        Brush::Pulse => p.pulse,
        Brush::HeatMin => p.heat_min,
        Brush::HeatMax => p.heat_max,
        Brush::Modifier => p.modifier,
        Brush::Warn => p.warn,
        Brush::Success => p.success,
        Brush::TextMuted => super::with_alpha(p.text_dim, 0.6),
        Brush::SurfaceHover => super::blend(p.surface, p.surface_alt, 0.8),
        Brush::Shadow => super::with_alpha([0.0, 0.0, 0.0, 1.0], 0.35),
    }
}

#[allow(unused_imports)]
pub(crate) use windows::Win32::Graphics::Direct2D::ID2D1Brush;

/// Brush by reference — exposed for callers that want to override the
/// stroke colour without copying through the slot enum.
#[allow(dead_code)]
pub fn brush_ref(ctx: &RenderContext, b: Brush) -> &ID2D1SolidColorBrush {
    ctx.brush(b)
}
