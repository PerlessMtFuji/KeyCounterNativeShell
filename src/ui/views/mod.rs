// Views — one module per "screen" the sidebar navigates between.
//
// Each view exports a `draw` function that takes the render context,
// the rect it owns (sidebar + topbar already subtracted by the caller),
// the AppState (which holds the snapshot), and the input state for
// click handling. Views are stateless — anything user-mutable lives
// in AppState (settings, view choice) so a view can be rebuilt at any
// frame without losing context.

pub mod dashboard;

use crate::app::AppState;
use crate::ui::controls::InputState;
use crate::ui::render::primitives::{text, HAlign, Rect, VAlign};
use crate::ui::render::{Brush, Font, RenderContext};

/// View identity. The numeric values are stable — the active view is
/// persisted in settings (Push 4 will hook this up; for now it
/// defaults to Dashboard).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Heatmap,
    Stats,
    Achievements,
    Settings,
}

pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    view: View,
    state: &AppState,
    input: &mut InputState,
) {
    match view {
        View::Dashboard => dashboard::draw(ctx, rect, state, input),
        // Stub views — full implementations land in Push 3.
        other => stub(ctx, rect, label_for(other)),
    }
}

fn stub(ctx: &RenderContext, rect: Rect, label: &str) {
    text(
        ctx,
        rect.shrink(40.0, 40.0),
        label,
        Font::Heading,
        Brush::TextDim,
        HAlign::Centre,
        VAlign::Centre,
    );
    text(
        ctx,
        Rect::new(rect.x, rect.bottom() - 60.0, rect.w, 24.0),
        "Coming in the next release.",
        Font::Caption,
        Brush::TextMuted,
        HAlign::Centre,
        VAlign::Centre,
    );
}

fn label_for(v: View) -> &'static str {
    match v {
        View::Dashboard => "Dashboard",
        View::Heatmap => "Keyboard heatmap",
        View::Stats => "Statistics",
        View::Achievements => "Achievements",
        View::Settings => "Settings",
    }
}
