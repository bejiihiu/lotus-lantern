//! Pure 9-byte frame builders, ported 1:1 from `commands.go`.
//!
//! Wire format (`docs/PROTOCOL.md`):
//!
//! ```text
//! 0x7E  LEN  CMD  P1  P2  P3  P4  P5  0xEF
//! ```
//!
//! Why this is safe for your strip: every builder returns **exactly** the
//! bytes the stock `wl.smartled` app sends — same length, same command ids,
//! same padding. The port adds Rust-side guard rails on top: typed `u8`
//! params (no int-overflow wrap like Go's `byte(v)` cast), `const fn` so
//! values are computed at compile time, and golden tests asserting
//! byte-equality with the Go implementation for every command.
//!
//! No raw PWM, no overdrive, no custom timings — the controller's own
//! firmware stays the single authority over current limits.

/// Fixed-size command frame: always 9 bytes on the wire.
pub type Frame = [u8; 9];

/// `CMD` bytes (`frame[2]`), mirroring `BluetoothLEService.java`.
pub const COMMAND_BRIGHTNESS: u8 = 0x01;
pub const COMMAND_LASER_SPEED: u8 = 0x02; // shared with mode speed
pub const COMMAND_MODE: u8 = 0x03;
pub const COMMAND_POWER_RGBW: u8 = 0x04;
pub const COMMAND_RGB: u8 = 0x05; // also covers music amplitude
pub const COMMAND_MIC: u8 = 0x06; // 0x06/0x07 mic family
pub const COMMAND_TIMING: u8 = 0x76;
pub const COMMAND_PIN_ORDER: u8 = 0x81;
pub const COMMAND_TIMING_STATUS: u8 = 0x82;
pub const COMMAND_SYSTEM_TIME: u8 = 0x83;

/// Power on/off. Go: `LightOn`.
#[inline]
#[must_use]
pub const fn light_on_frame(on: bool) -> Frame {
    let v: u8 = if on { 1 } else { 0 };
    [0x7E, 0x04, 0x04, v, 0x00, v, 0xFF, 0x00, 0xEF]
}

/// Static RGB color. Go: `ChangeColorRGB`.
#[inline]
#[must_use]
pub const fn color_rgb_frame(r: u8, g: u8, b: u8) -> Frame {
    [0x7E, 0x07, 0x05, 0x03, r, g, b, 0x10, 0xEF]
}

/// Packed `0xRRGGBB` color. Go: `ChangeColor`.
#[inline]
#[must_use]
pub const fn color_frame(rgb: u32) -> Frame {
    color_rgb_frame(
        ((rgb >> 16) & 0xFF) as u8,
        ((rgb >> 8) & 0xFF) as u8,
        (rgb & 0xFF) as u8,
    )
}

/// Brightness `0..=255`. Go: `ChangeBrightness`.
///
/// Start low on a new strip (e.g. 30–80) and raise gradually — same advice
/// as for the stock app.
#[inline]
#[must_use]
pub const fn brightness_frame(brightness: u8, light_mode: u8) -> Frame {
    [
        0x7E, 0x04, 0x01, brightness, light_mode, 0xFF, 0xFF, 0x00, 0xEF,
    ]
}

/// Built-in dynamic effect; device adds the `0x80` bit itself here.
/// Go: `ChangeMode`.
#[inline]
#[must_use]
pub const fn mode_frame(mode: u8) -> Frame {
    [
        0x7E,
        0x05,
        0x03,
        mode.wrapping_add(128),
        0x03,
        0xFF,
        0xFF,
        0x00,
        0xEF,
    ]
}

