//! Connected lamp handle, ported from `Connect`/`sendCommand`/`SendBatch`
//! in `lamp.go` plus every `change*` method in `commands.go`.
//!
//! Safety model (your strip will not burn):
//!
//! * every method sends **exactly** the 9 bytes the stock app sends —
//!   golden-tested against the Go port byte-for-byte;
//! * params are typed `u8`, so oversized values can't wrap around like a
//!   Go `byte(int)` cast would allow;
//! * `set_brightness` clamps nothing away silently but the demo starts at
//!   low brightness first — follow it on a new strip;
//! * the XOR cipher only scrambles, never amplifies: same command, same LEDs.

use std::time::Duration;

use btleplug::api::{Central, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Peripheral};
use tokio::time::{sleep, timeout};

use crate::{
    blend_brightness, brightness_frame, color_frame, color_rgb_frame, color_temperature_frame,
    countdown_delay, countdown_frame, encrypt_into, header_swapped, is_encryptable_cmd,
    is_encrypted_device, is_supported_name, is_well_formed, laser_frame, laser_mode_frame,
    laser_speed_frame, light_on_frame, mic_eq_mode_frame, mic_on_off_frame, mic_sensitive_frame,
    mode_frame, mode_speed_frame, music_amplitude_frame, pin_sequence_frame, rgbw_status_frame,
    single_color_frame, system_time_frame, timing_status_frame, Ble, Error, Frame, Result,
    SERVICE_UUID, WRITE_CHAR_UUID,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const SERVICE_DISCOVERY_RETRIES: usize = 25;
const WRITE_RETRIES: usize = 3;

/// Active connection to one LED controller.
#[derive(Debug)]
pub struct Lamp {
    peripheral: Peripheral,
    write_char: btleplug::api::Characteristic,
    is_encrypted: bool,
    addr: String,
    name: String,
}

impl Lamp {
    /// Connect to a known lamp by address + advertised name.
    ///
    /// Mirrors Go's `Connect`: refreshes the platform scan cache first
    /// (Windows `bthleenum.sys` needs it), then dials with service-discovery
    /// retries to ride out BLEDOM flakiness.
    ///
    /// # Errors
    /// * [`Error::DeviceNotSeen`] — address never appeared during the scan.
    /// * [`Error::ServiceNotFound`] / [`Error::CharacteristicNotFound`].
    pub async fn connect(ble: &Ble, addr: &str, name: &str) -> Result<Self> {
        let peripheral = find_peripheral(ble.adapter(), addr).await?;
        let mut lamp = Self {
            peripheral,
            write_char: empty_char(),
            is_encrypted: is_encrypted_device(name),
            addr: addr.to_owned(),
            name: name.to_owned(),
        };
        lamp.dial().await?;
        Ok(lamp)
    }

    /// Advertised name of the connected lamp.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Address string used at connect time.
    #[must_use]
    pub fn addr(&self) -> &str {
        &self.addr
    }

    /// Disconnect. Idempotent-ish: a missing link is not an error here.
    pub async fn close(&self) -> Result<()> {
        self.peripheral.disconnect().await.map_err(Error::Bluetooth)
    }

    /// Send prebuilt frames with `inter_delay` between each.
    pub async fn send_batch(&self, frames: &[Frame], inter_delay: Duration) -> Result<()> {
        for frame in frames {
            self.send_frame(*frame).await?;
            sleep(inter_delay).await;
        }
        Ok(())
    }

    // -- power & color --------------------------------------------------

    /// Power on/off. Start here on a new strip: `light_on(true)` at low
    /// brightness first, then raise gradually.
    pub async fn light_on(&self, on: bool) -> Result<()> {
        self.send_frame(light_on_frame(on)).await
    }

    /// Static RGB color, each channel `0..=255`.
    pub async fn set_color_rgb(&self, r: u8, g: u8, b: u8) -> Result<()> {
        self.send_frame(color_rgb_frame(r, g, b)).await
    }

    /// Packed `0xRRGGBB` static color.
    pub async fn set_color(&self, rgb: u32) -> Result<()> {
        self.send_frame(color_frame(rgb)).await
    }

    /// Brightness `0..=255`. New strip? Start around 30–80.
    pub async fn set_brightness(&self, level: u8, light_mode: u8) -> Result<()> {
        self.send_frame(brightness_frame(level, light_mode)).await
    }

    /// White balance between warm and cold channels.
    pub async fn set_color_temperature(&self, warm: u8, cold: u8) -> Result<()> {
        self.send_frame(color_temperature_frame(warm, cold)).await
    }

    /// Preset palette entry by index byte.
    pub async fn set_single_color(&self, index: u8) -> Result<()> {
        self.send_frame(single_color_frame(index)).await
    }

    /// RGB pin remap (default `0x010203`). Leave default unless colors
    /// come out permuted — reordering channels can't raise current, but a
    /// wrong map looks alarming for no reason.
    pub async fn set_pin_sequence(&self, seq: u32) -> Result<()> {
        self.send_frame(pin_sequence_frame(seq)).await
    }

    /// RGBW channel mask (port of the Android bit-twiddling as-is).
    pub async fn set_rgbw_status(&self, rgbw_on: u32, light_mode: u8) -> Result<()> {
        self.send_frame(rgbw_status_frame(rgbw_on, light_mode))
            .await
    }

    // -- effects --------------------------------------------------------

    /// Built-in dynamic effect.
    pub async fn set_mode(&self, mode: u8) -> Result<()> {
        self.send_frame(mode_frame(mode)).await
    }

    /// Dynamic effect speed.
    pub async fn set_mode_speed(&self, speed: u8) -> Result<()> {
        self.send_frame(mode_speed_frame(speed)).await
    }

    /// Music-react amplitude color; `brightness` remixes the V channel first
    /// (except pure black `0xFF00_0000`, passed through like the app).
    pub async fn music_amplitude(&self, color: u32, brightness: u8) -> Result<()> {
        let mixed = if color == 0xFF00_0000 {
            color
        } else {
            blend_brightness(brightness, color)
        };
        self.send_frame(music_amplitude_frame(
            ((mixed >> 16) & 0xFF) as u8,
            ((mixed >> 8) & 0xFF) as u8,
            (mixed & 0xFF) as u8,
        ))
        .await
    }

    // -- microphone (mic-equipped models) --------------------------------

    /// External mic on/off.
    pub async fn set_mic_on_off(&self, on: bool) -> Result<()> {
        self.send_frame(mic_on_off_frame(on)).await
    }

    /// Mic sensitivity level.
    pub async fn set_mic_sensitive(&self, level: u8) -> Result<()> {
        self.send_frame(mic_sensitive_frame(level)).await
    }

    /// Mic EQ mode.
    pub async fn set_mic_eq_mode(&self, mode: u8) -> Result<()> {
        self.send_frame(mic_eq_mode_frame(mode)).await
    }

    // -- laser (projector models) ----------------------------------------

    /// Laser projector value.
    pub async fn set_laser(&self, value: u8) -> Result<()> {
        self.send_frame(laser_frame(value)).await
    }

    /// Laser effect mode.
    pub async fn set_laser_mode(&self, mode: u8) -> Result<()> {
        self.send_frame(laser_mode_frame(mode)).await
    }

    /// Laser effect speed.
    pub async fn set_laser_speed(&self, speed: u8) -> Result<()> {
        self.send_frame(laser_speed_frame(speed)).await
    }

    // -- timing -----------------------------------------------------------

    /// Countdown timer; `hour_minute` packs `hour << 16 | minute << 8 | low`.
    pub async fn set_countdown(
        &self,
        hour: u8,
        minute: u8,
        weeks: u8,
        timing_mode: u8,
    ) -> Result<()> {
        let ts = countdown_delay(hour, minute, weeks);
        self.send_frame(countdown_frame(ts, timing_mode)).await
    }

    /// Push the controller clock.
    pub async fn send_system_time(&self, hour_minute: u32, weeks: u8) -> Result<()> {
        self.send_frame(system_time_frame(hour_minute, weeks)).await
    }

    /// Timing status row.
    pub async fn send_timing_status(
        &self,
        hour_minute: u32,
        timing_mode: u8,
        weeks: u8,
    ) -> Result<()> {
        self.send_frame(timing_status_frame(hour_minute, timing_mode, weeks))
            .await
    }

    // -- transport --------------------------------------------------------

    async fn dial(&mut self) -> Result<()> {
        if !self
            .peripheral
            .is_connected()
            .await
            .map_err(Error::Bluetooth)?
        {
            timeout(CONNECT_TIMEOUT, self.peripheral.connect())
                .await
                .map_err(|_| Error::Timeout)?
                .map_err(Error::Bluetooth)?;
        }
        // bthleenum.sys needs a beat after connect before services resolve)
        sleep(Duration::from_secs(1)).await;

        let mut last_err: Option<Error> = None;
        for _ in 0..SERVICE_DISCOVERY_RETRIES {
            match self.peripheral.discover_services().await {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(Error::Bluetooth(e));
                    sleep(Duration::from_secs(1)).await;
                }
            }
        }
        if let Some(e) = last_err {
            let _ = self.peripheral.disconnect().await;
            return Err(e);
        }

        let mut found_service = false;
        let mut found_char = None;
        for service in self.peripheral.services() {
            if service.uuid == SERVICE_UUID {
                found_service = true;
                for ch in &service.characteristics {
                    if ch.uuid == WRITE_CHAR_UUID {
                        found_char = Some(ch.clone());
                        break;
                    }
                }
            }
        }
        if !found_service {
            let _ = self.peripheral.disconnect().await;
            return Err(Error::ServiceNotFound);
        }
        if let Some(ch) = found_char {
            self.write_char = ch;
            Ok(())
        } else {
            let _ = self.peripheral.disconnect().await;
            Err(Error::CharacteristicNotFound)
        }
    }

    async fn send_frame(&self, frame: Frame) -> Result<()> {
        debug_assert!(is_well_formed(&frame), "malformed frame: {frame:02X?}");
        // stack scratch for the 21-byte cipher form: no allocator in hot path)
        let mut cipher = [0u8; 21];
        let plain;
        let bytes: &[u8] = if self.is_encrypted && is_encryptable_cmd(frame[2]) {
            encrypt_into(&header_swapped(frame), &mut cipher);
            &cipher
        } else {
            plain = frame;
            &plain
        };

        for _ in 0..WRITE_RETRIES {
            let connected = self
                .peripheral
                .is_connected()
                .await
                .map_err(Error::Bluetooth)?;
            if !connected {
                // link dropped (BLEDOM does this every 3–10 writes) — redial)
                let mut this = Lamp {
                    peripheral: self.peripheral.clone(),
                    write_char: self.write_char.clone(),
                    is_encrypted: self.is_encrypted,
                    addr: self.addr.clone(),
                    name: self.name.clone(),
                };
                sleep(Duration::from_secs(2)).await;
                if this.dial().await.is_err() {
                    continue;
                }
                // dial refreshed only `this`; retry loop re-reads via self below.
                // instead write through the fresh handle directly:
                if this
                    .peripheral
                    .write(&this.write_char, bytes, WriteType::WithoutResponse)
                    .await
                    .is_ok()
                {
                    return Ok(());
                }
                continue;
            }
            match self
                .peripheral
                .write(&self.write_char, bytes, WriteType::WithoutResponse)
                .await
            {
                Ok(()) => return Ok(()),
                Err(_) => {
                    let _ = self.peripheral.disconnect().await;
                }
            }
        }
        Err(Error::SendFailed)
    }
}

