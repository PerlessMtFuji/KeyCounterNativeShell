// Translations.
//
// Parity with the Tauri version's i18n.ts — same key namespace, same
// English strings. Polish copy is preserved verbatim. We resolve via
// a `match` on the key (codegen produces a jump table) instead of a
// HashMap because the key set is fixed at compile time and lookups
// happen once per visible widget per repaint.

use parking_lot::RwLock;

use crate::core::theme::Theme;
use crate::core::layouts::LayoutId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    En,
    Pl,
}

impl Lang {
    pub fn id(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Pl => "pl",
        }
    }

    pub fn from_id(s: &str) -> Self {
        match s {
            "pl" => Self::Pl,
            _ => Self::En,
        }
    }
}

/// Settings persisted to disk between launches. Tiny INI-style file in
/// `data_dir/settings.ini` — we deliberately don't pull in serde for
/// this so the on-disk format stays human-readable and stable.
#[derive(Debug, Clone)]
pub struct Settings {
    pub lang: Lang,
    pub theme: Theme,
    pub layout: LayoutId,
    pub widget_compact: bool,
    pub widget_snap: bool,
    pub widget_opacity: u8, // 10..=100
    pub autostart: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            lang: Lang::En,
            theme: Theme::Dark,
            layout: LayoutId::Qwerty,
            widget_compact: false,
            widget_snap: false,
            widget_opacity: 60,
            autostart: false,
        }
    }
}

/// Lock-guarded global settings. Set once at startup, mutated only via
/// the Settings view. RwLock is right here: many readers (every UI
/// frame), rare writers.
pub static SETTINGS: RwLock<Settings> = RwLock::new(Settings {
    lang: Lang::En,
    theme: Theme::Dark,
    layout: LayoutId::Qwerty,
    widget_compact: false,
    widget_snap: false,
    widget_opacity: 60,
    autostart: false,
});

pub fn current_lang() -> Lang {
    SETTINGS.read().lang
}

/// Translate a key (with optional {var} substitution).
pub fn t(key: &str) -> &'static str {
    let lang = current_lang();
    lookup(lang, key)
}

pub fn t_var(key: &str, vars: &[(&str, &str)]) -> String {
    let lang = current_lang();
    let mut s = lookup(lang, key).to_string();
    for (k, v) in vars {
        s = s.replace(&format!("{{{k}}}"), v);
    }
    s
}

fn lookup(lang: Lang, key: &str) -> &'static str {
    let val = match lang {
        Lang::En => en(key),
        Lang::Pl => pl(key).or_else(|| en(key)),
    };
    val.unwrap_or(key)
}

// ---------------------------------------------------------------------------
// English source of truth. Keep this list in sync with `pl()` below
// — translations missing from `pl` fall back to English at runtime.
// ---------------------------------------------------------------------------