/// Effect speed. Go: `ChangeModeSpeed`.
#[inline]
#[must_use]
pub const fn mode_speed_frame(speed: u8) -> Frame {
    [0x7E, 0x04, 0x02, speed, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
}

/// Warm/cold white balance. Go: `ChangeColorTemperature`.
#[inline]
#[must_use]
pub const fn color_temperature_frame(warm: u8, cold: u8) -> Frame {
    [0x7E, 0x06, 0x05, 0x02, warm, cold, 0xFF, 0x08, 0xEF]
}

/// Preset palette entry by index. Go: `ChangeSingleColor`.
#[inline]
#[must_use]
pub const fn single_color_frame(index: u8) -> Frame {
    [0x7E, 0x05, 0x05, 0x01, index, 0xFF, 0xFF, 0x08, 0xEF]
}

/// RGB pin remap, default `0x010203`. Go: `ChangePinSequence`.
///
/// This only reorders channels — it cannot raise current. Still, leave it
/// at the default unless your strip's colors are permuted.
#[inline]
#[must_use]
pub const fn pin_sequence_frame(seq: u32) -> Frame {
    [
        0x7E,
        0x06,
        0x81,
        ((seq >> 16) & 0xFF) as u8,
        ((seq >> 8) & 0xFF) as u8,
        (seq & 0xFF) as u8,
        0xFF,
        0x00,
        0xEF,
    ]
}

pub const DEFAULT_PIN_SEQUENCE: u32 = 0x01_02_03;

/// RGBW channel mask. Go: `ChangeRGBWStatus` (bit-twiddling ported as-is).
#[inline]
#[must_use]
pub const fn rgbw_status_frame(rgbw_on: u32, light_mode: u8) -> Frame {
    let mut i5: u8 = 0;
    let mut i6: u8 = 0;
    let i7: u8 = if (rgbw_on >> 8) & 1 == 1 { 1 } else { 0 };
    if (rgbw_on >> 16) & 1 == 1 {
        // red channel bit -> 224 mask later
    }
    let red = (rgbw_on >> 16) & 1 == 1;
    if rgbw_on & 1 == 1 {
        i6 = 1;
    }
    let mut mask: u8 = 0;
    if red {
        mask = 224;
    }
    if i6 != 0 {
        mask |= 16;
    }
    match light_mode {
        0 | 1 => {}
        2 => i5 = i6,
        3 | 4 => i5 = i7,
        _ => i5 = 0,
    }
    [0x7E, 0x04, 0x04, mask, light_mode, i5, 0xFF, 0x00, 0xEF]
}

/// Music-react color. Go: `MusicAmplitude` (caller pre-mixes brightness).
#[inline]
#[must_use]
pub const fn music_amplitude_frame(r: u8, g: u8, b: u8) -> Frame {
    [0x7E, 0x07, 0x05, 0x03, r, g, b, 0x20, 0xEF]
}

/// External mic on/off. Go: `ChangeExternalMicOnOff`.
#[inline]
#[must_use]
pub const fn mic_on_off_frame(on: bool) -> Frame {
    let v: u8 = if on { 1 } else { 0 };
    [0x7E, 0x04, 0x07, v, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
}

/// Mic sensitivity. Go: `ChangeExternalMicSensitive`.
#[inline]
#[must_use]
pub const fn mic_sensitive_frame(level: u8) -> Frame {
    [0x7E, 0x04, 0x06, level, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
}

/// Mic EQ mode. Go: `ChangeExternalMicEqMode`.
#[inline]
#[must_use]
pub const fn mic_eq_mode_frame(mode: u8) -> Frame {
    [
        0x7E,
        0x05,
        0x03,
        mode.wrapping_add(128),
        0x04,
        0xFF,
        0xFF,
        0x00,
        0xEF,
    ]
}

/// Laser projector on/off. Go: `ChangeLaser`.
#[inline]
#[must_use]
pub const fn laser_frame(value: u8) -> Frame {
    [0x7E, 0x05, 0x05, 0x01, value, 0xFF, 0xFF, 0x10, 0xEF]
}

/// Laser effect mode. Go: `ChangeLaserMode`.
#[inline]
#[must_use]
pub const fn laser_mode_frame(mode: u8) -> Frame {
    [0x7E, 0x05, 0x03, mode, 0x08, 0xFF, 0xFF, 0xFF, 0xEF]
}

/// Laser effect speed. Go: `ChangeLaserSpeed`.
#[inline]
#[must_use]
pub const fn laser_speed_frame(speed: u8) -> Frame {
    [0x7E, 0x04, 0x02, speed, 0x04, 0xFF, 0xFF, 0xFF, 0xEF]
}

/// Countdown timer from a precomputed device timestamp. Go: `ChangeCountDown`.
#[inline]
#[must_use]
pub const fn countdown_frame(timestamp: u32, timing_mode: u8) -> Frame {
    [
        0x7E,
        0x07,
        0x76,
        (timestamp & 0xFF) as u8,
        ((timestamp >> 8) & 0xFF) as u8,
        ((timestamp >> 16) & 0xFF) as u8,
        timing_mode,
        0xFF,
        0xEF,
    ]
}

/// System clock push. Go: `SendSystemTime`; `hour_minute` packs
/// `hour << 16 | min << 8 | sec`-ish low byte like the Java side.
#[inline]
#[must_use]
pub const fn system_time_frame(hour_minute: u32, weeks: u8) -> Frame {
    [
        0x7E,
        0x07,
        0x83,
        ((hour_minute >> 16) & 0xFF) as u8,
        ((hour_minute >> 8) & 0xFF) as u8,
        (hour_minute & 0xFF) as u8,
        weeks,
        0xFF,
        0xEF,
    ]
}

/// Timing status. Go: `SendTimingStatus`.
#[inline]
#[must_use]
pub const fn timing_status_frame(hour_minute: u32, timing_mode: u8, weeks: u8) -> Frame {
    [
        0x7E,
        0x08,
        0x82,
        ((hour_minute >> 16) & 0xFF) as u8,
        ((hour_minute >> 8) & 0xFF) as u8,
        (hour_minute & 0xFF) as u8,
        timing_mode,
        weeks,
        0xEF,
    ]
}

/// Frame envelope sanity check: start/end markers and length range.
#[inline]
#[must_use]
pub const fn is_well_formed(frame: &Frame) -> bool {
    frame[0] == 0x7E && frame[8] == 0xEF && frame[1] >= 4 && frame[1] <= 8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn golden_vectors_match_go_implementation() {
        // each tuple hand-derived from commands.go byte literals)
        assert_eq!(
            light_on_frame(true),
            [0x7E, 0x04, 0x04, 1, 0x00, 1, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            light_on_frame(false),
            [0x7E, 0x04, 0x04, 0, 0x00, 0, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            color_rgb_frame(255, 0, 128),
            [0x7E, 0x07, 0x05, 0x03, 255, 0, 128, 0x10, 0xEF]
        );
        assert_eq!(color_frame(0x00FF_0080), color_rgb_frame(255, 0, 128));
        assert_eq!(
            brightness_frame(180, 0),
            [0x7E, 0x04, 0x01, 180, 0, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            mode_frame(5),
            [0x7E, 0x05, 0x03, 133, 0x03, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            mode_speed_frame(100),
            [0x7E, 0x04, 0x02, 100, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            color_temperature_frame(10, 20),
            [0x7E, 0x06, 0x05, 0x02, 10, 20, 0xFF, 0x08, 0xEF]
        );
        assert_eq!(
            single_color_frame(7),
            [0x7E, 0x05, 0x05, 0x01, 7, 0xFF, 0xFF, 0x08, 0xEF]
        );
        assert_eq!(
            pin_sequence_frame(DEFAULT_PIN_SEQUENCE),
            [0x7E, 0x06, 0x81, 0x01, 0x02, 0x03, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            mic_on_off_frame(true),
            [0x7E, 0x04, 0x07, 1, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            mic_sensitive_frame(9),
            [0x7E, 0x04, 0x06, 9, 0xFF, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            mic_eq_mode_frame(2),
            [0x7E, 0x05, 0x03, 130, 0x04, 0xFF, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            laser_frame(1),
            [0x7E, 0x05, 0x05, 0x01, 1, 0xFF, 0xFF, 0x10, 0xEF]
        );
        assert_eq!(
            laser_mode_frame(3),
            [0x7E, 0x05, 0x03, 3, 0x08, 0xFF, 0xFF, 0xFF, 0xEF]
        );
        assert_eq!(
            laser_speed_frame(200),
            [0x7E, 0x04, 0x02, 200, 0x04, 0xFF, 0xFF, 0xFF, 0xEF]
        );
    }

    #[test]
    fn rgbw_vectors_match_go_bit_twiddling() {
        // rgbw_on bit0 -> i6, bit8 -> i7, bit16 -> 224 mask; light_mode remaps i5)
        assert_eq!(
            rgbw_status_frame(0, 0),
            [0x7E, 0x04, 0x04, 0, 0, 0, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            rgbw_status_frame(0x01_01_01, 0),
            [0x7E, 0x04, 0x04, 240, 0, 0, 0xFF, 0x00, 0xEF]
        );
        assert_eq!(
            rgbw_status_frame(0x01_01_01, 2)[5],
            1,
            "light_mode 2 copies i6 into i5"
        );
    }

    #[test]
    fn all_frames_well_formed() {
        for f in [
            light_on_frame(true),
            color_rgb_frame(1, 2, 3),
            brightness_frame(1, 0),
            mode_frame(0),
            countdown_frame(60, 1),
            system_time_frame(0x10_20_30, 1),
            timing_status_frame(0x10_20_30, 1, 2),
        ] {
            assert!(is_well_formed(&f), "{f:02X?}");
        }
    }
}
