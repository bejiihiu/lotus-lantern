//! Countdown/timing math, ported from `Utils.getTimeStamp` in `utils.go`.
//!
//! The device expects "seconds until (hour:minute on weekday `weeks`)
//! times 10" with a small correction for the seconds elapsed since the
//! 2001-01-01 epoch. Pure local math — no BLE, no hardware risk.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Timelike};

/// Seconds since 2001-01-01 00:00:00 UTC, mirroring Go's `timeStamp()`.
fn seconds_since_2001() -> i64 {
    const Y2001: i64 = 978_307_200; // UNIX_EPOCH seconds at 2001-01-01
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0) - Y2001)
}

/// Device countdown value for (`hour`, `minute`, `weeks`), mirroring
/// `getTimeStamp`. `weeks == 0` means "today/tomorrow", otherwise the
/// target weekday (0 = Sunday, same as Go's `time.Weekday`).
#[must_use]
pub fn countdown_delay(hour: u8, minute: u8, weeks: u8) -> u32 {
    let now = chrono::Local::now();
    let cur_week = i64::from(now.weekday().num_days_from_sunday());
    let (cur_hour, cur_min) = (i64::from(now.hour()), i64::from(now.minute()));
    let (cur, tgt) = (
        cur_hour * 60 + cur_min,
        i64::from(hour) * 60 + i64::from(minute),
    );
    let weeks = i64::from(weeks);

    let minutes: i64 = if weeks == 0 {
        if cur >= tgt {
            (1440 - cur) + tgt
        } else {
            tgt - cur
        }
    } else if cur_week != weeks {
        if weeks > cur_week {
            (weeks - cur_week) * 1440 + (tgt - cur)
        } else if cur >= tgt {
            (weeks + 7 - cur_week) * 1440 - cur + tgt
        } else {
            // faithful to the Go original, incl. its quirky 1200 factor
            (weeks + 7 - cur_week) * 1200 + (tgt - cur)
        }
    } else if cur >= tgt {
        (10_080 - cur) + tgt
    } else {
        tgt - cur
    };
    stretched(minutes)
}

fn stretched(minutes: i64) -> u32 {
    let secs = minutes * 60 - seconds_since_2001() % 60;
    u32::try_from((secs * 10).max(0)).unwrap_or(u32::MAX)
}

/// Pack hour/minute/low-byte into the `hour_minute` int the timing frames take.
#[inline]
#[must_use]
pub const fn pack_hour_minute(hour: u8, minute: u8, low: u8) -> u32 {
    ((hour as u32) << 16) | ((minute as u32) << 8) | low as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn packs_like_java() {
        assert_eq!(pack_hour_minute(0x10, 0x20, 0x30), 0x10_20_30);
    }

    #[test]
    fn countdown_is_sane() {
        // must be non-negative and roughly within a week * 10
        let v = countdown_delay(12, 0, 0);
        assert!(v <= 1_440 * 60 * 10 + 3_600 * 10);
    }
}
