// Render layer — owns the Direct2D / DirectWrite resources.
//
// The factories live for the process lifetime. The HWND render target
// and every brush / text format derived from it are *device-bound*: if
// the GPU resets (driver crash, sleep/resume, monitor change) D2D
// returns D2DERR_RECREATE_TARGET on EndDraw and we drop the whole bag
// and rebuild on the next paint. That rebuild is < 1 ms on modern
// hardware so we don't try to be clever about it.
//
// Brush + text-format caches are keyed by the active Theme. When the
// user toggles dark/light we rebuild the brush bag once; per-frame we
// just look up by index — no allocations on the paint path.

pub mod primitives;
pub mod charts;
pub mod heatmap;
pub mod calendar;
pub mod punch_card;

use anyhow::Result;
use windows::core::{w, Interface};
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory1, ID2D1HwndRenderTarget, ID2D1RenderTarget,
    ID2D1SolidColorBrush, D2D1_FACTORY_TYPE_SINGLE_THREADED, D2D1_FEATURE_LEVEL_DEFAULT,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT,
    D2D1_RENDER_TARGET_USAGE_NONE,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat, DWRITE_FACTORY_TYPE_SHARED,
    DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL, DWRITE_FONT_WEIGHT_NORMAL,
    DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_PARAGRAPH_ALIGNMENT_CENTER,
    DWRITE_PARAGRAPH_ALIGNMENT_NEAR, DWRITE_TEXT_ALIGNMENT_CENTER,
    DWRITE_TEXT_ALIGNMENT_LEADING, DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;

use crate::core::theme::{Palette, Theme, DARK, LIGHT};

/// Indexed brush slots — cheaper than a HashMap lookup during paint.
/// Order matters; the indices feed straight into the brushes array.
#[repr(usize)]
#[derive(Copy, Clone)]
pub enum Brush {
    Bg = 0,
    Surface,
    SurfaceAlt,
    Border,
    Text,
    TextDim,
    Accent,
    AccentStrong,
    Pulse,
    HeatMin,
    HeatMax,
    Modifier,
    Warn,
    Success,
    /// Translucent text for footnotes — derived from text_dim at 60 %.
    TextMuted,
    /// Hovered surface — surface_alt at 80 %.
    SurfaceHover,
    /// Translucent overlay for shadows / scrims.
    Shadow,
}

const BRUSH_COUNT: usize = 17;

#[repr(usize)]
#[derive(Copy, Clone)]
pub enum Font {
    /// Body — 14 px Segoe UI, dim.
    Body = 0,
    /// Body emphasised — 14 px Semibold.
    BodyStrong,
    /// Caption — 11 px, muted.
    Caption,
    /// Card label above big numbers — 12 px uppercase tracked.
    CardLabel,
    /// Big number on cards — 32 px Semibold.
    Display,
    /// Sidebar nav — 14 px.
    Nav,
    /// Section heading — 18 px Semibold.
    Heading,
    /// Live KPM digits in top bar — 22 px Semibold.
    LiveKpm,
    /// Tiny labels on heatmap keys — 9 px.
    Glyph,
}

const FONT_COUNT: usize = 9;

/// Per-window render resources. Owned by the main_window struct, lives
/// from first paint until the window is destroyed (or until D2D asks
/// for a rebuild).
///
/// Two render-target handles point at the same underlying D2D object:
/// `hwnd_target` is the concrete `ID2D1HwndRenderTarget` (needed for
/// `Resize` / window-state checks), and `target` is the upcast
/// `ID2D1RenderTarget` view used by every drawing primitive. We keep
/// both because windows-rs 0.58 doesn't expose parent-interface
/// methods via `Deref` for generic-typed methods like
/// `CreateSolidColorBrush`, so callers that need those go through the
/// upcast.
pub struct RenderContext {
    pub d2d: ID2D1Factory1,
    pub dwrite: IDWriteFactory,
    pub hwnd_target: ID2D1HwndRenderTarget,
    pub target: ID2D1RenderTarget,
    pub brushes: [Option<ID2D1SolidColorBrush>; BRUSH_COUNT],
    pub fonts: [Option<IDWriteTextFormat>; FONT_COUNT],
    pub theme: Theme,
    pub dpi_scale: f32,
    pub size_px: (u32, u32),
}

impl RenderContext {
    pub fn create(hwnd: HWND, size_px: (u32, u32), dpi: u32, theme: Theme) -> Result<Self> {
        let d2d: ID2D1Factory1 =
            unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
        let dwrite: IDWriteFactory =
            unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };

