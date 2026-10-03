//! Connected lamp handle, ported from `Connect`/`sendCommand`/`SendBatch`
//! in `lamp.go` plus every `change*` method in `commands.go`.
//!
//! Safety model (your strip will not burn):
//!
//! * every method sends **exactly** the 9 bytes the stock app sends —
//!   golden-tested against the Go port byte-for-byte;
//! * typed enums ([`EffectMode`], [`LightMode`], …) replace magic `u8`s at
//!   the public API; `*_raw` methods still take plain bytes for unknown
//!   firmware values;
//! * `set_brightness` clamps nothing away silently but the demo starts at
//!   low brightness first — follow it on a new strip;
//! * the XOR cipher only scrambles, never amplifies: same command, same LEDs.

use std::time::Duration;

use btleplug::api::{Central, CharPropFlags, Peripheral as _, ScanFilter, WriteType};
use btleplug::platform::{Adapter, Peripheral};
use futures::StreamExt as _;
use tokio::time::{sleep, timeout};

use crate::{
    brightness_frame, brightness_steps, color_frame, color_rgb_frame, color_temperature_frame,
    countdown_delay, countdown_frame, encrypt_into, header_swapped, hsv_to_rgb, is_encryptable_cmd,
    is_encrypted_device, is_supported_name, is_well_formed, laser_frame, laser_mode_frame,
    laser_speed_frame, light_on_frame, mic_eq_mode_frame, mic_on_off_frame, mic_sensitive_frame,
    mode_frame, mode_speed_frame, music_amplitude_frame, music_react_rgb, pack_system_time_now,
    pin_sequence_frame, rgbw_status_frame, single_color_frame, system_time_frame,
    timing_read_request_frame, timing_status_frame, Ble, EffectMode, Error, Frame, LaserMode,
    LaserState, LightMode, MicEqMode, Result, TimingInfo, TimingMode, SERVICE_UUID,
    WRITE_CHAR_UUID,
};

/// Tuning knobs for [`Lamp::connect_with_options`].
///
/// Defaults reproduce the original hardcoded behavior, so plain
/// [`Lamp::connect`] acts exactly like 0.1.0 did.
///
/// ```no_run
/// use std::time::Duration;
/// use lotus_lantern::{Ble, ConnectOptions, Lamp};
///
/// # async fn demo(ble: &Ble, addr: &str, name: &str) -> lotus_lantern::Result<()> {
/// let opts = ConnectOptions {
///     timeout: Duration::from_secs(10),
///     ..ConnectOptions::default()
/// };
/// let lamp = Lamp::connect_with_options(ble, addr, name, opts).await?;
/// lamp.close().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct ConnectOptions {
    /// Deadline for BLE connect + each scan sweep.
    pub timeout: Duration,
    /// How many times service discovery is retried (BLEDOM flakes).
    pub discovery_retries: usize,
    /// How many times a frame write is retried (link drops every 3–10 writes).
    pub write_retries: usize,
    /// Pause after connect before service discovery (`bthleenum.sys` needs it).
    pub post_connect_delay: Duration,
    /// Pause between service-discovery retries.
    pub discovery_retry_delay: Duration,
    /// Pause before redial when the link drops mid-write.
    pub reconnect_delay: Duration,
    /// How long [`Lamp::read_timing_info`] waits for the `0x85` reply.
    pub notification_timeout: Duration,
}

impl Default for ConnectOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(15),
            discovery_retries: 25,
            write_retries: 3,
            post_connect_delay: Duration::from_secs(1),
            discovery_retry_delay: Duration::from_secs(1),
            reconnect_delay: Duration::from_secs(2),
            notification_timeout: Duration::from_secs(5),
        }
    }
}

/// Active connection to one LED controller.
#[derive(Debug)]
pub struct Lamp {
    peripheral: Peripheral,
    write_char: btleplug::api::Characteristic,
    notify_char: Option<btleplug::api::Characteristic>,
    is_encrypted: bool,
    addr: String,
    name: String,
    opts: ConnectOptions,
}

