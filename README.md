# KeyCounter — NativeShell

A lightweight, native rewrite of [KeyCounter](https://github.com/perlessmtfuji/keycounter). Same feature set as the Tauri version, but with no WebView — pure Win32 + Direct2D, one small `.exe`.

[Wersja polska](README.pl.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.77+-orange)](https://www.rust-lang.org/)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078d4)](#)

---

## Why a separate repo?

The Tauri version works, but it ships a full WebView2 (Chromium) per application. At idle that's ~50 MB RAM and measurable CPU/GPU usage on every UI animation. For a keystroke counter that mostly waits and ticks the visible number once a second, that's a lot of overhead.

NativeShell is the same idea, rebuilt from the bottom up:

- **UI:** Direct2D + DirectWrite, drawn via `WM_PAINT` on demand. No HTML compositor, no JS, no DOM.
- **Hook:** raw `WH_KEYBOARD_LL` instead of `rdev`. ~30 lines, the callback only does `atomic.fetch_add(1)` + an MPSC push.
- **Storage:** SQLite (rusqlite with bundled), same batched-writes pattern as the Tauri version.
- **Tray:** `Shell_NotifyIconW`, `RegisterClassExW`. No JS framework involved.
- **Widget:** layered window (`WS_EX_LAYERED`) for full opacity control.

**Target:** ≤ 10 MB RAM at idle, < 0.1 % CPU under fast typing. Binary < 4 MB.

## Features (parity with the Tauri version)

- Live KPM with pulse
- Keyboard heatmap (QWERTY, QWERTZ, Dvorak, Colemak)
- Top keys: today / 7 d / 30 d / all-time
- Finger load under standard touch typing
- Day × hour punch card (30 d) and a 365-day activity calendar
- Streak, modifier mix, backspace ratio
- Achievements with toasts
- Tray (show / hide / pause / quit), autostart via `HKCU\...\Run`
- Floating widget (full + compact pill, snap above the taskbar, adjustable opacity & tint)
- JSON export, full history reset
- Polish and English, dark and light themes

## Status

Early development. Core modules (hook, store, keycode, achievements, i18n, layouts) are stable. UI is still being polished — see `docs/roadmap.md`.

| Module | Status |
|---|---|
| Keyboard hook (LL) | ✅ |
| SQLite store + queries | ✅ |
| Main window + sidebar | ✅ |
| Dashboard view | ✅ |
| Heatmap view | ✅ |
| Stats view | ✅ |
| Achievements view | ✅ |
| Settings view | ✅ |
| Floating widget | ✅ (compact + full) |
| i18n PL/EN | ✅ |
| Light theme | ✅ |
| Pulse animations | ✅ |
| Tray icon + menu | ⏳ |
| Autostart | ⏳ |
| JSON export | ⏳ |
| MSI installer | ⏳ |
| Code signing | ⏳ |

## Build

Requirements:

- **Rust 1.77+** (install via [`rustup`](https://rustup.rs/))
- **Visual Studio Build Tools 2022** with the "Desktop development with C++" workload — needed for `link.exe` and `rc.exe` (manifest + icon compilation)
- Windows 10 1809+ or Windows 11

```powershell
git clone https://github.com/perlessmtfuji/keycounternativeshell.git
cd keycounternativeshell

cargo run                  # debug, opens window + tray
cargo run --release        # size-optimised release build
cargo build --release      # production binary at target\release\keycounter.exe
```

A release binary is typically ~3-4 MB. Debug builds are ~30-40 MB (symbols).

## Architecture

```
┌────────────────────────────────────────────────────────────┐
│                    KeyCounter NativeShell                  │
│                                                            │
│  ┌────────────────────────┐   ┌─────────────────────────┐  │
│  │  UI (Direct2D)         │   │   Core (Rust)           │  │
│  │  ────────────────────  │   │   ────────────────────  │  │
│  │  • main window HWND    │◄──┤  • WH_KEYBOARD_LL hook  │  │
│  │  • widget (layered)    │   │  • SQLite store thread  │  │
│  │  • sidebar + 5 views   │   │  • aggregator (256/3s)  │  │
│  │  • cached brushes/text │   │  • Shell_NotifyIcon     │  │
│  │  • WM_PAINT on demand  │   │  • Atomic counters      │  │
│  └────────────────────────┘   └─────────────────────────┘  │
│              ▲                            │                │
│              └─── Arc<AppState> ──────────┘                │
└────────────────────────────────────────────────────────────┘
                              │
                              ▼
                  %LOCALAPPDATA%\KeyCounter\
                          keycounter.db
```

More in [docs/architecture.md](docs/architecture.md).

## Performance

Measured on a stock Windows 11 box, typing at ~120 KPM:

| Metric | KeyCounter (Tauri) | KeyCounter NativeShell |
|---|---|---|
| RAM at idle | ~50 MB | < 10 MB |
| RAM with window open | ~80-110 MB | < 25 MB |
| CPU while typing | 1-3 % | < 0.2 % |
| GPU during animations | measurable | negligible (on-demand D2D) |
| Binary size | ~10 MB | ~3-4 MB |
| Cold start | ~600 ms | < 100 ms |

The principle: at idle the app blocks in `GetMessageW` — zero polling. The hook callback is a single `fetch_add(1)`. The storage thread wakes every 3 s or after 256 events. The UI redraws only on `WM_PAINT`, which we never trigger more than 2 Hz (for the pulse animation), and only with a visible window.

## Antivirus

Same caveat as the original: `WH_KEYBOARD_LL` is the same OS primitive that keyloggers use. An unsigned build will trip SmartScreen on first launch. Full breakdown in [docs/antivirus.md](docs/antivirus.md).

## License

MIT — see [LICENSE](LICENSE).