        let dpi_f = dpi as f32;
        let render_props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            // Render target uses physical pixels; we pass DIPs ourselves.
            // Setting dpi=96 makes 1 D2D unit == 1 device pixel and our
            // own scale factor controls layout. Simpler than the
            // double-scale that D2D's own DPI handling does.
            dpiX: 96.0,
            dpiY: 96.0,
            usage: D2D1_RENDER_TARGET_USAGE_NONE,
            minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
        };
        let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: size_px.0.max(1),
                height: size_px.1.max(1),
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        let hwnd_target =
            unsafe { d2d.CreateHwndRenderTarget(&render_props, &hwnd_props)? };
        // QueryInterface upcast — same underlying D2D object, different
        // view of the methods. In practice a refcount bump.
        let target: ID2D1RenderTarget = hwnd_target.cast()?;

        let mut ctx = Self {
            d2d,
            dwrite,
            hwnd_target,
            target,
            brushes: std::array::from_fn(|_| None),
            fonts: std::array::from_fn(|_| None),
            theme,
            dpi_scale: dpi_f / 96.0,
            size_px,
        };
        ctx.build_brushes()?;
        ctx.build_fonts()?;
        Ok(ctx)
    }

    pub fn resize(&mut self, w: u32, h: u32) -> Result<()> {
        self.size_px = (w.max(1), h.max(1));
        unsafe {
            self.hwnd_target.Resize(&D2D_SIZE_U {
                width: self.size_px.0,
                height: self.size_px.1,
            })?;
        }
        Ok(())
    }

    pub fn set_theme(&mut self, theme: Theme) -> Result<()> {
        if self.theme == theme {
            return Ok(());
        }
        self.theme = theme;
        self.brushes.iter_mut().for_each(|s| *s = None);
        self.build_brushes()?;
        Ok(())
    }

    pub fn set_dpi(&mut self, dpi: u32) -> Result<()> {
        self.dpi_scale = dpi as f32 / 96.0;
        // Fonts encode size in DIPs scaled by dpi_scale, so rebuild.
        self.fonts.iter_mut().for_each(|s| *s = None);
        self.build_fonts()
    }

    pub fn palette(&self) -> &'static Palette {
        match self.theme {
            Theme::Dark => &DARK,
            Theme::Light => &LIGHT,
        }
    }

    pub fn brush(&self, b: Brush) -> &ID2D1SolidColorBrush {
        // SAFETY: build_brushes populates every slot. The only path to
        // an empty slot is between drop and rebuild, which never spans
        // a draw call — the rebuild is synchronous.
        self.brushes[b as usize]
            .as_ref()
            .expect("brush slot uninitialised")
    }

    pub fn font(&self, f: Font) -> &IDWriteTextFormat {
        self.fonts[f as usize]
            .as_ref()
            .expect("font slot uninitialised")
    }

    /// Convert DIPs to device pixels for the current DPI.
    pub fn px(&self, dip: f32) -> f32 {
        dip * self.dpi_scale
    }

    fn build_brushes(&mut self) -> Result<()> {
        let p = self.palette();
        let muted = with_alpha(p.text_dim, 0.6);
        let hover = blend(p.surface, p.surface_alt, 0.8);
        let shadow = with_alpha([0.0, 0.0, 0.0, 1.0], 0.35);
        let pairs = [
            (Brush::Bg, p.bg),
            (Brush::Surface, p.surface),
            (Brush::SurfaceAlt, p.surface_alt),
            (Brush::Border, p.border),
            (Brush::Text, p.text),
            (Brush::TextDim, p.text_dim),
            (Brush::Accent, p.accent),
            (Brush::AccentStrong, p.accent_strong),
            (Brush::Pulse, p.pulse),
            (Brush::HeatMin, p.heat_min),
            (Brush::HeatMax, p.heat_max),
            (Brush::Modifier, p.modifier),
            (Brush::Warn, p.warn),
            (Brush::Success, p.success),
            (Brush::TextMuted, muted),
            (Brush::SurfaceHover, hover),
            (Brush::Shadow, shadow),
        ];
        for (slot, rgba) in pairs {
            let color = D2D1_COLOR_F {
                r: rgba[0],
                g: rgba[1],
                b: rgba[2],
                a: rgba[3],
            };
            let brush = create_solid_brush(&self.target, color)?;
            self.brushes[slot as usize] = Some(brush);
        }
        Ok(())
    }

    fn build_fonts(&mut self) -> Result<()> {
        let s = self.dpi_scale;
        let specs: [(Font, f32, _, _); FONT_COUNT] = [
            (Font::Body, 14.0 * s, DWRITE_FONT_WEIGHT_NORMAL, false),
            (Font::BodyStrong, 14.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, false),
            (Font::Caption, 11.0 * s, DWRITE_FONT_WEIGHT_NORMAL, false),
            (Font::CardLabel, 12.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, true),
            (Font::Display, 32.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, false),
            (Font::Nav, 14.0 * s, DWRITE_FONT_WEIGHT_NORMAL, false),
            (Font::Heading, 18.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, false),
            (Font::LiveKpm, 22.0 * s, DWRITE_FONT_WEIGHT_SEMI_BOLD, false),
            (Font::Glyph, 9.0 * s, DWRITE_FONT_WEIGHT_NORMAL, false),
        ];
        for (slot, size, weight, _uppercase) in specs {
            let format = unsafe {
                self.dwrite.CreateTextFormat(
                    w!("Segoe UI"),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("en-us"),
                )?
            };
            // Default alignment = leading + near. Per-call helpers in
            // primitives.rs override these as needed.
            unsafe {
                let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING);
                let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_NEAR);
            }
            self.fonts[slot as usize] = Some(format);
        }
        Ok(())
    }
}

