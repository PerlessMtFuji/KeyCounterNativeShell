// Views — one module per "screen" the sidebar navigates between.
//
// Each view exports a `draw` function that takes the render context,
// the rect it owns (sidebar + topbar already subtracted by the caller),
// the AppState (which holds the snapshot), and the input state for
// click handling. Views are stateless — anything user-mutable lives
// in AppState (settings, view choice) so a view can be rebuilt at any
// frame without losing context.

pub mod achievements;
pub mod dashboard;
pub mod heatmap_view;
pub mod settings;
pub mod stats;

use crate::app::AppState;
use crate::core::theme::Theme;
use crate::ui::controls::InputState;
use crate::ui::render::primitives::Rect;
use crate::ui::render::RenderContext;

/// View identity. The numeric values are stable — the active view is
/// persisted in settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Dashboard,
    Heatmap,
    Stats,
    Achievements,
    Settings,
}

/// What a view paint asks the main window to do afterwards. Most views
/// return the default (no-op) — only the Settings view emits intent
/// today, but the channel exists so future views (achievements toast
/// dismissal, dashboard quick-actions) can join without refactoring.
#[derive(Default)]
pub struct ViewOutput {
    pub picked_theme: Option<Theme>,
    pub settings_dirty: bool,
    pub reset_requested: bool,
    pub export_requested: bool,
    /// Set when the autostart toggle just flipped. Main window calls
    /// `system::autostart::apply` so the registry write happens
    /// immediately, not just at exit.
    pub autostart_changed: bool,
    /// Set when the user toggled the floating-widget visibility from
    /// inside the settings panel. Main window calls ShowWindow on the
    /// widget HWND.
    pub widget_visibility_changed: Option<bool>,
}

pub fn draw(
    ctx: &RenderContext,
    rect: Rect,
    view: View,
    state: &AppState,
    input: &mut InputState,
) -> ViewOutput {
    match view {
        View::Dashboard => {
            dashboard::draw(ctx, rect, state, input);
            ViewOutput::default()
        }
        View::Heatmap => {
            heatmap_view::draw(ctx, rect, state, input);
            ViewOutput::default()
        }
        View::Stats => {
            stats::draw(ctx, rect, state, input);
            ViewOutput::default()
        }
        View::Achievements => {
            achievements::draw(ctx, rect, state, input);
            ViewOutput::default()
        }
        View::Settings => {
            let s = settings::draw(ctx, rect, state, input);
            ViewOutput {
                picked_theme: s.picked_theme,
                settings_dirty: s.settings_dirty,
                reset_requested: s.reset_requested,
                export_requested: s.export_requested,
                autostart_changed: s.autostart_changed,
                widget_visibility_changed: s.widget_visibility_changed,
            }
        }
    }
}