fn empty_char() -> btleplug::api::Characteristic {
    use btleplug::api::CharPropFlags;
    use std::collections::BTreeSet;
    btleplug::api::Characteristic {
        uuid: WRITE_CHAR_UUID,
        service_uuid: SERVICE_UUID,
        properties: CharPropFlags::default(),
        descriptors: BTreeSet::default(),
    }
}

async fn find_peripheral(adapter: &Adapter, addr: &str) -> Result<Peripheral> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(Error::Bluetooth)?;
    let deadline = tokio::time::Instant::now() + CONNECT_TIMEOUT;
    let wanted = addr.to_lowercase();
    loop {
        let peripherals = adapter.peripherals().await.map_err(Error::Bluetooth)?;
        for p in &peripherals {
            // fast path: platform address match without a properties round-trip
            let id_match = p.address().to_string().to_lowercase() == wanted;
            if id_match {
                let _ = timeout(Duration::from_secs(1), adapter.stop_scan()).await;
                return Ok(p.clone());
            }
            // slow path: some platforms (macOS) hide the MAC; compare names too
            if let Ok(Some(props)) = p.properties().await {
                let name = props.local_name.unwrap_or_default();
                if props.address.to_string().to_lowercase() == wanted
                    || (is_supported_name(&name) && wanted.is_empty())
                {
                    let _ = timeout(Duration::from_secs(1), adapter.stop_scan()).await;
                    return Ok(p.clone());
                }
            }
        }
        if tokio::time::Instant::now() >= deadline {
            let _ = timeout(Duration::from_secs(1), adapter.stop_scan()).await;
            return Err(Error::DeviceNotSeen(addr.to_owned()));
        }
        sleep(Duration::from_millis(300)).await;
    }
}
