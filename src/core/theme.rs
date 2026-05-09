// Colour palettes for the two themes.
//
// Direct2D wants colours as `D2D1_COLOR_F` (linear RGBA, 0..1). We
// store them as plain `[f32; 4]` so this module has no Direct2D
// dependency — the renderer converts at the brush-create site. This
// keeps the colour table easy to tweak without dragging windows-rs
// types into business logic.
//
// Palette intent:
//   bg               canvas background
//   surface          card / panel background
//   surface_alt      hovered card
//   border           hairline between sections
//   text             primary text
//   text_dim         secondary / hint text
//   accent           current view, primary CTA
//   accent_strong    KPM live number, key counts above zero
//   pulse            momentary live-pulse highlight
//   heat_min/max     heatmap gradient endpoints
//   modifier         shift/ctrl/alt/meta bar colour
//   warn             reset, dangerous actions

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Dark,
    Light,
}

impl Theme {
    pub fn id(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "light" => Self::Light,
            _ => Self::Dark,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub bg: [f32; 4],
    pub surface: [f32; 4],
    pub surface_alt: [f32; 4],
    pub border: [f32; 4],
    pub text: [f32; 4],
    pub text_dim: [f32; 4],
    pub accent: [f32; 4],
    pub accent_strong: [f32; 4],
    pub pulse: [f32; 4],
    pub heat_min: [f32; 4],
    pub heat_max: [f32; 4],
    pub modifier: [f32; 4],
    pub warn: [f32; 4],
    pub success: [f32; 4],
}

const fn rgba(r: u8, g: u8, b: u8, a: u8) -> [f32; 4] {
    [
        r as f32 / 255.0,
        g as f32 / 255.0,
        b as f32 / 255.0,
        a as f32 / 255.0,
    ]
}

pub const DARK: Palette = Palette {
    bg:            rgba(0x0c, 0x0d, 0x12, 0xff),
    surface:       rgba(0x16, 0x18, 0x21, 0xff),
    surface_alt:   rgba(0x1d, 0x20, 0x2c, 0xff),
    border:        rgba(0x2a, 0x2e, 0x3c, 0xff),
    text:          rgba(0xe8, 0xea, 0xf2, 0xff),
    text_dim:      rgba(0x8b, 0x90, 0xa6, 0xff),
    accent:        rgba(0x6e, 0x82, 0xff, 0xff),
    accent_strong: rgba(0x9c, 0xa9, 0xff, 0xff),
    pulse:         rgba(0xff, 0xc4, 0x6b, 0xff),
    heat_min:      rgba(0x1d, 0x20, 0x2c, 0xff),
    heat_max:      rgba(0xff, 0x6b, 0xa6, 0xff),
    modifier:      rgba(0x4f, 0xc1, 0xa6, 0xff),
    warn:          rgba(0xff, 0x6b, 0x6b, 0xff),
    success:       rgba(0x4c, 0xd4, 0x8a, 0xff),
};

pub const LIGHT: Palette = Palette {
    bg:            rgba(0xf6, 0xf7, 0xfb, 0xff),
    surface:       rgba(0xff, 0xff, 0xff, 0xff),
    surface_alt:   rgba(0xee, 0xf0, 0xf6, 0xff),
    border:        rgba(0xd9, 0xdd, 0xea, 0xff),
    text:          rgba(0x16, 0x18, 0x21, 0xff),
    text_dim:      rgba(0x66, 0x6c, 0x82, 0xff),
    accent:        rgba(0x4a, 0x5e, 0xea, 0xff),
    accent_strong: rgba(0x2d, 0x40, 0xc8, 0xff),
    pulse:         rgba(0xe8, 0x88, 0x18, 0xff),
    heat_min:      rgba(0xee, 0xf0, 0xf6, 0xff),
    heat_max:      rgba(0xea, 0x4c, 0x88, 0xff),
    modifier:      rgba(0x29, 0x9d, 0x84, 0xff),
    warn:          rgba(0xd0, 0x40, 0x40, 0xff),
    success:       rgba(0x29, 0xa6, 0x66, 0xff),
};

pub fn palette(theme: Theme) -> Palette {
    match theme {
        Theme::Dark => DARK,
        Theme::Light => LIGHT,
    }
}
