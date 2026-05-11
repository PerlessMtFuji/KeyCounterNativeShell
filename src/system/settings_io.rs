// Settings persistence — INI-style key/value file at
// %LOCALAPPDATA%\KeyCounter\settings.ini.
//
// We hand-roll the format instead of pulling in serde_ini for two
// reasons: (1) the field set is tiny (8 keys today, ≤ 20 ever);
// (2) keeping the file human-readable and forward-compatible matters
// more than schema rigour — a future field just appears in the file,
// missing fields fall back to defaults, unknown fields are ignored.
//
// Format:
//   lang=pl
//   theme=dark
//   layout=qwerty
//   widget_compact=false
//   widget_snap=true
//   widget_opacity=60
//   autostart=false
//
// Comments (#-prefixed) and blank lines are skipped. The first `=` on
// each line is the separator; values may contain `=` after that.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::core::i18n::{Lang, Settings};
use crate::core::layouts::LayoutId;
use crate::core::theme::Theme;
use crate::system::paths;

pub const FILENAME: &str = "settings.ini";

pub fn path() -> PathBuf {
    paths::data_dir().join(FILENAME)
}

pub fn load() -> Settings {
    let p = path();
    let raw = match fs::read_to_string(&p) {
        Ok(s) => s,
        Err(_) => return Settings::default(),
    };

    let mut s = Settings::default();
    for line in raw.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = match line.split_once('=') {
            Some(pair) => pair,
            None => continue,
        };
        let k = k.trim();
        let v = v.trim();
        match k {
            "lang" => s.lang = Lang::from_id(v),
            "theme" => s.theme = Theme::from_id(v),
            "layout" => s.layout = LayoutId::from_id(v),
            "widget_visible" => s.widget_visible = parse_bool(v),
            "widget_compact" => s.widget_compact = parse_bool(v),
            "widget_snap" => s.widget_snap = parse_bool(v),
            "widget_opacity" => {
                if let Ok(n) = v.parse::<u32>() {
                    s.widget_opacity = n.clamp(10, 100) as u8;
                }
            }
            "autostart" => s.autostart = parse_bool(v),
            _ => {} // ignore unknown — forward compat
        }
    }
    s
}

pub fn save(s: &Settings) -> std::io::Result<()> {
    let p = path();
    if let Some(parent) = p.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut f = fs::File::create(&p)?;
    writeln!(f, "# KeyCounter settings — human-editable.")?;
    writeln!(f, "lang={}", s.lang.id())?;
    writeln!(f, "theme={}", s.theme.id())?;
    writeln!(f, "layout={}", s.layout.id())?;
    writeln!(f, "widget_visible={}", s.widget_visible)?;
    writeln!(f, "widget_compact={}", s.widget_compact)?;
    writeln!(f, "widget_snap={}", s.widget_snap)?;
    writeln!(f, "widget_opacity={}", s.widget_opacity)?;
    writeln!(f, "autostart={}", s.autostart)?;
    Ok(())
}

fn parse_bool(s: &str) -> bool {
    matches!(
        s.to_ascii_lowercase().as_str(),
        "true" | "1" | "yes" | "on"
    )
}
