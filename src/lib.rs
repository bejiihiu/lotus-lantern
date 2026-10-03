//! Async Rust client for **Lotus Lantern** / **BLEDOM** / **ELK-BLEDOM**
//! LED-strip controllers.
//!
//! Port of [`lotuslantern-go`](https://github.com/Rxflex/LotusLantern)
//! (reverse-engineered from the stock `wl.smartled` Android app).
//! No phone, no app, no cloud — just BLE GATT writes.
//!
//! ```no_run
//! use std::time::Duration;
//! use lotus_lantern::{Ble, Lamp};
//!
//! # async fn demo() -> anyhow::Result<()> {
//! let ble = Ble::new().await?;
//! let found = ble.discover(Duration::from_secs(15)).await?;
//! let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;
//! lamp.light_on(true).await?;
//! lamp.set_color_rgb(255, 0, 128).await?;
//! lamp.set_brightness(180, 0).await?;
//! lamp.set_mode(5).await?;
//! lamp.close().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Layout
//!
//! * [`frame`] — pure 9-byte frame builders (`const fn`, zero-alloc).
//! * [`encryption`] — XOR cipher for `ELK-*` devices, zero-copy.
//! * [`color`] / [`timing`] — ports of `Utils.newColor` / `getTimeStamp`.
//! * [`Ble`] / [`Lamp`] — async btleplug transport with BLEDOM reconnect quirks.
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
mod timing;

pub use ble::{Ble, DiscoveredLamp};
pub use color::blend_brightness;
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
    system_time_frame, timing_status_frame, Frame, COMMAND_BRIGHTNESS, COMMAND_LASER_SPEED,
    COMMAND_MIC, COMMAND_MODE, COMMAND_PIN_ORDER, COMMAND_POWER_RGBW, COMMAND_RGB,
    COMMAND_SYSTEM_TIME, COMMAND_TIMING, COMMAND_TIMING_STATUS, DEFAULT_PIN_SEQUENCE,
};
pub use lamp::Lamp;
pub use timing::{countdown_delay, pack_hour_minute};
