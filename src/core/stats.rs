// In-memory snapshot of everything the UI views need.
//
// The store thread refreshes a single `DataSnapshot` every ~5 s while
// the main window is visible; the UI reads it under a parking_lot
// RwLock and renders. Coalescing to one snapshot (instead of four
// independent state slices, like the React store did) means any single
// repaint sees a consistent point-in-time view — no chance of
// "yesterday's total + today's by-hour" frame mismatch.

use chrono::Duration as ChronoDuration;
use chrono::Utc;
use rusqlite::Connection;

use crate::core::store::{
    self, DayStats, DayTotal, KeyCount, LiveSnapshot, RangeStats,
};

#[derive(Debug, Default, Clone)]
pub struct DataSnapshot {
    pub today: Option<DayStats>,
    pub range7: Option<RangeStats>,
    pub range30: Option<RangeStats>,
    pub top_30d: Vec<KeyCount>,
    pub hourly: [i64; 24],
    pub punch_card: [[i64; 24]; 7],
    pub streak: i64,
    pub lifetime: i64,
    pub live: LiveSnapshot,
    pub calendar_365: Vec<DayTotal>,
    pub earned_achievements: Vec<(String, i64)>,
}

impl DataSnapshot {
    /// Re-fetch every slice the UI needs from the database. Call site
    /// is the periodic 5 s timer on the UI thread; the actual SQL runs
    /// on the same thread (queries are quick — single-digit
    /// milliseconds even at 100 k rows). The previous design ran
    /// queries on a background thread and posted results back, which
    /// added latency and complicated the cancel-on-window-hide path.
    pub fn refresh_full(conn: &Connection) -> Self {
        let today_str = store::today();
        let day7 = (Utc::now() - ChronoDuration::days(6))
            .format("%Y-%m-%d")
            .to_string();
        let day30 = (Utc::now() - ChronoDuration::days(29))
            .format("%Y-%m-%d")
            .to_string();

        DataSnapshot {
            today: store::day_stats(conn, &today_str).ok(),
            range7: store::range_stats(conn, &day7, &today_str).ok(),
            range30: store::range_stats(conn, &day30, &today_str).ok(),
            top_30d: store::top_keys(conn, &day30, &today_str, 30).unwrap_or_default(),
            hourly: store::today_hourly(conn).unwrap_or([0; 24]),
            punch_card: store::punch_card_30d(conn).unwrap_or([[0; 24]; 7]),
            streak: store::streak_days(conn).unwrap_or(0),
            lifetime: store::lifetime_total(conn).unwrap_or(0),
            live: store::live_snapshot(conn).unwrap_or_default(),
            calendar_365: store::calendar(conn, 365).unwrap_or_default(),
            earned_achievements: store::earned_achievements(conn).unwrap_or_default(),
        }
    }

    /// Lighter refresh used by the 500 ms tick: only the live KPM /
    /// last-minute window. Avoids re-running the heavier aggregations
    /// (calendar, top keys) every half-second.
    pub fn refresh_live(&mut self, conn: &Connection) {
        if let Ok(live) = store::live_snapshot(conn) {
            self.live = live;
        }
    }
}

/// 60-second sliding window of keystroke deltas → instant client-side
/// KPM. Mirrors the recordPulse logic in the original Tauri store.
#[derive(Debug, Default)]
pub struct PulseHistory {
    samples: Vec<(i64, i64)>, // (timestamp_ms, delta)
}

impl PulseHistory {
    pub fn record(&mut self, now_ms: i64, delta: i64) {
        // Drop samples older than 60 s — they're outside the sliding
        // window. We reconstruct a clean Vec rather than draining in
        // place because the typical sample count (≤ 200 entries) makes
        // this trivially cheaper than the index bookkeeping.
        self.samples.retain(|(ts, _)| now_ms - *ts < 60_000);
        self.samples.push((now_ms, delta));
    }

    pub fn kpm(&self) -> i64 {
        self.samples.iter().map(|(_, d)| *d).sum()
    }

    pub fn clear(&mut self) {
        self.samples.clear();
    }
}