fn en(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.dashboard" => "Dashboard",
        "nav.heatmap" => "Heatmap",
        "nav.stats" => "Stats",
        "nav.achievements" => "Achievements",
        "nav.settings" => "Settings",
        "nav.widget" => "Floating widget",
        "nav.recording" => "Recording",
        "nav.paused" => "Paused",
        "nav.pause" => "Pause",
        "nav.resume" => "Resume",

        "common.keys" => "keys",
        "common.kpm" => "KPM",
        "common.day" => "day",
        "common.days" => "days",
        "common.perMin" => "/ min",
        "common.perDay" => "/ day",
        "common.avg" => "avg",
        "common.today" => "Today",
        "common.last7" => "7d",
        "common.last30" => "30d",
        "common.all" => "All",
        "common.less" => "less",
        "common.more" => "more",
        "common.totalPresses" => "total presses",
        "common.presses" => "presses",
        "common.keystrokes" => "keystrokes",
        "common.noData" => "No data yet.",
        "common.notEnough" => "Not enough data yet.",
        "common.target" => "target",
        "common.unlocked" => "unlocked",
        "common.of" => "of",
        "common.cancel" => "Cancel",
        "common.confirm" => "Confirm",

        "dashboard.title" => "Dashboard",
        "dashboard.subtitleNormal" =>
            "Snapshot of your typing activity. All numbers stay on this machine.",
        "dashboard.subtitleFresh" =>
            "Type something — your stats appear here in real time.",
        "dashboard.kpmHint" => "Live · last minute",
        "dashboard.streak" => "Streak",
        "dashboard.lifetime" => "Lifetime",
        "dashboard.last30" => "Last 30 days",
        "dashboard.trend7" => "7-day trend",
        "dashboard.top5" => "Top 5 keys · last 30 days",
        "dashboard.byHour" => "Today by hour",

        "heatmap.title" => "Keyboard heatmap",
        "heatmap.subtitle" => "Where your fingers actually go.",
        "heatmap.fingerLoad" => "Finger load · {layout} touch-typing",
        "heatmap.fingerLoadHint" =>
            "Assumes standard finger assignment. Consider remapping if any finger is doing too much work.",
        "heatmap.punchCard" => "When you type · day × hour · last 30 days",
        "heatmap.calendar" => "Activity calendar · last 365 days",

        "stats.title" => "Stats",
        "stats.subtitle" => "Detailed breakdown across all keys and behaviours.",
        "stats.top20" => "Top 20 keys · last 30 days",
        "stats.modifierMix" => "Modifier mix · today",
        "stats.ofAllKeys" => "of all keys",
        "stats.backspaceRatio" => "Backspace ratio · today",
        "stats.backspaceWarmup" => "Type a bit, then come back.",
        "stats.backspaceLow" => "You delete less than most. Confident typist.",
        "stats.backspaceMid" => "Healthy correction rate.",
        "stats.backspaceHigh" => "Consider slowing down for accuracy.",
        "stats.leastUsed" => "Least used (with non-zero count)",

        "ach.title" => "Achievements",
        "ach.firstStepsTitle" => "First Steps",
        "ach.firstStepsDesc" => "Hit 100 keystrokes",
        "ach.warmingUpTitle" => "Warming Up",
        "ach.warmingUpDesc" => "Hit 10,000 keystrokes",
        "ach.cruiseTitle" => "Cruise Control",
        "ach.cruiseDesc" => "Hit 100,000 keystrokes",
        "ach.millionTitle" => "Million Club",
        "ach.millionDesc" => "Hit 1,000,000 keystrokes",
        "ach.deciTitle" => "Decimillion",
        "ach.deciDesc" => "Hit 10,000,000 keystrokes",
        "ach.weeklongTitle" => "Weeklong",
        "ach.weeklongDesc" => "7-day streak",
        "ach.marathonTitle" => "Marathon",
        "ach.marathonDesc" => "30-day streak",

        "settings.title" => "Settings",
        "settings.subtitle" => "Everything stays on this machine.",
        "settings.recording" => "Recording",
        "settings.pauseLabel" => "Pause counting",
        "settings.pauseHint" => "Hook stays installed; events are silently dropped.",
        "settings.autostartLabel" => "Start with system",
        "settings.autostartHint" => "Launch KeyCounter when you log in.",
        "settings.display" => "Display",
        "settings.layoutLabel" => "Keyboard layout",
        "settings.layoutHint" =>
            "Affects how labels are drawn on the heatmap. Counts are physical-position based and never change.",
        "settings.themeLabel" => "Theme",
        "settings.themeDark" => "Dark",
        "settings.themeLight" => "Light",
        "settings.langLabel" => "Language",
        "settings.langEn" => "English",
        "settings.langPl" => "Polski",
        "settings.data" => "Data",
        "settings.dbPath" => "Database location",
        "settings.exportLabel" => "Export",
        "settings.exportHint" => "Save all counters to a JSON file.",
        "settings.exportButton" => "Export…",
        "settings.resetLabel" => "Reset all data",
        "settings.resetHint" => "Wipes the local database. Cannot be undone.",
        "settings.resetButton" => "Reset",
        "settings.confirmReset" => "Confirm reset",
        "settings.about" => "About",
        "settings.version" => "Version",
        "settings.license" => "License",
        "settings.privacy" => "Privacy",
        "settings.privacyValue" => "counts only · no network",

        "toast.milestone" => "Milestone unlocked",
        "toast.keystrokes" => "{n} keystrokes",
        "toast.keepGoing" => "Keep going.",

        "day.mon" => "Mon", "day.tue" => "Tue", "day.wed" => "Wed",
        "day.thu" => "Thu", "day.fri" => "Fri", "day.sat" => "Sat", "day.sun" => "Sun",

        "month.jan" => "Jan", "month.feb" => "Feb", "month.mar" => "Mar",
        "month.apr" => "Apr", "month.may" => "May", "month.jun" => "Jun",
        "month.jul" => "Jul", "month.aug" => "Aug", "month.sep" => "Sep",
        "month.oct" => "Oct", "month.nov" => "Nov", "month.dec" => "Dec",

        "finger.lPinky" => "L pinky",  "finger.lRing" => "L ring",
        "finger.lMiddle" => "L middle", "finger.lIndex" => "L index",
        "finger.lThumb" => "L thumb",
        "finger.rThumb" => "R thumb",
        "finger.rIndex" => "R index",  "finger.rMiddle" => "R middle",
        "finger.rRing" => "R ring",    "finger.rPinky" => "R pinky",

        "widget.kpm" => "KPM",
        "widget.today" => "Today",
        "widget.modeLabel" => "Floating widget style",
        "widget.modeHint" =>
            "Full = card with today total. Compact = customizable pill badge.",
        "widget.modeFull" => "Full",
        "widget.modeCompact" => "Compact",
        "widget.opacityLabel" => "Pill opacity",
        "widget.opacityHint" =>
            "How solid the compact pill looks against your wallpaper.",
        "widget.snapLabel" => "Anchor to taskbar",
        "widget.snapHint" =>
            "Auto-position the widget above the bottom-right corner of the screen each time it opens — looks pinned to the taskbar.",

        "welcome.tag" => "First launch",
        "welcome.title" => "Welcome to KeyCounter",
        "welcome.body1" =>
            "KeyCounter installs a low-level keyboard hook to count keystrokes. The hook only sees that a key was pressed — never which character, sequence, or app you typed it in.",
        "welcome.body2" =>
            "All counts stay in a local SQLite database under %LOCALAPPDATA%\\KeyCounter. The app never opens a network socket.",
        "welcome.continue" => "Got it, start counting",

        _ => return None,
    })
}

