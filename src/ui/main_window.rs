// Main window — Win32 class registration, message pump, WNDPROC.
//
// Architecture:
//   * One Window struct lives on the heap (`Box<Window>`); its raw
//     pointer is stuffed into GWLP_USERDATA on WM_NCCREATE so every
//     subsequent WNDPROC dispatch can grab it back without globals.
//   * WM_PAINT calls into Window::paint, which manages the
//     RenderContext (lazy-create on first paint, recreate on
//     D2DERR_RECREATE_TARGET).
//   * Three timers:
//       T_PULSE  500 ms  — repaint for the pulse animation
//       T_LIVE     1 s   — refresh live snapshot
//       T_FULL     5 s   — full DataSnapshot::refresh_full
//   * WM_TIMER triggers the appropriate refresh and InvalidateRect.
//   * Resize handles WM_SIZE → ctx.resize. DPI changes go through
//     WM_DPICHANGED which updates the cached scale and reissues the
//     window rect with the suggested coords.
//   * Mouse input feeds a small InputState that the immediate-mode
//     controls in sidebar / views consume.

use std::sync::atomic::Ordering;

use anyhow::{Context, Result};
use chrono::Utc;
use windows::core::{w, HRESULT, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, UpdateWindow, HBRUSH, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, SetProcessDpiAwarenessContext,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetClientRect, GetMessageW,
    GetWindowLongPtrW, IsWindowVisible, KillTimer, LoadCursorW, MessageBoxW, PostQuitMessage,
    RegisterClassExW, SetForegroundWindow, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    TranslateMessage, CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HCURSOR,
    HICON, HMENU, IDC_ARROW, IDYES, MB_ICONWARNING, MB_YESNO, MINMAXINFO, MSG, SWP_NOACTIVATE,
    SWP_NOZORDER, SW_HIDE, SW_RESTORE, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE,
    WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_GETMINMAXINFO, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_LBUTTONDBLCLK, WM_MOUSEMOVE, WM_PAINT, WM_RBUTTONUP, WM_SIZE, WM_TIMER, WNDCLASSEXW,
    WS_OVERLAPPEDWINDOW,
};

/// HRESULT D2D returns from `EndDraw` when the render target needs to
/// be rebuilt (GPU reset, monitor change, sleep/resume). Not exposed
/// as a named constant by windows-rs 0.58, so we inline the value
/// straight from d2derr.h.
pub const D2DERR_RECREATE_TARGET: HRESULT = HRESULT(0x8899000Cu32 as i32);

use crate::app::AppState;
use crate::core::i18n;
use crate::core::stats::DataSnapshot;
use crate::system::tray::{
    self, ID_QUIT, ID_SHOW, ID_TOGGLE_PAUSE, ID_TOGGLE_WIDGET, WM_TRAY_CALLBACK,
};
use crate::ui::controls::InputState;
use crate::ui::live_pulse;
use crate::ui::render::primitives::{clear, Rect};
use crate::ui::render::{Brush, RenderContext};
use crate::ui::sidebar;
use crate::ui::views::View;

const CLASS_NAME: PCWSTR = w!("KeyCounter.MainWindow");
const TITLE: PCWSTR = w!("KeyCounter");

const T_PULSE: usize = 1;
const T_LIVE: usize = 2;
const T_FULL: usize = 3;
/// 60 FPS animation tick. Fires for the duration of an active pulse
/// glow only — `KillTimer` stops it once `now - last_event_ms` exceeds
/// the glow window, so idle CPU stays at the snapshot cadence.
const T_ANIM: usize = 4;

/// Animation tick spacing (ms). 16 ms ≈ 60 FPS, which is what the
/// React/CSS version of KeyCounter ran at and what the pulse decay
/// curve was tuned for.
const ANIM_TICK_MS: u32 = 16;

/// How long the pulse dot stays animated after the last keystroke.
/// Slightly longer than before (700 → 900 ms) so the ease-out tail is
/// visible without being distracting.
pub const PULSE_GLOW_MS: i64 = 900;

const INITIAL_DIPS: (i32, i32) = (1100, 720);

/// Floor on the window's tracking size. Below this the sidebar + top
/// bar + 4-card dashboard row starts to overlap, so we let the OS clamp
/// the drag rather than try to gracefully reflow the layout. Tuned to
/// the widest "must fit" row: sidebar (220) + 4 dashboard cards
/// (≈ 4 × 160 + 3 × 16) + outer padding ≈ 940. Vertically the sidebar
/// stack + dashboard hero row + hourly chart wants ≈ 620.
const MIN_TRACK_DIPS: (i32, i32) = (940, 620);

