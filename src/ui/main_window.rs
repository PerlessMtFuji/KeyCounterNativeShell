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
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::D2DERR_RECREATE_TARGET;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, UpdateWindow, HBRUSH, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForWindow, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectExForDpi, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, GetWindowLongPtrW, KillTimer, LoadCursorW, PostQuitMessage, RegisterClassExW,
    SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, CW_USEDEFAULT, GWLP_USERDATA, HCURSOR, HICON, HMENU, IDC_ARROW, MSG,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WM_CLOSE, WM_CREATE, WM_DESTROY,
    WM_DPICHANGED, WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_PAINT, WM_SIZE,
    WM_TIMER, WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
};

use crate::app::AppState;
use crate::core::i18n;
use crate::core::stats::DataSnapshot;
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

const INITIAL_DIPS: (i32, i32) = (1100, 720);

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

    /// Tick: pulse counter + live snapshot.
    fn tick_pulse(&mut self) {
        let counter = self.state.live_counter.load(Ordering::Relaxed);
        let delta = counter - self.prev_counter;
        self.prev_counter = counter;
        if delta > 0 {
            self.last_event_ms = self.now_ms();
        }
        let now = self.now_ms();
        self.state.pulse.write().record(now, delta.max(0));
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
        {
            let ctx = self.ctx.as_ref().unwrap();
            crate::ui::views::draw(ctx, body_rect, view, &state_clone, &mut self.input);
        }

        self.apply_sidebar_output(sidebar_out);
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
            let _ = SetTimer(hwnd, T_PULSE, 500, None);
            let _ = SetTimer(hwnd, T_LIVE, 1_000, None);
            let _ = SetTimer(hwnd, T_FULL, 5_000, None);
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
            WM_TIMER => {
                match w.0 as usize {
                    T_PULSE => win.tick_pulse(),
                    T_LIVE => win.tick_live(),
                    T_FULL => win.tick_full(),
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
                let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(hwnd);
                LRESULT(0)
            }
            WM_DESTROY => {
                let _ = KillTimer(hwnd, T_PULSE);
                let _ = KillTimer(hwnd, T_LIVE);
                let _ = KillTimer(hwnd, T_FULL);
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

    // Box the AppState so we can pass it through CREATESTRUCT.lpCreateParams.
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
