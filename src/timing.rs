//! Countdown/timing math, ported from `Utils.getTimeStamp` in `utils.go`.
//!
//! The device expects "seconds until (hour:minute on weekday `weeks`)
//! times 10" with a small correction for the seconds elapsed since the
//! 2001-01-01 epoch. Pure local math — no BLE, no hardware risk.

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Datelike, Timelike};

use crate::{Error, Result, COMMAND_TIMING_READBACK};

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

/// Current local time packed for [`crate::frame::system_time_frame`].
///
/// Returns `(hour_minute, weeks)`: hour/minute/second from
/// [`chrono::Local`], `weeks` as the weekday index (0 = Sunday, same
/// convention as [`countdown_delay`]).
#[must_use]
pub fn pack_system_time_now() -> (u32, u8) {
    let now = chrono::Local::now();
    // hour/min/sec всегда влезают в u8, но try_from честнее чем as)
    let h = u8::try_from(now.hour()).unwrap_or(23);
    let m = u8::try_from(now.minute()).unwrap_or(59);
    let s = u8::try_from(now.second()).unwrap_or(59);
    let w = u8::try_from(now.weekday().num_days_from_sunday()).unwrap_or(0);
    (pack_hour_minute(h, m, s), w)
}

/// Timing-info readback (`CMD 0x85`).
///
/// Two shapes exist in the wild: the documented `7E 09 85 b0..b5` row (no
/// `EF` terminator) and — observed on an `ELK-BLEDDM` clone — a verbatim
/// echo of the request frame (`7E 04 85 … EF`). The parser accepts both
/// 9-byte forms; per-byte semantics were never published, so the struct
/// keeps the raw slots. Note: on that same clone a freshly programmed timer
/// is broadcast unsolicited as a `0x76` frame (same layout as the
/// countdown write), not as a `0x85` reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimingInfo {
    /// Raw `b0..b5` payload bytes.
    pub slots: [u8; 6],
}

impl TimingInfo {
    /// Parse a `7E 09 85 b0..b5` notification payload.
    ///
    /// ```no_run
    /// use lotus_lantern::TimingInfo;
    ///
    /// # fn demo(raw: &[u8]) -> lotus_lantern::Result<()> {
    /// let info = TimingInfo::parse(raw)?;
    /// println!("ts={} mode={} weeks={}", info.timestamp(), info.timing_mode(), info.weeks());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::InvalidParam`] when the buffer is not a `0x85`
    /// readback (wrong length, markers, or command byte).
    pub fn parse(value: &[u8]) -> Result<Self> {
        if value.len() != 9 {
            return Err(Error::InvalidParam(format!(
                "0x85 reply must be 9 bytes, got {}",
                value.len()
            )));
        }
        if value[0] != 0x7E {
            return Err(Error::InvalidParam(format!(
                "0x85 reply must start with 0x7E, got {:#04X}",
                value[0]
            )));
        }
        if value[2] != COMMAND_TIMING_READBACK {
            return Err(Error::InvalidParam(format!(
                "not a 0x85 reply (frame[2] = {:#04X})",
                value[2]
            )));
        }
        let mut slots = [0u8; 6];
        slots.copy_from_slice(&value[3..9]);
        Ok(Self { slots })
    }

    /// First three slots as the little-endian device timestamp
    /// (same packing as [`countdown_delay`]).
    #[inline]
    #[must_use]
    pub const fn timestamp(&self) -> u32 {
        (self.slots[0] as u32) | ((self.slots[1] as u32) << 8) | ((self.slots[2] as u32) << 16)
    }

    /// Fourth slot — timing mode byte, compare with [`crate::TimingMode`].
    #[inline]
    #[must_use]
    pub const fn timing_mode(&self) -> u8 {
        self.slots[3]
    }

    /// Fifth slot — weeks/weekday byte.
    #[inline]
    #[must_use]
    pub const fn weeks(&self) -> u8 {
        self.slots[4]
    }
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

    #[test]
    fn timing_info_parses_valid_reply() {
        let info = TimingInfo::parse(&[0x7E, 0x09, 0x85, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06])
            .expect("valid 0x85 reply");
        assert_eq!(info.slots, [1, 2, 3, 4, 5, 6]);
        assert_eq!(info.timestamp(), 0x03_02_01);
        assert_eq!(info.timing_mode(), 4);
        assert_eq!(info.weeks(), 5);
    }

    #[test]
    fn timing_info_rejects_garbage() {
        // не та длина)
        assert!(TimingInfo::parse(&[0x7E, 0x09, 0x85]).is_err());
        // не тот старт)
        assert!(TimingInfo::parse(&[0x00, 0x09, 0x85, 1, 2, 3, 4, 5, 6]).is_err());
        // не та команда — обычный ответ 0x82 не должен пролезть)
        assert!(TimingInfo::parse(&[0x7E, 0x08, 0x82, 1, 2, 3, 4, 5, 0xEF]).is_err());
    }

    #[test]
    fn system_time_now_is_sane() {
        let (hm, weeks) = pack_system_time_now();
        assert!((hm >> 16) & 0xFF <= 23, "hour fits a day");
        assert!((hm >> 8) & 0xFF <= 59, "minute fits an hour");
        assert!(hm & 0xFF <= 59, "low byte is seconds");
        assert!(weeks <= 6, "weekday index");
    }
}