/// Convenience: build a centred text format on the fly. Used for
/// one-off labels (badges, key glyphs) where we don't want to pollute
/// the cached slot enum. Cheap — DWrite reuses the underlying font
/// face internally.
pub fn make_text_format(
    dwrite: &IDWriteFactory,
    size: f32,
    weight: u32,
    align_centre: bool,
    centre_paragraph: bool,
) -> Result<IDWriteTextFormat> {
    let format = unsafe {
        dwrite.CreateTextFormat(
            w!("Segoe UI"),
            None,
            windows::Win32::Graphics::DirectWrite::DWRITE_FONT_WEIGHT(weight as i32),
            DWRITE_FONT_STYLE_NORMAL,
            DWRITE_FONT_STRETCH_NORMAL,
            size,
            w!("en-us"),
        )?
    };
    unsafe {
        if align_centre {
            let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_CENTER);
        } else {
            let _ = format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_TRAILING);
        }
        if centre_paragraph {
            let _ = format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER);
        }
    }
    Ok(format)
}

/// Replace the alpha channel of an RGBA colour. Used to derive the
/// muted / shadow brushes from the base palette.
pub fn with_alpha(rgba: [f32; 4], a: f32) -> [f32; 4] {
    [rgba[0], rgba[1], rgba[2], a.clamp(0.0, 1.0)]
}

/// Linear blend between two RGBA colours (t = 0 → a, t = 1 → b).
pub fn blend(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let t = t.clamp(0.0, 1.0);
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
        a[3] + (b[3] - a[3]) * t,
    ]
}

/// Heat ramp t∈[0,1] → palette colour. Lives here so heatmap, calendar
/// and punch-card share the same gradient without duplicating the math.
pub fn heat(p: &Palette, t: f32) -> [f32; 4] {
    blend(p.heat_min, p.heat_max, t)
}

/// RECT → D2D rect (f32 corners, exclusive bottom-right). Win32 RECT
/// already uses exclusive coords for client areas so this is a direct
/// numeric cast.
pub fn rect_to_d2d(r: &RECT) -> windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
    windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F {
        left: r.left as f32,
        top: r.top as f32,
        right: r.right as f32,
        bottom: r.bottom as f32,
    }
}

/// `CreateSolidColorBrush` wrapper. Trivial bind to the
/// windows-rs-generated method, but keeps the `unsafe` block and the
/// `&color` reference out of every call site — most of which sit in
/// hot inner loops in the heatmap / calendar / punch-card renderers.
///
/// Note: this method is feature-gated by `Foundation_Numerics` in
/// windows-rs 0.58 because `D2D1_BRUSH_PROPERTIES` contains a
/// `D2D1_MATRIX_3X2_F`. Cargo.toml has that feature enabled — without
/// it the method silently disappears from the bindings (which is what
/// the earlier "method not found" errors were really telling us).
pub fn create_solid_brush(
    target: &ID2D1RenderTarget,
    color: D2D1_COLOR_F,
) -> Result<ID2D1SolidColorBrush> {
    let brush = unsafe { target.CreateSolidColorBrush(&color, None)? };
    Ok(brush)
}
