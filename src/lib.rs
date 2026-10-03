//! Async Rust client for **Lotus Lantern** / **BLEDOM** / **ELK-BLEDOM**
//! LED-strip controllers.
//!
//! Port of [`lotuslantern-go`](https://github.com/Rxflex/LotusLantern)
//! (reverse-engineered from the stock `wl.smartled` Android app).
//! No phone, no app, no cloud — just BLE GATT writes.
//!
//! ```no_run
//! use std::time::Duration;
//! use lotus_lantern::{Ble, EffectMode, Lamp, LightMode};
//!
//! # async fn demo() -> anyhow::Result<()> {
//! let ble = Ble::new().await?;
//! let found = ble.discover(Duration::from_secs(8)).await?;
//! let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;
//! lamp.light_on(true).await?;
//! lamp.set_color_rgb(255, 0, 128).await?;
//! lamp.set_brightness(60, LightMode::Mode0).await?;
//! lamp.set_effect(EffectMode::Breathe, 128).await?;
//! lamp.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Layout
//!
//! * Frame builders ([`brightness_frame`], …) — pure 9-byte builders
//!   (`const fn`, zero-alloc).
//! * [`encrypt_into`] — XOR cipher for `ELK-*` devices, zero-copy.
//! * Color helpers re-exported as [`hsv_to_rgb`] / [`brightness_steps`]
//!   for the `set_hsv` / `fade_brightness` methods;
//!   [`countdown_delay`] ports `Utils.getTimeStamp`.
//! * Typed enums for modes ([`EffectMode`], [`LightMode`], mic EQ, laser,
//!   [`TimingMode`]) with `Custom(u8)` / `*_raw` escape hatches.
//! * [`TimingInfo`] — `0x85` timing-readback parser.
//! * [`Ble`] / [`Lamp`] — async btleplug transport with BLEDOM reconnect quirks.
//!   [`Lamp::set_effect`] combines mode + speed, [`Lamp::connect_with_options`]
//!   tunes timeouts/retries, [`Ble::scan_sorted`] sorts by RSSI.
//!
//! ## Performance notes
//!
//! BLE is I/O-bound: throughput is the radio, not the CPU. The crate keeps
//! the CPU side boring on purpose — fixed-size arrays, no per-write
//! allocation, event-driven scans (`tokio::time::sleep`, no busy loops).

mod ble;
mod color;
mod consts;
mod encryption;
mod error;
mod frame;
mod lamp;
mod model;
mod timing;

pub use ble::{Ble, DiscoveredLamp, ScanOptions};
pub use color::{blend_brightness, brightness_steps, hsv_to_rgb, music_react_rgb};
pub use consts::{
    is_encrypted_device, is_supported_name, ENCRYPTION_MARKER, NAME_FILTER, NAME_LED_LIGHT_STRIP,
    NAME_NEW_STRENGTH, NAME_PREFIXES, NAME_WAVY_FILTER, SERVICE_UUID, WRITE_CHAR_UUID,
};
pub use encryption::{
    encrypt_into, encrypt_with_random, header_swapped, is_encryptable_cmd, PRESET_KEY,
};
pub use error::{Error, Result};
pub use frame::{
    brightness_frame, color_frame, color_rgb_frame, color_temperature_frame, countdown_frame,
    is_well_formed, laser_frame, laser_mode_frame, laser_speed_frame, light_on_frame,
    mic_eq_mode_frame, mic_on_off_frame, mic_sensitive_frame, mode_frame, mode_speed_frame,
    music_amplitude_frame, pin_sequence_frame, rgbw_status_frame, single_color_frame,
    system_time_frame, timing_read_request_frame, timing_status_frame, Frame, COMMAND_BRIGHTNESS,
    COMMAND_LASER_SPEED, COMMAND_MIC, COMMAND_MODE, COMMAND_PIN_ORDER, COMMAND_POWER_RGBW,
    COMMAND_RGB, COMMAND_SYSTEM_TIME, COMMAND_TIMING, COMMAND_TIMING_READBACK,
    COMMAND_TIMING_STATUS, DEFAULT_PIN_SEQUENCE,
};
pub use lamp::{ConnectOptions, Lamp};
pub use model::{EffectMode, LaserMode, LaserState, LightMode, MicEqMode, TimingMode};
pub use timing::{countdown_delay, pack_hour_minute, pack_system_time_now, TimingInfo};