struct Window {
    hwnd: HWND,
    state: AppState,
    ctx: Option<RenderContext>,
    input: InputState,
    view: View,
    /// Stamp of the last keyboard event we observed in the live
    /// counter — used to drive the pulse glow without polling the
    /// hook channel directly.
    last_event_ms: i64,
    /// Last value of state.live_counter from the previous tick;
    /// difference becomes the new pulse sample.
    prev_counter: i64,
}

impl Window {
    fn new(hwnd: HWND, state: AppState) -> Box<Self> {
        Box::new(Self {
            hwnd,
            state,
            ctx: None,
            input: InputState::default(),
            view: View::Dashboard,
            last_event_ms: 0,
            prev_counter: 0,
        })
    }

    fn now_ms(&self) -> i64 {
        // Wall-clock for the live snapshot windows; we use the
        // hook-side counter delta on the same axis. Utc::now is fine
        // because the deltas are local to a single process tick.
        Utc::now().timestamp_millis()
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

    /// Sync the hook counter into the pulse history. Runs every
    /// 100 ms — the animation timer (16 ms) handles the visible glow
    /// decay, so we don't need to repaint here unless there's actual
    /// activity to surface.
    fn tick_pulse(&mut self) {
        let counter = self.state.live_counter.load(Ordering::Relaxed);
        let delta = counter - self.prev_counter;
        self.prev_counter = counter;
        let now = self.now_ms();
        if delta > 0 {
            self.last_event_ms = now;
            self.state.pulse.write().record(now, delta);
            self.ensure_anim_timer();
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        } else {
            // No new keystrokes — still trim the pulse history so KPM
            // ticks down naturally. `record(_, 0)` does the prune.
            self.state.pulse.write().record(now, 0);
            // Idle repaint at the 100 ms cadence so the KPM block
            // animates down even if no new events arrive.
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    fn ensure_anim_timer(&self) {
        unsafe {
            let _ = SetTimer(self.hwnd, T_ANIM, ANIM_TICK_MS, None);
        }
    }

    fn tick_anim(&mut self) {
        let age = self.now_ms() - self.last_event_ms;
        if age >= PULSE_GLOW_MS {
            unsafe {
                let _ = KillTimer(self.hwnd, T_ANIM);
            }
            return;
        }
        unsafe {
            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    fn tick_live(&mut self) {
        if let Ok(conn) = self.state.store.reader() {
            let mut snap = self.state.snapshot.write();
            snap.refresh_live(&conn);
        }
    }

    fn tick_full(&mut self) {
        if let Ok(conn) = self.state.store.reader() {
            let fresh = DataSnapshot::refresh_full(&conn);
            *self.state.snapshot.write() = fresh;
            unsafe {
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    fn handle_resize(&mut self) {
        let mut rect = RECT::default();
        unsafe {
            let _ = GetClientRect(self.hwnd, &mut rect);
        }
        let w = (rect.right - rect.left).max(1) as u32;
        let h = (rect.bottom - rect.top).max(1) as u32;
        if let Some(ctx) = self.ctx.as_mut() {
            let _ = ctx.resize(w, h);
        }
    }

    fn handle_dpi(&mut self, new_dpi: u32, suggested: &RECT) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                suggested.left,
                suggested.top,
                suggested.right - suggested.left,
                suggested.bottom - suggested.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        if let Some(ctx) = self.ctx.as_mut() {
            let _ = ctx.set_dpi(new_dpi);
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
            match result {
                Err(e) if e.code() == D2DERR_RECREATE_TARGET => true,
                _ => false,
            }
        };
        if need_recreate {
            self.ctx = None;
        }
        unsafe {
            let _ = EndPaint(self.hwnd, &ps);
        }
    }

    fn draw_frame(&mut self) {
        // Capture every value the per-section closures need *before* we
        // start juggling borrows. The `self.ctx.as_ref().unwrap()` calls
        // below each take an immutable borrow of self for their scope
        // only — interleaving them with `&mut self.input` requires that
        // each scope completes before the next opens.
        let (scale, canvas) = {
            let ctx = self.ctx.as_ref().unwrap();
            clear(ctx, Brush::Bg);
            let (w_px, h_px) = ctx.size_px;
            (ctx.dpi_scale, Rect::new(0.0, 0.0, w_px as f32, h_px as f32))
        };

        let sb_w = sidebar::WIDTH * scale;
        let (sb_rect, rest) = canvas.split_left(sb_w);
        let tb_h = live_pulse::HEIGHT * scale;
        let (tb_rect, body_rect) = rest.split_top(tb_h);

        let view = self.view;
        let state_clone = self.state.clone();

        // Sidebar
        let sidebar_out = {
            let ctx = self.ctx.as_ref().unwrap();
            sidebar::draw(ctx, sb_rect, view, &state_clone, &mut self.input)
        };

        // Top bar — copy the scalar deps out of self so we can hold ctx
        // and the snapshot read-guards together without aliasing.
        let last_event_ms = self.last_event_ms;
        let now_ms = self.now_ms();
        {
            let snap = self.state.snapshot.read();
            let pulse = self.state.pulse.read();
            let ctx = self.ctx.as_ref().unwrap();
            live_pulse::draw(
                ctx,
                tb_rect,
                &pulse,
                &snap.live,
                snap.lifetime,
                last_event_ms,
                now_ms,
            );
        }

        // Active view
        let view_out = {
            let ctx = self.ctx.as_ref().unwrap();
            crate::ui::views::draw(ctx, body_rect, view, &state_clone, &mut self.input)
        };

        self.apply_sidebar_output(sidebar_out);
        self.apply_view_output(view_out);
    }

    fn apply_view_output(&mut self, out: crate::ui::views::ViewOutput) {
        if let Some(theme) = out.picked_theme {
            i18n::SETTINGS.write().theme = theme;
            if let Some(ctx) = self.ctx.as_mut() {
                let _ = ctx.set_theme(theme);
            }
        }
        if out.reset_requested {
            self.confirm_and_reset();
        }
        if out.export_requested {
            match crate::system::export::save_dialog(self.hwnd, &self.state) {
                Ok(Some(path)) => log::info!("export saved to {}", path.display()),
                Ok(None) => log::info!("export cancelled"),
                Err(e) => log::warn!("export failed: {e}"),
            }
        }
        if out.autostart_changed {
            let enabled = i18n::SETTINGS.read().autostart;
            if let Err(e) = crate::system::autostart::apply(enabled) {
                log::warn!("autostart apply failed: {e}");
            }
        }
    }

    fn confirm_and_reset(&mut self) {
        // MessageBox is a modal blocking call. That's fine — the user
        // already clicked Reset and expects the world to pause until
        // they confirm.
        let title: Vec<u16> = "KeyCounter\0".encode_utf16().collect();
        let body: Vec<u16> =
            "Wipe the local database? All counts and achievements will be lost.\0"
                .encode_utf16()
                .collect();
        let answer = unsafe {
            MessageBoxW(
                self.hwnd,
                PCWSTR(body.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_YESNO | MB_ICONWARNING,
            )
        };
        if answer != IDYES {
            return;
        }
        // We open a fresh write connection — the store's reader() gives
        // us a read-only handle, and `reset_all` needs a transaction.
        match rusqlite::Connection::open(self.state.store.db_path()) {
            Ok(mut conn) => {
                if let Err(e) = crate::core::store::reset_all(&mut conn) {
                    log::warn!("reset_all failed: {e}");
                } else {
                    self.state.live_counter.store(0, Ordering::Relaxed);
                    self.prev_counter = 0;
                    self.state.pulse.write().clear();
                    self.tick_full();
                }
            }
            Err(e) => log::warn!("reset: open write conn failed: {e}"),
        }
    }

    fn apply_sidebar_output(&mut self, out: sidebar::SidebarOutput) {
        if let Some(v) = out.picked_view {
            self.view = v;
        }
        if out.toggled_pause {
            self.state.toggle_paused();
        }
        if let Some(theme) = out.picked_theme {
            i18n::SETTINGS.write().theme = theme;
            if let Some(ctx) = self.ctx.as_mut() {
                let _ = ctx.set_theme(theme);
            }
        }
        if let Some(lang) = out.picked_lang {
            i18n::SETTINGS.write().lang = lang;
        }
    }
}

extern "system" fn wnd_proc(hwnd: HWND, msg: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_CREATE {
            // Stash the AppState pointer that came in via lpCreateParams.
            let cs = &*(l.0 as *const CREATESTRUCTW);
            let state_ptr = cs.lpCreateParams as *mut AppState;
            // Reconstitute the boxed AppState (owned by this WNDPROC scope).
            let state = *Box::from_raw(state_ptr);
            let window = Window::new(hwnd, state);
            let raw = Box::into_raw(window);
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, raw as isize);

            // Set up timers as soon as the window exists.
            // T_PULSE drives the hook-counter delta sync; we run it at
            // 100 ms so the pulse glow starts visibly within a frame or
            // two of a key landing. T_FULL drops to 1.5 s so today's
            // total + lifetime catch up without feeling laggy. T_ANIM
            // is registered lazily on the first keystroke.
            let _ = SetTimer(hwnd, T_PULSE, 100, None);
            let _ = SetTimer(hwnd, T_LIVE, 1_000, None);
            let _ = SetTimer(hwnd, T_FULL, 1_500, None);

            // Install tray icon. A failure is non-fatal — without the
            // tray the user can still close via the titlebar — but log
            // so we know the install pass dropped it.
            if let Err(e) = tray::install(hwnd, "KeyCounter") {
                log::warn!("tray install failed: {e}");
            }
            return LRESULT(0);
        }

        let win_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Window;
        if win_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, w, l);
        }
        let win = &mut *win_ptr;

        match msg {
            WM_PAINT => {
                win.paint();
                LRESULT(0)
            }
            WM_ERASEBKGND => LRESULT(1), // handled in WM_PAINT (D2D clears)
            WM_SIZE => {
                win.handle_resize();
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                let new_dpi = (w.0 as u32) & 0xFFFF;
                let suggested = &*(l.0 as *const RECT);
                win.handle_dpi(new_dpi, suggested);
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_GETMINMAXINFO => {
                // Clamp resize at the layout's natural minimum. The
                // OS gives us a pointer to its own MINMAXINFO to fill
                // in. We compute the window-coord minimum (i.e.
                // including the titlebar + borders the current DPI's
                // theme adds) so the *content* area really gets the
                // DIPs we asked for.
                let info = &mut *(l.0 as *mut MINMAXINFO);
                let dpi = GetDpiForWindow(hwnd).max(96);
                let scale = dpi as f32 / 96.0;
                let mut r = RECT {
                    left: 0,
                    top: 0,
                    right: (MIN_TRACK_DIPS.0 as f32 * scale) as i32,
                    bottom: (MIN_TRACK_DIPS.1 as f32 * scale) as i32,
                };
                let _ = AdjustWindowRectExForDpi(
                    &mut r,
                    WS_OVERLAPPEDWINDOW,
                    false,
                    WINDOW_EX_STYLE(0),
                    dpi,
                );
                info.ptMinTrackSize.x = r.right - r.left;
                info.ptMinTrackSize.y = r.bottom - r.top;
                LRESULT(0)
            }
            WM_TIMER => {
                match w.0 as usize {
                    T_PULSE => win.tick_pulse(),
                    T_LIVE => win.tick_live(),
                    T_FULL => win.tick_full(),
                    T_ANIM => win.tick_anim(),
                    _ => {}
                }
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                let x = ((l.0 & 0xFFFF) as i16) as f32;
                let y = (((l.0 >> 16) & 0xFFFF) as i16) as f32;
                win.input.mouse_x = x;
                win.input.mouse_y = y;
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                win.input.mouse_down = true;
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_LBUTTONUP => {
                win.input.mouse_down = false;
                win.input.click_pending = true;
                let _ = InvalidateRect(hwnd, None, false);
                LRESULT(0)
            }
            WM_CLOSE => {
                // Closing via [X] hides the window instead of quitting
                // — the tray icon stays so the counter keeps running.
                // ID_QUIT in the tray menu posts a real destroy.
                let _ = ShowWindow(hwnd, SW_HIDE);
                LRESULT(0)
            }
            WM_TRAY_CALLBACK => {
                // lParam.low = the mouse event the user generated; the
                // high word is the icon id (always TRAY_UID here).
                let event = (l.0 & 0xFFFF) as u32;
                match event {
                    WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                        // Toggle visibility of the main window — single
                        // click brings it back, click again hides.
                        if IsWindowVisible(hwnd).as_bool() {
                            let _ = ShowWindow(hwnd, SW_HIDE);
                        } else {
                            let _ = ShowWindow(hwnd, SW_RESTORE);
                            let _ = SetForegroundWindow(hwnd);
                        }
                    }
                    WM_RBUTTONUP => {
                        let widget_visible = crate::ui::shared::is_widget_visible();
                        let _ = tray::show_menu(hwnd, win.state.is_paused(), widget_visible);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_COMMAND => {
                let id = (w.0 & 0xFFFF) as u16;
                match id {
                    ID_SHOW => {
                        let _ = ShowWindow(hwnd, SW_RESTORE);
                        let _ = SetForegroundWindow(hwnd);
                    }
                    ID_TOGGLE_PAUSE => {
                        win.state.toggle_paused();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                    ID_TOGGLE_WIDGET => {
                        if let Some(wh) = crate::ui::shared::widget_hwnd() {
                            let now_visible = !crate::ui::shared::is_widget_visible();
                            let _ = ShowWindow(
                                wh,
                                if now_visible { SW_SHOW } else { SW_HIDE },
                            );
                            crate::ui::shared::set_widget_visible(now_visible);
                        }
                    }
                    ID_QUIT => {
                        let _ = DestroyWindow(hwnd);
                    }
                    _ => {}
                }
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(hwnd, T_PULSE);
                let _ = KillTimer(hwnd, T_LIVE);
                let _ = KillTimer(hwnd, T_FULL);
                let _ = KillTimer(hwnd, T_ANIM);
                let _ = tray::remove(hwnd);
                // Also tear down the widget so it doesn't outlive the
                // main window (its WNDPROC would dereference a stale
                // AppState clone otherwise).
                if let Some(wh) = crate::ui::shared::widget_hwnd() {
                    let _ = DestroyWindow(wh);
                }
                // Drop the boxed window.
                let _ = Box::from_raw(win_ptr);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, w, l),
        }
    }
}

pub fn run(state: AppState) -> Result<()> {
    unsafe {
        // Per-monitor V2 DPI awareness is also asserted in the manifest;
        // the runtime call is a belt-and-braces for older Win10 builds
        // that ignore the manifest entry on cold start.
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
    // GetModuleHandleW returns HMODULE; HINSTANCE is structurally
    // identical (both wrap an isize handle) so we re-tag explicitly —
    // windows-rs 0.58 doesn't expose a From impl between the two.
    let hmodule = unsafe { GetModuleHandleW(None) }.context("GetModuleHandleW")?;
    let hinstance: HINSTANCE = HINSTANCE(hmodule.0);

    let cursor: HCURSOR = unsafe { LoadCursorW(None, IDC_ARROW) }.context("LoadCursorW")?;

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
    unsafe {
        let atom = RegisterClassExW(&wc);
        if atom == 0 {
            anyhow::bail!("RegisterClassExW failed");
        }
    }

    // Adjust the requested client size for the system-default DPI; the
    // window will be resized again on first WM_DPICHANGED if the user
    // is on a non-96 DPI monitor.
    let dpi = 96u32;
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: INITIAL_DIPS.0,
        bottom: INITIAL_DIPS.1,
    };
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut rect, WS_OVERLAPPEDWINDOW, false, WINDOW_EX_STYLE(0), dpi);
    }

    // Clone the AppState before we move it into the main window's
    // CREATESTRUCT.lpCreateParams. The clone goes to the floating
    // widget, which is its own top-level window but rides the same
    // GetMessageW pump.
    let widget_state = state.clone();
    let state_box = Box::new(state);
    let state_ptr = Box::into_raw(state_box);
    let hwnd = unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            CLASS_NAME,
            TITLE,
            WS_OVERLAPPEDWINDOW,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            rect.right - rect.left,
            rect.bottom - rect.top,
            HWND::default(),
            HMENU::default(),
            hinstance,
            Some(state_ptr as *const _),
        )
    }
    .context("CreateWindowExW")?;

    unsafe {
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = UpdateWindow(hwnd);
    }

    // Spawn the floating widget alongside. It rides the same message
    // pump (GetMessageW dispatches to every window in the thread). A
    // failure here shouldn't take down the main window — log and
    // continue.
    if let Err(e) = crate::ui::floating_widget::spawn(widget_state) {
        log::warn!("floating widget spawn failed: {e}");
    }

    // Standard message loop.
    let mut msg = MSG::default();
    unsafe {
        while GetMessageW(&mut msg, HWND::default(), 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    let _ = hwnd; // hwnd is dropped when the message loop exits
    Ok(())
}
