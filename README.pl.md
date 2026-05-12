# KeyCounter — NativeShell

Natywna, lekka wersja [KeyCountera](https://github.com/perlessmtfuji/keycounter). Pełna funkcjonalność oryginału, ale bez WebView — czysty Win32 + Direct2D, jeden mały plik `.exe`.

[English version](README.md)

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/Rust-1.77+-orange)](https://www.rust-lang.org/)
[![Platform: Windows](https://img.shields.io/badge/Platform-Windows%2010%2F11-0078d4)](#)

---

## Po co osobne repo?

Wersja Tauri działa, ale zaszywa pełen WebView2 (Chromium) per aplikacja. W idle to ~50 MB RAM i mierzalne zużycie CPU/GPU przy każdej animacji UI. Dla licznika klawiszy, który ma głównie czekać i raz na sekundę odświeżyć liczbę, to spory narzut.

NativeShell to ten sam pomysł, ale od dołu:

- **UI:** Direct2D + DirectWrite, rysowane przez WM_PAINT na żądanie. Bez compositora HTML, bez JS, bez DOM.
- **Hook:** czyste `WH_KEYBOARD_LL` zamiast `rdev`. ~30 linii kodu, callback robi tylko `atomic.fetch_add(1)` + push do MPSC.
- **Storage:** SQLite (rusqlite z bundled), ten sam wzorzec batched-writes co w wersji Tauri.
- **Tray:** `Shell_NotifyIconW`, `RegisterClassExW`. Bez JS frameworka.
- **Widget:** layered window (`WS_EX_LAYERED`) dla pełnej kontroli krycia.

**Cel:** ≤ 10 MB RAM idle, < 0.1 % CPU przy szybkim pisaniu. Binarka < 4 MB.

## Funkcje (parytet z wersją Tauri)

- Live KPM z pulsem
- Heatmapa klawiatury (QWERTY, QWERTZ, Dvorak, Colemak)
- Top klawisze: dziś / 7 dni / 30 dni / cały czas
- Obciążenie palców przy standardowym touch typingu
- Punch card dzień × godzina (30 dni) i 365-dniowy kalendarz aktywności
- Streak, mix modyfikatorów, stosunek backspace
- Achievementy z toast'ami
- Tray (show / hide / pause / quit), autostart przez rejestr `HKCU\...\Run`
- Floating widget (full + compact pill, snap nad pasek zadań, regulowane krycie i tint)
- Eksport do JSON, reset całej historii
- Polski i angielski, ciemny i jasny motyw

## Status

Wczesny development. Architektura i moduły core (hook, store, keycode, achievementy, i18n, layouts) są stabilne. UI wciąż się dopracowuje — patrz `docs/roadmap.md`.

| Moduł | Status |
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
| Animacje pulsu | ✅ |
| Tray icon + menu | ✅ |
| Autostart | ✅ |
| Eksport JSON | ✅ |
| Zapis ustawień | ✅ |
| Instalator MSI | ⏳ |
| Code signing | ⏳ |

## Build

Wymagania:

- **Rust 1.77+** (instalacja przez [`rustup`](https://rustup.rs/))
- **Visual Studio Build Tools 2022** z komponentem "Desktop development with C++" — potrzebne dla `link.exe` i `rc.exe` (kompilacji manifestu i ikon)
- Windows 10 1809+ lub Windows 11

```powershell
git clone https://github.com/perlessmtfuji/keycounternativeshell.git
cd keycounternativeshell

cargo run                  # debug, otwiera okno + tray
cargo run --release        # build optymalizowany pod rozmiar
cargo build --release      # produkcyjna binarka w target\release\keycounter.exe
```

Pełna binarka release waży typowo ~3-4 MB. W debug ~30-40 MB (symbole).

## Architektura

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

Więcej w [docs/architecture.md](docs/architecture.md).

## Wydajność

Mierzone na typowym Windows 11. Wyniki ze schowanym oknem zakładają,
że widżet jest widoczny — tray zlicza dalej nawet gdy oba okna są
ukryte.

| Scenariusz | CPU | GPU |
|---|---|---|
| Idle, okno schowane (sam widżet) | 0.06 – 0.1 % | 0.01 – 0.05 % |
| Pisanie ~160 KPM, okno schowane | 0.1 – 0.2 % | 0.05 – 0.08 % |
| Idle, okno + widżet widoczne | 0.2 – 0.6 % | 0.3 – 0.6 % |
| Pisanie ~160 KPM, okno + widżet | 0.2 – 0.3 % | 2 – 2.5 % |

RAM ≤ 10 MB w idle, ≤ 25 MB z otwartym oknem. Binarka ~3-4 MB.
Cold start < 100 ms. Dla porównania: Tauri w idle ciągnął ~50 MB RAM
i 1-3 % CPU przy pisaniu.

Zasada: w trybie idle aplikacja śpi w `GetMessageW` — zero pollingu.
Hook callback jest pojedynczym `fetch_add(1)`. Storage wątek budzi się
co 3 s lub po 256 zdarzeniach. Główne okno przemaluje się tylko na
`WM_PAINT`; podczas pisania 30 FPS animacja pulsu invaliduje wąski
pasek na górze, więc dzięki `D2D1_PRESENT_OPTIONS_RETAIN_CONTENTS` D2D
przemaluje tylko ten pasek. Pływający widżet invaliduje się tylko gdy
zmieni się rysowana liczba (KPM).

## Antywirus

Tak samo jak oryginał: `WH_KEYBOARD_LL` to ta sama prymityw systemowa, której używają keyloggery. Niepodpisany build dostanie wpis w SmartScreen przy pierwszym uruchomieniu. Pełen rozkład w [docs/antivirus.pl.md](docs/antivirus.pl.md).

## Licencja

MIT — patrz [LICENSE](LICENSE).