impl Lamp {
    /// Connect to a known lamp by address + advertised name.
    ///
    /// Mirrors Go's `Connect`: refreshes the platform scan cache first
    /// (Windows `bthleenum.sys` needs it), then dials with service-discovery
    /// retries to ride out BLEDOM flakiness.
    ///
    /// Same as `connect_with_options` with [`ConnectOptions::default`].
    ///
    /// # Errors
    /// * [`Error::DeviceNotSeen`] — address never appeared during the scan.
    /// * [`Error::ServiceNotFound`] / [`Error::CharacteristicNotFound`].
    pub async fn connect(ble: &Ble, addr: &str, name: &str) -> Result<Self> {
        Self::connect_with_options(ble, addr, name, ConnectOptions::default()).await
    }

    /// Connect with custom timeouts/retries.
    ///
    /// # Errors
    /// Same as [`Lamp::connect`].
    pub async fn connect_with_options(
        ble: &Ble,
        addr: &str,
        name: &str,
        opts: ConnectOptions,
    ) -> Result<Self> {
        let peripheral = find_peripheral(ble.adapter(), addr, opts.timeout).await?;
        let mut lamp = Self {
            peripheral,
            write_char: empty_char(),
            notify_char: None,
            is_encrypted: is_encrypted_device(name),
            addr: addr.to_owned(),
            name: name.to_owned(),
            opts,
        };
        lamp.dial().await?;
        Ok(lamp)
    }

    /// `true` when the lamp name carries the `ELK-*` marker,
    /// i.e. frames with `CMD ∈ {1, 3, 4}` go out XOR-encrypted.
    #[must_use]
    pub fn is_encrypted(&self) -> bool {
        self.is_encrypted
    }

    /// Best-effort link check; `false` on any error too.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) {
    /// if lamp.is_connected().await {
    ///     let _ = lamp.light_on(true).await;
    /// }
    /// # }
    /// ```
    pub async fn is_connected(&self) -> bool {
        self.peripheral.is_connected().await.unwrap_or(false)
    }

