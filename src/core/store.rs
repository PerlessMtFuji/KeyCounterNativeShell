// SQLite persistence layer.
//
// Single writer thread owns the connection and drains the event
// channel; read-side queries open fresh read-only connections via
// `Store::reader()`. WAL mode means multiple readers don't block each
// other or the writer.
//
// Schema is intentionally close to the Tauri version (key codes match
// 1:1, table names align) so the analytical SQL can be lifted across
// without rewriting. We don't claim cross-version compatibility of
// the DB *file* — different filename, different folder, different
// `meta.schema` version.
//
// Aggregation is in-memory before the SQL upsert: even at 200 KPM the
// 256-event flush window collapses to maybe 30-40 distinct (day, code)
// rows, so we hit each row once per flush instead of once per
// keystroke. That's the dominant write reduction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Timelike, Utc};
use crossbeam_channel::{Receiver, RecvTimeoutError};
use rusqlite::{params, Connection, OpenFlags, Result as SqlResult};
use serde::Serialize;

use crate::core::hook::KeyEvent;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS keystrokes (
    day  TEXT    NOT NULL,
    code INTEGER NOT NULL,
    count INTEGER NOT NULL,
    PRIMARY KEY (day, code)
);

CREATE TABLE IF NOT EXISTS minute_totals (
    minute INTEGER PRIMARY KEY,
    count  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS modifier_usage (
    day      TEXT    NOT NULL,
    modifier TEXT    NOT NULL,
    count    INTEGER NOT NULL,
    PRIMARY KEY (day, modifier)
);

CREATE TABLE IF NOT EXISTS achievements (
    id        TEXT    PRIMARY KEY,
    earned_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_keystrokes_day ON keystrokes(day);
"#;

/// Schema version — bump when the table layout becomes incompatible.
/// At v1 we don't migrate anything; older / unknown numbers should
/// log a warning and proceed (best effort).
pub const SCHEMA_VERSION: i64 = 1;

/// Flush thresholds — whichever fires first.
const FLUSH_EVERY_EVENTS: usize = 256;
const FLUSH_EVERY: Duration = Duration::from_secs(3);

#[derive(Clone)]
pub struct Store {
    db_path: PathBuf,
}

impl Store {
    pub fn open(db_path: impl AsRef<Path>) -> SqlResult<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        if let Some(parent) = db_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(&db_path)?;
        // WAL: lets readers run concurrent with the writer thread.
        // synchronous=NORMAL: the difference between FULL and NORMAL is
        // an extra fsync per commit; under WAL the durability guarantee
        // we get is "no corruption on crash, possibly losing the last
        // few seconds of writes" — perfectly acceptable for a counter.
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.execute_batch(SCHEMA)?;

        // Record / verify schema version. If the user runs a build that
        // somehow ends up looking at a DB from a future schema, we let
        // the queries fail individually rather than refusing to start.
        conn.execute(
            "INSERT INTO meta(key, value) VALUES ('schema_version', ?1)
             ON CONFLICT(key) DO NOTHING",
            params![SCHEMA_VERSION.to_string()],
        )?;

        Ok(Self { db_path })
    }

    pub fn db_path(&self) -> &Path {
        &self.db_path
    }

    /// A fresh read-only connection. Cheap because of WAL — opening
    /// O_RDONLY against a SQLite file under WAL is a few syscalls and
    /// doesn't touch the journal.
    pub fn reader(&self) -> SqlResult<Connection> {
        Connection::open_with_flags(
            &self.db_path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
    }

    /// Spawn the writer thread. Drains `rx` into the database; flushes
    /// on either FLUSH_EVERY_EVENTS or FLUSH_EVERY (whichever first).
    pub fn spawn_writer(&self, rx: Receiver<KeyEvent>) {
        let db_path = self.db_path.clone();
        thread::Builder::new()
            .name("kc-store".into())
            .spawn(move || {
                let mut conn = match Connection::open(&db_path) {
                    Ok(c) => c,
                    Err(e) => {
                        log::error!("store: cannot open db: {e}");
                        return;
                    }
                };
                let _ = conn.execute_batch("PRAGMA journal_mode=WAL;");

                let mut buffer: Vec<KeyEvent> = Vec::with_capacity(FLUSH_EVERY_EVENTS * 2);
                loop {
                    match rx.recv_timeout(FLUSH_EVERY) {
                        Ok(ev) => {
                            buffer.push(ev);
                            if buffer.len() >= FLUSH_EVERY_EVENTS {
                                flush(&mut conn, &mut buffer);
                            }
                        }
                        Err(RecvTimeoutError::Timeout) => {
                            if !buffer.is_empty() {
                                flush(&mut conn, &mut buffer);
                            }
                        }
                        Err(RecvTimeoutError::Disconnected) => {
                            if !buffer.is_empty() {
                                flush(&mut conn, &mut buffer);
                            }
                            log::info!("store: hook channel closed, exiting writer");
                            break;
                        }
                    }
                }
            })
            .expect("failed to spawn kc-store thread");
    }
}

fn flush(conn: &mut Connection, buffer: &mut Vec<KeyEvent>) {
    if buffer.is_empty() {
        return;
    }

    let mut per_day_code: HashMap<(String, i32), i64> = HashMap::new();
    let mut per_minute: HashMap<i64, i64> = HashMap::new();
    let mut per_day_mod: HashMap<(String, &'static str), i64> = HashMap::new();

    for ev in buffer.drain(..) {
        let dt: DateTime<Utc> = match Utc.timestamp_millis_opt(ev.timestamp_ms).single() {
            Some(d) => d,
            None => continue,
        };
        let day = format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day());
        let minute = ev.timestamp_ms / 60_000;

        *per_day_code
            .entry((day.clone(), ev.code.as_i32()))
            .or_insert(0) += 1;
        *per_minute.entry(minute).or_insert(0) += 1;

        if let Some(group) = ev.code.modifier_group() {
            *per_day_mod.entry((day, group.as_str())).or_insert(0) += 1;
        }
    }

    let tx = match conn.transaction() {
        Ok(t) => t,
        Err(e) => {
            log::error!("store: begin tx failed: {e}");
            return;
        }
    };

    {
        let mut stmt = tx
            .prepare_cached(
                "INSERT INTO keystrokes(day, code, count) VALUES (?1, ?2, ?3)
                 ON CONFLICT(day, code) DO UPDATE SET count = count + excluded.count",
            )
            .expect("prepare keystrokes upsert");
        for ((day, code), n) in &per_day_code {
            if let Err(e) = stmt.execute(params![day, code, n]) {
                log::error!("store: keystrokes upsert failed: {e}");
            }
        }

        let mut stmt = tx
            .prepare_cached(
                "INSERT INTO minute_totals(minute, count) VALUES (?1, ?2)
                 ON CONFLICT(minute) DO UPDATE SET count = count + excluded.count",
            )
            .expect("prepare minute upsert");
        for (minute, n) in &per_minute {
            if let Err(e) = stmt.execute(params![minute, n]) {
                log::error!("store: minute upsert failed: {e}");
            }
        }

        let mut stmt = tx
            .prepare_cached(
                "INSERT INTO modifier_usage(day, modifier, count) VALUES (?1, ?2, ?3)
                 ON CONFLICT(day, modifier) DO UPDATE SET count = count + excluded.count",
            )
            .expect("prepare modifier upsert");
        for ((day, group), n) in &per_day_mod {
            if let Err(e) = stmt.execute(params![day, group, n]) {
                log::error!("store: modifier upsert failed: {e}");
            }
        }
    }

    if let Err(e) = tx.commit() {
        log::error!("store: commit failed: {e}");
    }
}

// ---------------------------------------------------------------------------
// Query DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct DayStats {
    pub day: String,
    pub total: i64,
    pub by_code: Vec<KeyCount>,
    pub modifiers: ModifierBreakdown,
}

#[derive(Debug, Clone, Serialize)]
pub struct KeyCount {
    pub code: i32,
    pub count: i64,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ModifierBreakdown {
    pub shift: i64,
    pub ctrl: i64,
    pub alt: i64,
    pub meta: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RangeStats {
    pub from: String,
    pub to: String,
    pub total: i64,
    pub days_active: i64,
    pub by_day: Vec<DayTotal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DayTotal {
    pub day: String,
    pub total: i64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LiveSnapshot {
    pub last_minute: i64,
    pub last_5_minutes: i64,
    pub last_hour: i64,
}

// ---------------------------------------------------------------------------
// Read queries — borrow a Connection so the caller controls lifetime.
// ---------------------------------------------------------------------------

pub fn today() -> String {
    let dt = Utc::now();
    format!("{:04}-{:02}-{:02}", dt.year(), dt.month(), dt.day())
}

pub fn day_stats(conn: &Connection, day: &str) -> SqlResult<DayStats> {
    let total: i64 = conn.query_row(
        "SELECT COALESCE(SUM(count), 0) FROM keystrokes WHERE day = ?1",
        params![day],
        |r| r.get(0),
    )?;

    let mut stmt =
        conn.prepare("SELECT code, count FROM keystrokes WHERE day = ?1 ORDER BY count DESC")?;
    let by_code = stmt
        .query_map(params![day], |r| {
            Ok(KeyCount {
                code: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let mut mods = ModifierBreakdown::default();
    let mut stmt = conn.prepare("SELECT modifier, count FROM modifier_usage WHERE day = ?1")?;
    let rows = stmt.query_map(params![day], |r| {
        let m: String = r.get(0)?;
        let c: i64 = r.get(1)?;
        Ok((m, c))
    })?;
    for row in rows.flatten() {
        match row.0.as_str() {
            "shift" => mods.shift = row.1,
            "ctrl" => mods.ctrl = row.1,
            "alt" => mods.alt = row.1,
            "meta" => mods.meta = row.1,
            _ => {}
        }
    }

    Ok(DayStats {
        day: day.to_string(),
        total,
        by_code,
        modifiers: mods,
    })
}

pub fn range_stats(conn: &Connection, from: &str, to: &str) -> SqlResult<RangeStats> {
    let mut stmt = conn.prepare(
        "SELECT day, SUM(count) FROM keystrokes
         WHERE day BETWEEN ?1 AND ?2
         GROUP BY day ORDER BY day",
    )?;
    let by_day: Vec<DayTotal> = stmt
        .query_map(params![from, to], |r| {
            Ok(DayTotal {
                day: r.get(0)?,
                total: r.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();

    let total: i64 = by_day.iter().map(|d| d.total).sum();
    let days_active = by_day.len() as i64;

    Ok(RangeStats {
        from: from.into(),
        to: to.into(),
        total,
        days_active,
        by_day,
    })
}

pub fn top_keys(conn: &Connection, from: &str, to: &str, limit: i64) -> SqlResult<Vec<KeyCount>> {
    let mut stmt = conn.prepare(
        "SELECT code, SUM(count) AS c FROM keystrokes
         WHERE day BETWEEN ?1 AND ?2
         GROUP BY code ORDER BY c DESC LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![from, to, limit], |r| {
            Ok(KeyCount {
                code: r.get(0)?,
                count: r.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

pub fn live_snapshot(conn: &Connection) -> SqlResult<LiveSnapshot> {
    let now_min = Utc::now().timestamp_millis() / 60_000;
    let last_minute: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(count), 0) FROM minute_totals WHERE minute = ?1",
            params![now_min],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let last_5: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(count), 0) FROM minute_totals WHERE minute > ?1",
            params![now_min - 5],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let last_hour: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(count), 0) FROM minute_totals WHERE minute > ?1",
            params![now_min - 60],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(LiveSnapshot {
        last_minute,
        last_5_minutes: last_5,
        last_hour,
    })
}

pub fn streak_days(conn: &Connection) -> SqlResult<i64> {
    let mut stmt = conn.prepare("SELECT DISTINCT day FROM keystrokes ORDER BY day DESC")?;
    let days: Vec<NaiveDate> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .filter_map(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .collect();

    if days.is_empty() {
        return Ok(0);
    }

    let today = Utc::now().date_naive();
    let mut expected = today;
    let mut count = 0i64;

    if !days.contains(&today) {
        expected = today.pred_opt().unwrap_or(today);
    }

    for day in days {
        if day == expected {
            count += 1;
            expected = match expected.pred_opt() {
                Some(d) => d,
                None => break,
            };
        } else if day < expected {
            break;
        }
    }
    Ok(count)
}

pub fn lifetime_total(conn: &Connection) -> SqlResult<i64> {
    conn.query_row("SELECT COALESCE(SUM(count), 0) FROM keystrokes", [], |r| {
        r.get(0)
    })
}

pub fn today_hourly(conn: &Connection) -> SqlResult<[i64; 24]> {
    let now = Utc::now();
    let day_start = now
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc();
    let start_min = day_start.timestamp() / 60;

    let mut stmt = conn.prepare("SELECT minute, count FROM minute_totals WHERE minute >= ?1")?;
    let rows = stmt.query_map(params![start_min], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;

    let mut hourly = [0i64; 24];
    for row in rows.flatten() {
        let h = ((row.0 - start_min) / 60) as usize;
        if h < 24 {
            hourly[h] += row.1;
        }
    }
    Ok(hourly)
}

pub fn punch_card_30d(conn: &Connection) -> SqlResult<[[i64; 24]; 7]> {
    let now = Utc::now();
    let start = now - chrono::Duration::days(30);
    let start_min = start.timestamp() / 60;

    let mut stmt = conn.prepare("SELECT minute, count FROM minute_totals WHERE minute >= ?1")?;
    let rows = stmt.query_map(params![start_min], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
    })?;

    let mut grid = [[0i64; 24]; 7];
    for (minute_unix, count) in rows.flatten() {
        let ts = minute_unix * 60;
        if let Some(dt) = Utc.timestamp_opt(ts, 0).single() {
            let dow = dt.weekday().num_days_from_sunday() as usize;
            let hour = dt.hour() as usize;
            grid[dow][hour] += count;
        }
    }
    Ok(grid)
}

pub fn calendar(conn: &Connection, days: i64) -> SqlResult<Vec<DayTotal>> {
    let from = (Utc::now() - chrono::Duration::days(days - 1))
        .format("%Y-%m-%d")
        .to_string();
    let mut stmt = conn.prepare(
        "SELECT day, SUM(count) FROM keystrokes
         WHERE day >= ?1
         GROUP BY day ORDER BY day",
    )?;
    let rows = stmt
        .query_map(params![from], |r| {
            Ok(DayTotal {
                day: r.get(0)?,
                total: r.get(1)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

pub fn earned_achievements(conn: &Connection) -> SqlResult<Vec<(String, i64)>> {
    let mut stmt = conn.prepare("SELECT id, earned_at FROM achievements")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

pub fn record_achievement(conn: &Connection, id: &str, earned_at_ms: i64) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO achievements(id, earned_at) VALUES (?1, ?2)
         ON CONFLICT(id) DO NOTHING",
        params![id, earned_at_ms],
    )?;
    Ok(())
}

pub fn reset_all(conn: &mut Connection) -> SqlResult<()> {
    let tx = conn.transaction()?;
    tx.execute_batch(
        "DELETE FROM keystrokes;
         DELETE FROM minute_totals;
         DELETE FROM modifier_usage;
         DELETE FROM achievements;
         DELETE FROM meta;",
    )?;
    tx.commit()
}

pub fn export_json(conn: &Connection) -> SqlResult<serde_json::Value> {
    let mut stmt = conn.prepare("SELECT day, code, count FROM keystrokes")?;
    let keystrokes: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "day": r.get::<_, String>(0)?,
                "code": r.get::<_, i32>(1)?,
                "count": r.get::<_, i64>(2)?,
            }))
        })?
        .filter_map(Result::ok)
        .collect();

    let mut stmt = conn.prepare("SELECT day, modifier, count FROM modifier_usage")?;
    let modifiers: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "day": r.get::<_, String>(0)?,
                "modifier": r.get::<_, String>(1)?,
                "count": r.get::<_, i64>(2)?,
            }))
        })?
        .filter_map(Result::ok)
        .collect();

    let mut stmt = conn.prepare("SELECT id, earned_at FROM achievements")?;
    let achievements: Vec<serde_json::Value> = stmt
        .query_map([], |r| {
            Ok(serde_json::json!({
                "id": r.get::<_, String>(0)?,
                "earned_at_ms": r.get::<_, i64>(1)?,
            }))
        })?
        .filter_map(Result::ok)
        .collect();

    Ok(serde_json::json!({
        "version": SCHEMA_VERSION,
        "exported_at": Utc::now().to_rfc3339(),
        "schema": "keycounter-native",
        "keystrokes": keystrokes,
        "modifiers": modifiers,
        "achievements": achievements,
    }))
}