// ---------------------------------------------------------------------------
// Polish.
// ---------------------------------------------------------------------------

fn pl(key: &str) -> Option<&'static str> {
    Some(match key {
        "nav.dashboard" => "Pulpit",
        "nav.heatmap" => "Mapa cieplna",
        "nav.stats" => "Statystyki",
        "nav.achievements" => "Osiągnięcia",
        "nav.settings" => "Ustawienia",
        "nav.widget" => "Widżet",
        "nav.recording" => "Rejestruję",
        "nav.paused" => "Wstrzymane",
        "nav.pause" => "Pauza",
        "nav.resume" => "Wznów",

        "common.keys" => "klawiszy",
        "common.kpm" => "KPM",
        "common.day" => "dzień",
        "common.days" => "dni",
        "common.perMin" => "/ min",
        "common.perDay" => "/ dzień",
        "common.avg" => "śr.",
        "common.today" => "Dzisiaj",
        "common.last7" => "7 dni",
        "common.last30" => "30 dni",
        "common.all" => "Całość",
        "common.less" => "mniej",
        "common.more" => "więcej",
        "common.totalPresses" => "naciśnięć łącznie",
        "common.presses" => "naciśnięć",
        "common.keystrokes" => "naciśnięć",
        "common.noData" => "Brak danych.",
        "common.notEnough" => "Za mało danych.",
        "common.target" => "cel",
        "common.unlocked" => "odblokowano",
        "common.of" => "z",
        "common.cancel" => "Anuluj",
        "common.confirm" => "Potwierdź",

        "dashboard.title" => "Pulpit",
        "dashboard.subtitleNormal" =>
            "Migawka Twojej aktywności. Wszystkie liczby zostają na tym komputerze.",
        "dashboard.subtitleFresh" =>
            "Napisz coś — statystyki pojawią się tutaj w czasie rzeczywistym.",
        "dashboard.kpmHint" => "Na żywo · ostatnia minuta",
        "dashboard.streak" => "Seria",
        "dashboard.lifetime" => "Łącznie",
        "dashboard.last30" => "Ostatnie 30 dni",
        "dashboard.trend7" => "Trend 7-dniowy",
        "dashboard.top5" => "Top 5 klawiszy · ostatnie 30 dni",
        "dashboard.byHour" => "Dzisiaj wg godziny",

        "heatmap.title" => "Mapa cieplna klawiatury",
        "heatmap.subtitle" => "Tu naprawdę chodzą Twoje palce.",
        "heatmap.fingerLoad" => "Obciążenie palców · układ {layout}",
        "heatmap.fingerLoadHint" =>
            "Standardowy przydział palców. Jeśli któryś za bardzo pracuje — rozważ remapowanie.",
        "heatmap.punchCard" => "Kiedy piszesz · dzień × godzina · ostatnie 30 dni",
        "heatmap.calendar" => "Kalendarz aktywności · ostatnie 365 dni",

        "stats.title" => "Statystyki",
        "stats.subtitle" => "Szczegółowe rozbicie po klawiszach i nawykach.",
        "stats.top20" => "Top 20 klawiszy · ostatnie 30 dni",
        "stats.modifierMix" => "Modyfikatory · dzisiaj",
        "stats.ofAllKeys" => "wszystkich klawiszy",
        "stats.backspaceRatio" => "Wskaźnik backspace · dzisiaj",
        "stats.backspaceWarmup" => "Napisz trochę i wróć później.",
        "stats.backspaceLow" => "Kasujesz mniej niż większość. Pewny pisarz.",
        "stats.backspaceMid" => "Zdrowy poziom korekt.",
        "stats.backspaceHigh" => "Może warto spowolnić dla lepszej celności.",
        "stats.leastUsed" => "Najrzadziej (z niezerowym licznikiem)",

        "ach.title" => "Osiągnięcia",
        "ach.firstStepsTitle" => "Pierwsze kroki",
        "ach.firstStepsDesc" => "100 naciśnięć",
        "ach.warmingUpTitle" => "Rozgrzewka",
        "ach.warmingUpDesc" => "10 000 naciśnięć",
        "ach.cruiseTitle" => "Tempomat",
        "ach.cruiseDesc" => "100 000 naciśnięć",
        "ach.millionTitle" => "Klub Miliona",
        "ach.millionDesc" => "1 000 000 naciśnięć",
        "ach.deciTitle" => "Dziesięć milionów",
        "ach.deciDesc" => "10 000 000 naciśnięć",
        "ach.weeklongTitle" => "Tydzień z rzędu",
        "ach.weeklongDesc" => "Seria 7 dni",
        "ach.marathonTitle" => "Maraton",
        "ach.marathonDesc" => "Seria 30 dni",

        "settings.title" => "Ustawienia",
        "settings.subtitle" => "Wszystko zostaje na tym komputerze.",
        "settings.recording" => "Rejestrowanie",
        "settings.pauseLabel" => "Wstrzymaj zliczanie",
        "settings.pauseHint" => "Hook pozostaje aktywny; zdarzenia są ignorowane.",
        "settings.autostartLabel" => "Uruchamiaj z systemem",
        "settings.autostartHint" => "Włącz KeyCounter przy logowaniu.",
        "settings.display" => "Wygląd",
        "settings.layoutLabel" => "Układ klawiatury",
        "settings.layoutHint" =>
            "Wpływa tylko na etykiety na mapie cieplnej. Liczniki bazują na fizycznych pozycjach i się nie zmieniają.",
        "settings.themeLabel" => "Motyw",
        "settings.themeDark" => "Ciemny",
        "settings.themeLight" => "Jasny",
        "settings.langLabel" => "Język",
        "settings.langEn" => "English",
        "settings.langPl" => "Polski",
        "settings.data" => "Dane",
        "settings.dbPath" => "Lokalizacja bazy",
        "settings.exportLabel" => "Eksport",
        "settings.exportHint" => "Zapisz liczniki do pliku JSON.",
        "settings.exportButton" => "Eksportuj…",
        "settings.resetLabel" => "Wyczyść wszystkie dane",
        "settings.resetHint" => "Usuwa lokalną bazę. Operacja nieodwracalna.",
        "settings.resetButton" => "Wyczyść",
        "settings.confirmReset" => "Potwierdź wyczyszczenie",
        "settings.about" => "O programie",
        "settings.version" => "Wersja",
        "settings.license" => "Licencja",
        "settings.privacy" => "Prywatność",
        "settings.privacyValue" => "tylko liczniki · bez sieci",

        "toast.milestone" => "Osiągnięcie odblokowane",
        "toast.keystrokes" => "{n} naciśnięć",
        "toast.keepGoing" => "Tak trzymaj.",

        "day.mon" => "Pon", "day.tue" => "Wt", "day.wed" => "Śr",
        "day.thu" => "Czw", "day.fri" => "Pt", "day.sat" => "Sob", "day.sun" => "Nie",

        "month.jan" => "Sty", "month.feb" => "Lut", "month.mar" => "Mar",
        "month.apr" => "Kwi", "month.may" => "Maj", "month.jun" => "Cze",
        "month.jul" => "Lip", "month.aug" => "Sie", "month.sep" => "Wrz",
        "month.oct" => "Paź", "month.nov" => "Lis", "month.dec" => "Gru",

        "finger.lPinky" => "L mały",     "finger.lRing" => "L serdeczny",
        "finger.lMiddle" => "L środkowy", "finger.lIndex" => "L wskazujący",
        "finger.lThumb" => "L kciuk",
        "finger.rThumb" => "P kciuk",
        "finger.rIndex" => "P wskazujący", "finger.rMiddle" => "P środkowy",
        "finger.rRing" => "P serdeczny",   "finger.rPinky" => "P mały",

        "widget.kpm" => "KPM",
        "widget.today" => "Dzisiaj",
        "widget.modeLabel" => "Styl widżetu",
        "widget.modeHint" =>
            "Pełny = karta z dzisiejszym totalem. Kompaktowy = konfigurowalna kapsułka.",
        "widget.modeFull" => "Pełny",
        "widget.modeCompact" => "Kompaktowy",
        "widget.opacityLabel" => "Krycie kapsułki",
        "widget.opacityHint" => "Jak mocno kapsułka odcina się od tapety.",
        "widget.snapLabel" => "Przyklej do paska zadań",
        "widget.snapHint" =>
            "Po każdym pokazaniu widget pozycjonuje się przy prawej krawędzi nad paskiem zadań.",

        "welcome.tag" => "Pierwsze uruchomienie",
        "welcome.title" => "Witaj w KeyCounter",
        "welcome.body1" =>
            "KeyCounter instaluje niskopoziomowy hook klawiatury, by zliczać naciśnięcia. Hook widzi tylko, że klawisz został wciśnięty — nigdy który znak, w jakiej sekwencji ani w jakiej aplikacji.",
        "welcome.body2" =>
            "Wszystkie liczniki trzymane są w lokalnej bazie SQLite w %LOCALAPPDATA%\\KeyCounter. Aplikacja nigdy nie otwiera połączenia sieciowego.",
        "welcome.continue" => "Rozumiem, zaczynamy",

        _ => return None,
    })
}