    /// Drop the link and dial again with the same options.
    ///
    /// The handle stays usable: GATT ids are stable, so the cached
    /// characteristics remain valid after the redial.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.reconnect().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// * [`Error::Timeout`] — reconnect did not finish in `opts.timeout`.
    /// * [`Error::ServiceNotFound`] / [`Error::CharacteristicNotFound`] —
    ///   services did not come back after the redial.
    pub async fn reconnect(&self) -> Result<()> {
        let _ = self.peripheral.disconnect().await;
        sleep(self.opts.reconnect_delay).await;
        // dial через тень: peripheral — хендл на то же железо,
        // характеристики по UUID те же, так что self трогать не надо)
        let mut shadow = Self {
            peripheral: self.peripheral.clone(),
            write_char: self.write_char.clone(),
            notify_char: self.notify_char.clone(),
            is_encrypted: self.is_encrypted,
            addr: self.addr.clone(),
            name: self.name.clone(),
            opts: self.opts.clone(),
        };
        shadow.dial().await
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
    ///
    /// ```no_run
    /// use std::time::Duration;
    /// use lotus_lantern::{brightness_frame, color_rgb_frame};
    ///
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// let frames = [color_rgb_frame(255, 180, 120), brightness_frame(60, 0)];
    /// lamp.send_batch(&frames, Duration::from_millis(120)).await?;
    /// # Ok(())
    /// # }
    /// ```
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

    /// Static color from HSV (`h` 0..=360, `s`/`v` 0..=255).
    ///
    /// # Errors
    /// Propagates [`Error`] from the underlying frame write.
    #[allow(clippy::many_single_char_names)]
    pub async fn set_hsv(&self, h: u16, s: u8, v: u8) -> Result<()> {
        let (r, g, b) = hsv_to_rgb(h, s, v);
        self.set_color_rgb(r, g, b).await
    }

    /// Smooth brightness ramp `from` → `to`, endpoints included.
    ///
    /// Steps through [`brightness_steps`] and sleeps `step_delay` between
    /// writes. On a new strip ramp upwards from a low value, not downwards
    /// from full.
    ///
    /// # Errors
    /// Propagates [`Error`] from the underlying frame writes.
    pub async fn fade_brightness(
        &self,
        from: u8,
        to: u8,
        steps: u8,
        light_mode: LightMode,
        step_delay: Duration,
    ) -> Result<()> {
        self.fade_brightness_raw(from, to, steps, light_mode.as_u8(), step_delay)
            .await
    }

    /// [`Lamp::fade_brightness`] with a raw `light_mode` byte.
    ///
    /// Escape hatch for firmware values with no [`LightMode`] variant;
    /// bytes go out identical to the typed call.
    ///
    /// # Errors
    /// Propagates [`Error`] from the underlying frame writes.
    pub async fn fade_brightness_raw(
        &self,
        from: u8,
        to: u8,
        steps: u8,
        light_mode: u8,
        step_delay: Duration,
    ) -> Result<()> {
        for level in brightness_steps(from, to, steps) {
            self.set_brightness_raw(level, light_mode).await?;
            sleep(step_delay).await;
        }
        Ok(())
    }

    /// Brightness `0..=255`. New strip? Start around 30–80.
    ///
    /// ```no_run
    /// use lotus_lantern::LightMode;
    ///
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.set_brightness(60, LightMode::Mode0).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_brightness(&self, level: u8, light_mode: LightMode) -> Result<()> {
        self.set_brightness_raw(level, light_mode.as_u8()).await
    }

    /// [`Lamp::set_brightness`] with a raw `light_mode` byte.
    ///
    /// Escape hatch for firmware values with no [`LightMode`] variant.
    pub async fn set_brightness_raw(&self, level: u8, light_mode: u8) -> Result<()> {
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
    pub async fn set_rgbw_status(&self, rgbw_on: u32, light_mode: LightMode) -> Result<()> {
        self.set_rgbw_status_raw(rgbw_on, light_mode.as_u8()).await
    }

    /// [`Lamp::set_rgbw_status`] with a raw `light_mode` byte.
    ///
    /// Escape hatch for firmware values with no [`LightMode`] variant.
    pub async fn set_rgbw_status_raw(&self, rgbw_on: u32, light_mode: u8) -> Result<()> {
        self.send_frame(rgbw_status_frame(rgbw_on, light_mode))
            .await
    }

    // -- effects --------------------------------------------------------

    /// Built-in dynamic effect.
    ///
    /// ```no_run
    /// use lotus_lantern::EffectMode;
    ///
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.set_effect(EffectMode::Breathe, 128).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn set_mode(&self, mode: EffectMode) -> Result<()> {
        self.set_mode_raw(mode.as_u8()).await
    }

    /// [`Lamp::set_mode`] with a raw mode byte.
    ///
    /// Escape hatch for firmware values with no [`EffectMode`] variant;
    /// the `0x80` bit is still added on the wire like the stock app does.
    pub async fn set_mode_raw(&self, mode: u8) -> Result<()> {
        self.send_frame(mode_frame(mode)).await
    }

    /// Dynamic effect speed.
    pub async fn set_mode_speed(&self, speed: u8) -> Result<()> {
        self.send_frame(mode_speed_frame(speed)).await
    }

    /// Effect + speed in one call: `set_mode` then `set_mode_speed`.
    ///
    /// # Errors
    /// Propagates [`Error`] from the first failing frame write.
    pub async fn set_effect(&self, mode: EffectMode, speed: u8) -> Result<()> {
        self.set_effect_raw(mode.as_u8(), speed).await
    }

    /// [`Lamp::set_effect`] with a raw mode byte.
    ///
    /// Escape hatch for firmware values with no [`EffectMode`] variant.
    ///
    /// # Errors
    /// Propagates [`Error`] from the first failing frame write.
    pub async fn set_effect_raw(&self, mode: u8, speed: u8) -> Result<()> {
        self.set_mode_raw(mode).await?;
        self.set_mode_speed(speed).await
    }

    /// Music-react color.
    ///
    /// Single contract: `rgb` is `0xRRGGBB` (any high byte is ignored),
    /// `brightness` replaces the V channel via [`music_react_rgb`]. Pure
    /// black stays black instead of degrading into `brightness`-gray.
    pub async fn music_amplitude(&self, rgb: u32, brightness: u8) -> Result<()> {
        let (r, g, b) = music_react_rgb(rgb, brightness);
        self.send_frame(music_amplitude_frame(r, g, b)).await
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

    /// Mic EQ preset.
    pub async fn set_mic_eq_mode(&self, mode: MicEqMode) -> Result<()> {
        self.set_mic_eq_mode_raw(mode.as_u8()).await
    }

    /// [`Lamp::set_mic_eq_mode`] with a raw mode byte.
    ///
    /// Escape hatch for firmware values with no [`MicEqMode`] variant.
    pub async fn set_mic_eq_mode_raw(&self, mode: u8) -> Result<()> {
        self.send_frame(mic_eq_mode_frame(mode)).await
    }

    // -- laser (projector models) ----------------------------------------

    /// Laser projector state.
    pub async fn set_laser(&self, state: LaserState) -> Result<()> {
        self.set_laser_raw(state.as_u8()).await
    }

    /// [`Lamp::set_laser`] with a raw value byte.
    ///
    /// Escape hatch for firmware values with no [`LaserState`] variant.
    pub async fn set_laser_raw(&self, value: u8) -> Result<()> {
        self.send_frame(laser_frame(value)).await
    }

    /// Laser effect selector.
    pub async fn set_laser_mode(&self, mode: LaserMode) -> Result<()> {
        self.set_laser_mode_raw(mode.as_u8()).await
    }

    /// [`Lamp::set_laser_mode`] with a raw mode byte.
    ///
    /// Escape hatch for firmware values with no [`LaserMode`] variant.
    pub async fn set_laser_mode_raw(&self, mode: u8) -> Result<()> {
        self.send_frame(laser_mode_frame(mode)).await
    }

    /// Laser effect speed.
    pub async fn set_laser_speed(&self, speed: u8) -> Result<()> {
        self.send_frame(laser_speed_frame(speed)).await
    }

    // -- timing -----------------------------------------------------------

    /// Countdown timer firing at `hour:minute` on weekday `weeks`.
    ///
    /// `weeks == 0` means once (today/tomorrow); otherwise the target
    /// weekday with 0 = Sunday, mirroring the stock app. The device delay is
    /// computed from the current local time via [`countdown_delay`].
    ///
    /// ```no_run
    /// use lotus_lantern::TimingMode;
    ///
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// // разовый таймер, без гашения чужой ленты в примерах — только PowerOn.
    /// lamp.set_countdown_now(7, 30, 0, TimingMode::PowerOn).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// * [`Error::InvalidParam`] — `hour > 23`, `minute > 59`, `weeks > 6`.
    /// * [`Error`] from the underlying frame write.
    pub async fn set_countdown_now(
        &self,
        hour: u8,
        minute: u8,
        weeks: u8,
        mode: TimingMode,
    ) -> Result<()> {
        if hour > 23 {
            return Err(Error::InvalidParam(format!("hour {hour} > 23")));
        }
        if minute > 59 {
            return Err(Error::InvalidParam(format!("minute {minute} > 59")));
        }
        if weeks > 6 {
            return Err(Error::InvalidParam(format!("weeks {weeks} > 6")));
        }
        let ts = countdown_delay(hour, minute, weeks);
        self.set_countdown_raw(ts, mode.as_u8()).await
    }

    /// Countdown timer from a precomputed device timestamp.
    ///
    /// Raw escape hatch: `timestamp` is the `getTimeStamp` value
    /// ([`countdown_delay`]) and `timing_mode` goes out as-is.
    pub async fn set_countdown_raw(&self, timestamp: u32, timing_mode: u8) -> Result<()> {
        self.send_frame(countdown_frame(timestamp, timing_mode))
            .await
    }

    /// Push the controller clock.
    pub async fn send_system_time(&self, hour_minute: u32, weeks: u8) -> Result<()> {
        self.send_frame(system_time_frame(hour_minute, weeks)).await
    }

    /// Push the current local time as the controller clock.
    ///
    /// Packs [`chrono::Local`] hour/minute/second via
    /// [`pack_system_time_now`] — no manual `hour_minute` math needed.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.send_system_time_now().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Propagates [`Error`] from the underlying frame write.
    pub async fn send_system_time_now(&self) -> Result<()> {
        let (hour_minute, weeks) = pack_system_time_now();
        self.send_system_time(hour_minute, weeks).await
    }

    /// Timing status row.
    pub async fn send_timing_status(
        &self,
        hour_minute: u32,
        mode: TimingMode,
        weeks: u8,
    ) -> Result<()> {
        self.send_timing_status_raw(hour_minute, mode.as_u8(), weeks)
            .await
    }

    /// [`Lamp::send_timing_status`] with a raw timing-mode byte.
    ///
    /// Escape hatch for firmware values with no [`TimingMode`] variant.
    pub async fn send_timing_status_raw(
        &self,
        hour_minute: u32,
        timing_mode: u8,
        weeks: u8,
    ) -> Result<()> {
        self.send_frame(timing_status_frame(hour_minute, timing_mode, weeks))
            .await
    }

    // -- timing readback (0x85) -------------------------------------------

    /// Subscribe to the notify characteristic for timing readbacks.
    ///
    /// Idempotent: subscribing twice is harmless. Call once, then
    /// [`Lamp::read_timing_info`] whenever the `0x85` row is needed.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.subscribe_timing().await?;
    /// let info = lamp.read_timing_info().await?;
    /// let _ = info.timestamp();
    /// lamp.unsubscribe_timing().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// * [`Error::NotConnected`] — no live link.
    /// * [`Error::SubscribeFailed`] — no notify characteristic found, or the
    ///   backend refused the subscription.
    pub async fn subscribe_timing(&self) -> Result<()> {
        let ch = self.notify_char.clone().ok_or_else(|| {
            Error::SubscribeFailed("no notify characteristic on FFF0 service".to_owned())
        })?;
        if !self.is_connected().await {
            return Err(Error::NotConnected);
        }
        self.peripheral
            .subscribe(&ch)
            .await
            .map_err(|e| Error::SubscribeFailed(e.to_string()))
    }

    /// Unsubscribe from timing readbacks. Missing characteristic is not an
    /// error here.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// lamp.unsubscribe_timing().await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Propagates [`Error::SubscribeFailed`] from the backend.
    pub async fn unsubscribe_timing(&self) -> Result<()> {
        if let Some(ch) = self.notify_char.clone() {
            self.peripheral
                .unsubscribe(&ch)
                .await
                .map_err(|e| Error::SubscribeFailed(e.to_string()))?;
        }
        Ok(())
    }

    /// Read the `0x85` timing-info row.
    ///
    /// Three attempts in order: plain GATT read of the write characteristic
    /// (some firmware exposes the row there), then the
    /// [`timing_read_request_frame()`](crate::timing_read_request_frame)
    /// write followed by another read, then the notification wait on the
    /// `FFF4` characteristic (bounded by `opts.notification_timeout`).
    /// Foreign notifications are skipped while waiting.
    ///
    /// ```no_run
    /// # async fn demo(lamp: &lotus_lantern::Lamp) -> lotus_lantern::Result<()> {
    /// let info = lamp.read_timing_info().await?;
    /// println!("ts={} mode={} weeks={}", info.timestamp(), info.timing_mode(), info.weeks());
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// * [`Error::NotConnected`] — no live link.
    /// * [`Error::SubscribeFailed`] — no notify characteristic, subscription
    ///   refused, or the stream ended before the reply.
    /// * [`Error::Timeout`] — no `0x85` reply in time.
    pub async fn read_timing_info(&self) -> Result<TimingInfo> {
        if !self.is_connected().await {
            return Err(Error::NotConnected);
        }
        let notify = self.notify_char.clone().ok_or_else(|| {
            Error::SubscribeFailed("no notify characteristic on FFF0 service".to_owned())
        })?;
        // быстрый путь: часть прошивок отдаёт 0x85 обычным read c FFF3)
        if let Ok(raw) = self.read_write_char().await {
            if let Ok(info) = TimingInfo::parse(&raw) {
                return Ok(info);
            }
        }
        // медленный путь: подписались, спросили, перечитали, ждём нотификацию)
        // стрим берём ДО subscribe, чтоб ответ не пролетел мимо)
        let mut stream = self
            .peripheral
            .notifications()
            .await
            .map_err(|e| Error::SubscribeFailed(e.to_string()))?;
        self.peripheral
            .subscribe(&notify)
            .await
            .map_err(|e| Error::SubscribeFailed(e.to_string()))?;
        self.send_frame(timing_read_request_frame()).await?;
        // у части прошивок ответ появляется на read ПОСЛЕ запроса)
        if let Ok(raw) = self.read_write_char().await {
            if let Ok(info) = TimingInfo::parse(&raw) {
                return Ok(info);
            }
        }
        timeout(self.opts.notification_timeout, async {
            while let Some(n) = stream.next().await {
                if n.uuid != notify.uuid {
                    continue;
                }
                if let Ok(info) = TimingInfo::parse(&n.value) {
                    return Ok(info);
                }
            }
            Err(Error::SubscribeFailed(
                "notification stream ended before 0x85 reply".to_owned(),
            ))
        })
        .await
        .map_err(|_| Error::Timeout)?
    }

    /// Best-effort GATT read of the write characteristic (`FFF3` also carries
    /// the READ property on stock clones).
    async fn read_write_char(&self) -> Result<Vec<u8>> {
        timeout(
            self.opts.notification_timeout,
            self.peripheral.read(&self.write_char),
        )
        .await
        .map_err(|_| Error::Timeout)?
        .map_err(Error::Bluetooth)
    }

    // -- transport --------------------------------------------------------

    async fn dial(&mut self) -> Result<()> {
        if !self
            .peripheral
            .is_connected()
            .await
            .map_err(Error::Bluetooth)?
        {
            timeout(self.opts.timeout, self.peripheral.connect())
                .await
                .map_err(|_| Error::Timeout)?
                .map_err(Error::Bluetooth)?;
        }
        // bthleenum.sys needs a beat after connect before services resolve)
        sleep(self.opts.post_connect_delay).await;

        let mut last_err: Option<Error> = None;
        let mut found_service = false;
        let mut found_char = None;
        let mut notify_char = None;
        for _ in 0..self.opts.discovery_retries.max(1) {
            match self.peripheral.discover_services().await {
                Ok(()) => last_err = None,
                Err(e) => {
                    last_err = Some(Error::Bluetooth(e));
                    sleep(self.opts.discovery_retry_delay).await;
                    continue;
                }
            }
            // FFF4 с нотификациями иногда приезжает не с первого раза,
            // так что терпим и переспрашиваем, пока write уже нашли)
            (found_service, found_char, notify_char) = self.resolve_chars();
            if found_char.is_some() && notify_char.is_some() {
                break;
            }
            sleep(self.opts.discovery_retry_delay).await;
        }
        if let Some(e) = last_err {
            let _ = self.peripheral.disconnect().await;
            return Err(e);
        }
        if !found_service {
            let _ = self.peripheral.disconnect().await;
            return Err(Error::ServiceNotFound);
        }
        if let Some(ch) = found_char {
            self.write_char = ch;
            self.notify_char = notify_char;
            Ok(())
        } else {
            let _ = self.peripheral.disconnect().await;
            Err(Error::CharacteristicNotFound)
        }
    }

    /// Pick write + notify characteristics out of the discovered services.
    ///
    /// Returns `(fff0_seen, write_char, notify_char)`; notify is the first
    /// `NOTIFY`/`INDICATE` characteristic on `FFF0` (stock layout: `FFF4`).
    fn resolve_chars(
        &self,
    ) -> (
        bool,
        Option<btleplug::api::Characteristic>,
        Option<btleplug::api::Characteristic>,
    ) {
        let mut found_service = false;
        let mut found_char = None;
        let mut notify_char = None;
        for service in self.peripheral.services() {
            if service.uuid == SERVICE_UUID {
                found_service = true;
                for ch in &service.characteristics {
                    if ch.uuid == WRITE_CHAR_UUID {
                        found_char = Some(ch.clone());
                    }
                    // нотификации обычно на отдельной характеристике живут)
                    if notify_char.is_none()
                        && ch
                            .properties
                            .intersects(CharPropFlags::NOTIFY | CharPropFlags::INDICATE)
                    {
                        notify_char = Some(ch.clone());
                    }
                }
            }
        }
        (found_service, found_char, notify_char)
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

        for _ in 0..self.opts.write_retries.max(1) {
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
                    notify_char: self.notify_char.clone(),
                    is_encrypted: self.is_encrypted,
                    addr: self.addr.clone(),
                    name: self.name.clone(),
                    opts: self.opts.clone(),
                };
                sleep(self.opts.reconnect_delay).await;
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

async fn find_peripheral(adapter: &Adapter, addr: &str, timeout_: Duration) -> Result<Peripheral> {
    adapter
        .start_scan(ScanFilter::default())
        .await
        .map_err(Error::Bluetooth)?;
    let deadline = tokio::time::Instant::now() + timeout_;
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
