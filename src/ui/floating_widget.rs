// Floating widget — small always-on-top window with the live counters.
//
// Two visual modes (`widget_compact` in settings):
//   * Full     — 200×96 card: KPM big, today total small underneath.
//   * Compact  — 140×44 pill: KPM as the only number, semi-transparent.
//
// Implementation notes:
//   * `WS_POPUP | WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`.
//     LAYERED + SetLayeredWindowAttributes(LWA_ALPHA) is enough for
//     whole-window translucency — we don't need UpdateLayeredWindow
//     because the body is opaque, only the alpha varies.
//   * Rounded silhouette: `SetWindowRgn` with `CreateRoundRectRgn`
//     clips the *window itself* to a rounded rectangle. Without this
//     the layered window paints its full square bg behind the card
//     and you see corners poking out where the rounded fill ends.
//     The region is re-set every time the mode/DPI changes.
//   * Dragging via WM_NCHITTEST → HTCAPTION trick: any click in the
//     body acts like a titlebar grab.
//   * The widget owns its own RenderContext (different HWND than the
//     main window, but the same Direct2D + DirectWrite factories — we
//     just don't bother sharing them; each context creates its own
//     factories on demand, and the factory cost is one-time).
//   * Snap-to-taskbar: when settings.widget_snap is on, every show
//     re-positions the widget to the bottom-right corner of the work
//     area minus a 16 DIP margin.

use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use chrono::Utc;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateRoundRectRgn, EndPaint, InvalidateRect, SetWindowRgn, UpdateWindow, HBRUSH,
    PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, GetClientRect, GetWindowLongPtrW, KillTimer, LoadCursorW,
    RegisterClassExW, SetLayeredWindowAttributes, SetTimer, SetWindowLongPtrW, SetWindowPos,
    ShowWindow, SystemParametersInfoW, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA,
    HCURSOR, HICON, HMENU, HTCAPTION, IDC_HAND, LWA_ALPHA, SPI_GETWORKAREA, SWP_NOACTIVATE,
    SWP_NOSIZE, SWP_NOZORDER, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WM_CREATE, WM_DESTROY,
    WM_ERASEBKGND, WM_LBUTTONDOWN, WM_NCHITTEST, WM_PAINT, WM_SIZE, WM_TIMER, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use crate::app::AppState;
use crate::core::i18n;
use crate::ui::render::primitives::{
    card, fill_rounded, fmt_int, pill, text, HAlign, Rect, VAlign,
};
use crate::ui::render::{Brush, Font, RenderContext};

const CLASS_NAME: PCWSTR = w!("KeyCounter.Widget");
const TITLE: PCWSTR = w!("KeyCounter widget");

const T_REFRESH: usize = 11;

const FULL_DIPS: (i32, i32) = (200, 96);
const COMPACT_DIPS: (i32, i32) = (140, 44);

struct Widget {
    hwnd: HWND,
    state: AppState,
    ctx: Option<RenderContext>,
    last_compact: bool,
    last_opacity: u8,
    prev_counter: i64,
    last_event_ms: i64,
}

impl Widget {
    fn new(hwnd: HWND, state: AppState) -> Box<Self> {
        Box::new(Self {
            hwnd,
            state,
            ctx: None,
            last_compact: false,
            last_opacity: 0,
            prev_counter: 0,
            last_event_ms: 0,
        })
    }

