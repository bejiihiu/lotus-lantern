//! Typed wrappers for the protocol's magic bytes.
//!
//! The stock `wl.smartled` app builds `mode` / `light_mode` / timing bytes
//! from UI widgets, so the exact value tables were never published
//! (`docs/PROTOCOL.md` says the same). These enums capture what is known or
//! observed, and every one has a `Custom(u8)` escape hatch plus a `*_raw(u8)`
//! method on [`crate::Lamp`] — unknown firmware values keep working, they
//! just travel without a pretty name.

/// Built-in dynamic effect (`CMD 0x03`, `mode + 0x80` on the wire).
///
/// Named variants follow the app enum extracted by the `LotusLampX` project
/// (`Jump`/`Strobe`/`Breathe`/`Warning` = 1..=4); anything else rides
/// [`EffectMode::Custom`]. Values outside 1..=4 are untested on real
/// hardware — start dim if you poke them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum EffectMode {
    /// Color jumping.
    Jump = 1,
    /// Strobe flash.
    Strobe = 2,
    /// Fade in/out.
    Breathe = 3,
    /// Warning flash.
    Warning = 4,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

/// `light_mode` byte of brightness / RGBW frames.
///
/// Exact semantics depend on the controller build (the match arms in
/// `rgbw_status_frame` show modes 0..=4 exist); names are intentionally
/// numeric so nobody reads meaning into them that was never confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum LightMode {
    /// `0` — stock-app default used by every example.
    Mode0 = 0,
    /// `1`.
    Mode1 = 1,
    /// `2` — RGBW path copies the white flag from bit 0 here.
    Mode2 = 2,
    /// `3` — RGBW path copies the white flag from bit 8 here.
    Mode3 = 3,
    /// `4` — same white-flag routing as `3`.
    Mode4 = 4,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

/// External-mic EQ preset (`CMD 0x03`, sub `0x04`, `mode + 0x80`).
///
/// Only meaningful on mic-equipped models; values are uncharted, so the
/// named range is just 0..=2 with [`MicEqMode::Custom`] for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum MicEqMode {
    /// EQ preset `0`.
    Mode0 = 0,
    /// EQ preset `1`.
    Mode1 = 1,
    /// EQ preset `2`.
    Mode2 = 2,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

/// Laser projector state (`CMD 0x05`, sub `0x01`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum LaserState {
    /// Projector off.
    Off = 0,
    /// Projector on.
    On = 1,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

/// Laser effect selector (`CMD 0x03`, sub `0x08`).
///
/// Value table was never published; named range is 0..=2 with
/// [`LaserMode::Custom`] for the rest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum LaserMode {
    /// Laser effect `0`.
    Mode0 = 0,
    /// Laser effect `1`.
    Mode1 = 1,
    /// Laser effect `2`.
    Mode2 = 2,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

/// Timer action for countdown / timing-status frames.
///
/// Best-effort mapping: `0` is what the stock flows send for a plain
/// turn-off timer. Unknown firmware values go through [`TimingMode::Custom`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
#[non_exhaustive]
pub enum TimingMode {
    /// Turn the strip off when the timer fires.
    PowerOff = 0,
    /// Turn the strip on when the timer fires.
    PowerOn = 1,
    /// Any other device value, passed through byte-identical.
    Custom(u8),
}

impl EffectMode {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Jump => 1,
            Self::Strobe => 2,
            Self::Breathe => 3,
            Self::Warning => 4,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Jump,
            2 => Self::Strobe,
            3 => Self::Breathe,
            4 => Self::Warning,
            _ => Self::Custom(v),
        }
    }
}

impl LightMode {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Mode0 => 0,
            Self::Mode1 => 1,
            Self::Mode2 => 2,
            Self::Mode3 => 3,
            Self::Mode4 => 4,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Mode0,
            1 => Self::Mode1,
            2 => Self::Mode2,
            3 => Self::Mode3,
            4 => Self::Mode4,
            _ => Self::Custom(v),
        }
    }
}

impl MicEqMode {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Mode0 => 0,
            Self::Mode1 => 1,
            Self::Mode2 => 2,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Mode0,
            1 => Self::Mode1,
            2 => Self::Mode2,
            _ => Self::Custom(v),
        }
    }
}

impl LaserState {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::On => 1,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Off,
            1 => Self::On,
            _ => Self::Custom(v),
        }
    }
}

impl LaserMode {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::Mode0 => 0,
            Self::Mode1 => 1,
            Self::Mode2 => 2,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::Mode0,
            1 => Self::Mode1,
            2 => Self::Mode2,
            _ => Self::Custom(v),
        }
    }
}

impl TimingMode {
    /// Raw wire value.
    #[inline]
    #[must_use]
    pub const fn as_u8(self) -> u8 {
        match self {
            Self::PowerOff => 0,
            Self::PowerOn => 1,
            Self::Custom(v) => v,
        }
    }

    /// Wrap a raw wire value; known bytes get names, the rest lands in
    /// `Custom` instead of erroring.
    #[inline]
    #[must_use]
    pub const fn from_u8(v: u8) -> Self {
        match v {
            0 => Self::PowerOff,
            1 => Self::PowerOn,
            _ => Self::Custom(v),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_roundtrips() {
        for (v, e) in [
            (1, EffectMode::Jump),
            (2, EffectMode::Strobe),
            (3, EffectMode::Breathe),
            (4, EffectMode::Warning),
        ] {
            assert_eq!(EffectMode::from_u8(v), e);
            assert_eq!(e.as_u8(), v);
        }
        // неизвестное не теряется)
        assert_eq!(EffectMode::from_u8(9).as_u8(), 9);
        assert_eq!(EffectMode::Custom(200).as_u8(), 200);
    }

    #[test]
    fn light_mode_roundtrips() {
        for v in 0..=4u8 {
            assert_eq!(LightMode::from_u8(v).as_u8(), v);
        }
        assert_eq!(LightMode::from_u8(255).as_u8(), 255);
    }

    #[test]
    fn mic_eq_roundtrips() {
        for v in 0..=2u8 {
            assert_eq!(MicEqMode::from_u8(v).as_u8(), v);
        }
        assert_eq!(MicEqMode::from_u8(77).as_u8(), 77);
    }

    #[test]
    fn laser_roundtrips() {
        assert_eq!(LaserState::from_u8(0), LaserState::Off);
        assert_eq!(LaserState::from_u8(1), LaserState::On);
        assert_eq!(LaserState::from_u8(9).as_u8(), 9);
        for v in 0..=2u8 {
            assert_eq!(LaserMode::from_u8(v).as_u8(), v);
        }
        assert_eq!(LaserMode::from_u8(250).as_u8(), 250);
    }

    #[test]
    fn timing_roundtrips() {
        assert_eq!(TimingMode::from_u8(0), TimingMode::PowerOff);
        assert_eq!(TimingMode::from_u8(1), TimingMode::PowerOn);
        assert_eq!(TimingMode::from_u8(13).as_u8(), 13);
    }
}
