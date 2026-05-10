// 365-day GitHub-style activity calendar.
//
// Cells are 7-row columns of days, each cell 12×12 DIPs by default. We
// align column 0 to "today's column" so today is always rightmost,
// consistent with how the Tauri version drew it. Months get a thin
// label row at the top whenever the column boundary lands on the
// first week of a new month.

use chrono::{Datelike, Duration as ChronoDuration, NaiveDate, Utc, Weekday};

use super::primitives::{fill_rounded, text, HAlign, Rect, VAlign};
use super::{heat, Brush, Font, RenderContext};
use crate::core::store::DayTotal;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;

/// Layout-aware draw — the calendar fits whatever rect it's given,
/// stepping the cell size down if needed. Returns the pixel width
/// actually used (caller can right-align if it wants).
pub fn draw(ctx: &RenderContext, rect: Rect, days: &[DayTotal]) -> f32 {
    if days.is_empty() {
        text(
            ctx,
            rect,
            "—",
            Font::Body,
            Brush::TextDim,
            HAlign::Centre,
            VAlign::Centre,
        );
        return rect.w;
    }

    // Index by date for O(1) lookup. The DB returns days sorted asc.
    let by_day: std::collections::HashMap<String, i64> =
        days.iter().map(|d| (d.day.clone(), d.total)).collect();

    let today = Utc::now().date_naive();
    let total_days = 365usize;
    // Calendar starts on the Monday before (today - 364) so columns
    // are full weeks. We back up to the most recent Monday on/before
    // the start day.
    let raw_start = today - ChronoDuration::days((total_days - 1) as i64);
    let weekday_offset = match raw_start.weekday() {
        Weekday::Mon => 0,
        Weekday::Tue => 1,
        Weekday::Wed => 2,
        Weekday::Thu => 3,
        Weekday::Fri => 4,
        Weekday::Sat => 5,
        Weekday::Sun => 6,
    };
    let start = raw_start - ChronoDuration::days(weekday_offset as i64);
    let span = (today - start).num_days() as usize + 1;
    let columns = (span + 6) / 7;

    let label_w = 22.0;
    let month_h = 14.0;
    let inner_w = rect.w - label_w;
    let avail_h = rect.h - month_h;
    let cell = ((inner_w / columns as f32).min(avail_h / 7.0) - 2.0).max(6.0);
    let gap = (cell * 0.18).max(1.0);
    let used_w = label_w + columns as f32 * (cell + gap);

    let max = by_day.values().copied().max().unwrap_or(0).max(1);
    let palette = ctx.palette();

    // Day-of-week labels (Mon, Wed, Fri only — keeps the strip uncluttered).
    for (row, lbl) in [(0, "Mon"), (2, "Wed"), (4, "Fri")] {
        let y = rect.y + month_h + row as f32 * (cell + gap);
        text(
            ctx,
            Rect::new(rect.x, y, label_w - 4.0, cell),
            lbl,
            Font::Caption,
            Brush::TextMuted,
            HAlign::Trailing,
            VAlign::Centre,
        );
    }

    let mut last_month: Option<u32> = None;
    for col in 0..columns {
        let col_x = rect.x + label_w + col as f32 * (cell + gap);
        for row in 0..7 {
            let day_index = col * 7 + row;
            if day_index >= span {
                break;
            }
            let date = start + ChronoDuration::days(day_index as i64);
            if date > today {
                continue;
            }
            let count = by_day.get(&date.format("%Y-%m-%d").to_string()).copied().unwrap_or(0);
            let t = if count == 0 {
                0.0
            } else {
                ((count as f32 / max as f32).powf(0.5)).clamp(0.08, 1.0)
            };
            let rgba = if count == 0 { palette.surface_alt } else { heat(palette, t) };
            let cell_rect = Rect::new(
                col_x,
                rect.y + month_h + row as f32 * (cell + gap),
                cell,
                cell,
            );
            draw_cell(ctx, cell_rect, rgba);
        }

        // Month label — print at the column where a new month starts.
        let first_in_col_date = start + ChronoDuration::days((col * 7) as i64);
        let m = first_in_col_date.month();
        if last_month != Some(m) && first_in_col_date <= today {
            text(
                ctx,
                Rect::new(col_x, rect.y, cell * 4.0, month_h),
                month_short(m),
                Font::Caption,
                Brush::TextMuted,
                HAlign::Leading,
                VAlign::Centre,
            );
            last_month = Some(m);
        }
    }
    used_w
}

fn draw_cell(ctx: &RenderContext, rect: Rect, rgba: [f32; 4]) {
    let color = D2D1_COLOR_F {
        r: rgba[0],
        g: rgba[1],
        b: rgba[2],
        a: rgba[3],
    };
    if let Ok(brush) = unsafe { ctx.target.CreateSolidColorBrush(&color, None) } {
        let rr = windows::Win32::Graphics::Direct2D::D2D1_ROUNDED_RECT {
            rect: rect.to_d2d(),
            radiusX: 2.0,
            radiusY: 2.0,
        };
        unsafe {
            ctx.target.FillRoundedRectangle(&rr, &brush);
        }
    } else {
        // Cheap fallback — should never hit unless the device is lost
        // mid-paint, in which case D2D will retry the whole frame.
        fill_rounded(ctx, rect, 2.0, Brush::SurfaceAlt);
    }
}

fn month_short(m: u32) -> &'static str {
    // English short names — i18n mapping is the caller's responsibility
    // when the calendar legend changes (Push 3). For now this matches
    // the React UI's default behaviour.
    [
        "", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][m as usize]
}

/// Standalone helper for the upcoming streak / lifetime counters that
/// want to highlight a specific date. Not used by the calendar itself
/// but lives here so the date math stays in one module.
#[allow(dead_code)]
pub fn day_label(day: NaiveDate) -> String {
    day.format("%a, %b %-d").to_string()
}