    fn ensure_ctx(&mut self) -> Result<()> {
        if self.ctx.is_some() {
            return Ok(());
        }
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96);
        let theme = i18n::SETTINGS.read().theme;
        let ctx = RenderContext::create(self.hwnd, (w, h), dpi, theme)?;
        self.ctx = Some(ctx);
        Ok(())
    }

    fn now_ms(&self) -> i64 {
        Utc::now().timestamp_millis()
    }

    fn refresh(&mut self) {
        // Mode / opacity changes resize + re-alpha the window.
        let (compact, opacity, snap) = {
            let s = i18n::SETTINGS.read();
            (s.widget_compact, s.widget_opacity, s.widget_snap)
        };
        if compact != self.last_compact {
            self.resize_to_mode(compact);
            self.last_compact = compact;
        }
        if opacity != self.last_opacity {
            unsafe {
                let _ = SetLayeredWindowAttributes(
                    self.hwnd,
                    windows::Win32::Foundation::COLORREF(0),
                    (255 * opacity as u32 / 100).clamp(40, 255) as u8,
                    LWA_ALPHA,
                );
            }
            self.last_opacity = opacity;
        }
        if snap {
            self.snap_to_taskbar();
        }

        // Pulse counter delta — drives the colour flash on the KPM number.
        let counter = self.state.live_counter.load(Ordering::Relaxed);
        let delta = counter - self.prev_counter;
        self.prev_counter = counter;
        if delta > 0 {
            self.last_event_ms = self.now_ms();
        }

        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn resize_to_mode(&self, compact: bool) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) }.max(96) as f32;
        let scale = dpi / 96.0;
        let (w, h) = if compact { COMPACT_DIPS } else { FULL_DIPS };
        let w_px = (w as f32 * scale) as i32;
        let h_px = (h as f32 * scale) as i32;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                0,
                0,
                w_px,
                h_px,
                SWP_NOZORDER | SWP_NOACTIVATE | windows::Win32::UI::WindowsAndMessaging::SWP_NOMOVE,
            );
            apply_rounded_region(self.hwnd, w_px, h_px, compact);
        }
    }

    fn snap_to_taskbar(&self) {
        let mut work = RECT::default();
        unsafe {
            let _ = SystemParametersInfoW(
                SPI_GETWORKAREA,
                0,
                Some(&mut work as *mut RECT as *mut _),
                SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
            );
        }
        let mut me = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut me);
        }
        let margin = 16;
        let w = me.right - me.left;
        let h = me.bottom - me.top;
        let x = work.right - w - margin;
        let y = work.bottom - h - margin;
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    }

    fn paint(&mut self) {
        if self.ensure_ctx().is_err() {
            return;
        }

        let mut ps = PAINTSTRUCT::default();
        let _hdc = unsafe { BeginPaint(self.hwnd, &mut ps) };

        unsafe {
            self.ctx.as_ref().unwrap().target.BeginDraw();
        }
        self.draw_frame();
        let need_recreate = {
            let ctx = self.ctx.as_ref().unwrap();
            let result = unsafe { ctx.target.EndDraw(None, None) };
            matches!(result, Err(e) if e.code() == crate::ui::main_window::D2DERR_RECREATE_TARGET)
        };
        if need_recreate {
            self.ctx = None;
        }
        unsafe {
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn draw_frame(&mut self) {
        let (kpm, today) = {
            let snap = self.state.snapshot.read();
            let pulse = self.state.pulse.read();
            (
                pulse.kpm(),
                snap.today.as_ref().map(|t| t.total).unwrap_or(0),
            )
        };

        let compact = i18n::SETTINGS.read().widget_compact;
        let ctx = self.ctx.as_ref().unwrap();
        let (w_px, h_px) = ctx.size_px;
        let rect = Rect::new(0.0, 0.0, w_px as f32, h_px as f32);

        // Clear to fully transparent? No — opaque background is fine,
        // the LWA_ALPHA on the window already gives whole-window
        // translucency. Drawing a card frame is what the user expects
        // for the "Full" mode; the compact mode draws a pill instead.
        crate::ui::render::primitives::clear(ctx, Brush::Bg);

        if compact {
            // Compact pill: 1 line, KPM number + label.
            pill(ctx, rect.shrink(2.0, 2.0), Brush::Surface);
            crate::ui::render::primitives::stroke_rounded(
                ctx,
                rect.shrink(2.0, 2.0),
                rect.h * 0.5,
                Brush::Border,
                1.0,
            );
            let value = fmt_int(kpm);
            // Number on the left half, label on the right half.
            text(
                ctx,
                Rect::new(rect.x + 14.0, rect.y, rect.w * 0.5, rect.h),
                &value,
                Font::LiveKpm,
                if kpm > 0 { Brush::AccentStrong } else { Brush::Text },
                HAlign::Leading,
                VAlign::Centre,
            );
            text(
                ctx,
                Rect::new(rect.x + rect.w * 0.5, rect.y, rect.w * 0.5 - 14.0, rect.h),
                i18n::t("widget.kpm"),
                Font::Caption,
                Brush::TextDim,
                HAlign::Trailing,
                VAlign::Centre,
            );
        } else {
            // Full card
            card(ctx, rect.shrink(2.0, 2.0), 12.0);
            let inner = rect.shrink(14.0, 12.0);
            let (kpm_row, today_row) = inner.split_top(inner.h * 0.62);

            // KPM block
            text(
                ctx,
                Rect::new(kpm_row.x, kpm_row.y, kpm_row.w, 14.0),
                i18n::t("widget.kpm"),
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Top,
            );
            let glow = {
                let age = (self.now_ms() - self.last_event_ms).max(0);
                if age < 600 {
                    1.0 - (age as f32 / 600.0)
                } else {
                    0.0
                }
            };
            // Subtle dot indicator
            crate::ui::render::primitives::fill_circle(
                ctx,
                kpm_row.right() - 10.0,
                kpm_row.y + 10.0,
                3.0 + 2.0 * glow,
                Brush::Pulse,
            );
            text(
                ctx,
                Rect::new(kpm_row.x, kpm_row.y + 16.0, kpm_row.w, kpm_row.h - 16.0),
                &fmt_int(kpm),
                Font::Display,
                if kpm > 0 { Brush::AccentStrong } else { Brush::Text },
                HAlign::Leading,
                VAlign::Top,
            );

            // Today total
            text(
                ctx,
                Rect::new(today_row.x, today_row.y, today_row.w * 0.5, today_row.h),
                i18n::t("widget.today"),
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Centre,
            );
            text(
                ctx,
                Rect::new(today_row.x + today_row.w * 0.4, today_row.y, today_row.w * 0.6, today_row.h),
                &fmt_int(today),
                Font::BodyStrong,
                Brush::Text,
                HAlign::Trailing,
                VAlign::Centre,
            );
        }

        // Tiny accent corner — visual cue that the widget is grabable.
        let grab = Rect::new(rect.right() - 10.0, rect.bottom() - 10.0, 6.0, 6.0);
        fill_rounded(ctx, grab, 2.0, Brush::Border);
    }
}

/// Replace the window's clipping region with a rounded rectangle of
/// the correct radius for the active mode. GDI takes ownership of the
/// region handle after `SetWindowRgn` succeeds — passing `true` for
/// `bRedraw` triggers a non-client refresh so the shape change is
/// immediately visible. We don't bother deleting the previous region
/// because GDI clones the bits we passed in (and the next call replaces
/// it).
unsafe fn apply_rounded_region(hwnd: HWND, w_px: i32, h_px: i32, compact: bool) {
    // CreateRoundRectRgn uses width/height (in device units) for the
    // ellipse — i.e. 2× the corner radius. For the pill we want a full
    // capsule, so the diameter equals the window height; for the full
    // card we use ~26 px diameter (≈ 13 px corner radius) which matches
    // the painted card frame.
    let diameter = if compact { h_px } else { 26 };
    let rgn = CreateRoundRectRgn(0, 0, w_px + 1, h_px + 1, diameter, diameter);
    if !rgn.is_invalid() {
        let _ = SetWindowRgn(hwnd, rgn, true);
        // SetWindowRgn takes ownership on success — do NOT DeleteObject.
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_CREATE {
            let cs = &*(l.0 as *const CREATESTRUCTW);
            let state_ptr = cs.lpCreateParams as *mut AppState;
            let state = *Box::from_raw(state_ptr);
            let widget = Widget::new(hwnd, state);
            let raw = Box::into_raw(widget);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);
            let _ = SetTimer(hwnd, T_REFRESH, 500, None);
            return LRESULT(0);
        }
        let raw = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Widget;
        if raw.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let widget = &mut *raw;
        match msg {
            WM_PAINT => {
                widget.paint();
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                let mut rect = RECT::default();
                let _ = GetClientRect(hwnd, &mut rect);
                let w_px = (rect.right - rect.left).max(1);
                let h_px = (rect.bottom - rect.top).max(1);
                if let Some(ctx) = widget.ctx.as_mut() {
                    let _ = ctx.resize(w_px as u32, h_px as u32);
                }
                // Re-clip — DPI and content swaps both flow through here.
                let compact = i18n::SETTINGS.read().widget_compact;
                apply_rounded_region(hwnd, w_px, h_px, compact);
                LRESULT(0)
            }
            WM_TIMER if w.0 as usize == T_REFRESH => {
                widget.refresh();
                LRESULT(0)
            }
            WM_NCHITTEST => {
                // Whole client area acts as a titlebar so the user can
                // drag the widget without aiming at a 4 px strip.
                LRESULT(HTCAPTION as isize)
            }
            WM_LBUTTONDOWN => {
                // Already handled by WM_NCHITTEST returning HTCAPTION,
                // but keep this here for completeness — if HTCAPTION is
                // ever changed we still want a no-op rather than a
                // default activation pulse.
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(hwnd, T_REFRESH);
                let _ = Box::from_raw(raw);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

/// Spawn the widget window. Runs alongside the main window in the same
/// message loop — both windows share the process's GetMessageW pump.
/// Returns the HWND so the caller can show / hide it later (Push 4
/// will wire that to the tray menu).
pub fn spawn(state: AppState) -> Result<HWND> {
    unsafe {
        let hmodule = GetModuleHandleW(None).context("GetModuleHandleW")?;
        let hinstance: HINSTANCE = HINSTANCE(hmodule.0);
        let cursor: HCURSOR = LoadCursorW(None, IDC_HAND).context("LoadCursorW")?;

        let wc = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: hinstance,
            hCursor: cursor,
            hbrBackground: HBRUSH::default(),
            hIcon: HICON::default(),
            hIconSm: HICON::default(),
            lpszClassName: CLASS_NAME,
            lpszMenuName: PCWSTR::null(),
            cbClsExtra: 0,
            cbWndExtra: 0,
        };
        // Class may already be registered if the widget was previously
        // shown and hidden — RegisterClassExW will return 0 in that case
        // and GetLastError gives ERROR_CLASS_ALREADY_EXISTS, which is
        // benign. We don't surface it.
        let _ = RegisterClassExW(&wc);

        let ex = WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
        let style = WS_POPUP;

        let state_box = Box::new(state);
        let state_ptr = Box::into_raw(state_box);

        // Honour the persisted compact mode on first paint so we don't
        // briefly flash the full card before the timer-driven resize.
        let initial_compact = i18n::SETTINGS.read().widget_compact;
        let (init_w, init_h) = if initial_compact { COMPACT_DIPS } else { FULL_DIPS };

        let hwnd = CreateWindowExW(
            ex,
            CLASS_NAME,
            TITLE,
            style,
            // Initial position — top-left of primary monitor (16, 16).
            // The user can drag; snap-to-taskbar repositions when on.
            16,
            16,
            init_w,
            init_h,
            HWND::default(),
            HMENU::default(),
            hinstance,
            Some(state_ptr as *const _),
        )
        .context("CreateWindowExW(widget)")?;

        // Start at the configured opacity.
        let opacity = i18n::SETTINGS.read().widget_opacity;
        let _ = SetLayeredWindowAttributes(
            hwnd,
            windows::Win32::Foundation::COLORREF(0),
            (255 * opacity as u32 / 100).clamp(40, 255) as u8,
            LWA_ALPHA,
        );

        // Clip the window to a rounded rectangle so the layered bg
        // doesn't show its square footprint outside the painted card.
        let dpi = GetDpiForWindow(hwnd).max(96) as f32;
        let scale = dpi / 96.0;
        let compact = i18n::SETTINGS.read().widget_compact;
        let (w, h) = if compact { COMPACT_DIPS } else { FULL_DIPS };
        apply_rounded_region(hwnd, (w as f32 * scale) as i32, (h as f32 * scale) as i32, compact);

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);

        // Publish for the tray/menu glue to find.
        crate::ui::shared::set_widget_hwnd(hwnd);
        crate::ui::shared::set_widget_visible(true);
        Ok(hwnd)
    }
}
