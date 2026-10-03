# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] — 2026-10-03

### BREAKING

- Magic `u8` mode bytes are now typed enums: `set_brightness(level, LightMode)`,
  `set_mode(EffectMode)`, `set_effect(EffectMode, speed)`,
  `set_mic_eq_mode(MicEqMode)`, `set_laser(LaserState)`,
  `set_laser_mode(LaserMode)`, `set_countdown_now(.., TimingMode)`,
  `send_timing_status(.., TimingMode, ..)`, `set_rgbw_status(.., LightMode)`,
  `fade_brightness(.., LightMode, ..)`. Every enum has a `Custom(u8)` variant
  and a `*_raw(u8)` method on `Lamp` with byte-identical output, so unknown
  firmware values keep working — but call sites passing plain `u8` must be
  updated.
- `music_amplitude` now takes a single packed `0xRRGGBB` color plus
  brightness (`music_amplitude(rgb, brightness)`); the high byte is ignored
  and pure black stays black instead of degrading into brightness-gray.
- `scan` / `scan_sorted` / `scan_filtered` now deduplicate by address
  (strongest RSSI wins) and sort strongest-first; lamps with unknown RSSI
  sort last. Repeated advertisements for one strip no longer produce
  duplicate entries.

### Added

- New module `model` with `EffectMode`, `LightMode`, `MicEqMode`,
  `LaserState`, `LaserMode`, `TimingMode` (`as_u8` / `from_u8`, `Custom(u8)`
  passthrough).
- `Lamp::reconnect` / `Lamp::is_connected`, `ConnectOptions`
  (`Lamp::connect_with_options`) with timeout/retry knobs.
- `Lamp::send_system_time_now`, `Lamp::set_countdown_now` (validated
  `hour/minute/weeks` + `countdown_delay` math).
- `Lamp::subscribe_timing` / `Lamp::read_timing_info` /
  `Lamp::unsubscribe_timing` and `TimingInfo::parse` (`slots`, `timestamp()`,
  `timing_mode()`, `weeks()`).
- `ScanOptions { rssi_min }`, `Ble::scan_with_options`,
  `Ble::scan_filtered_with_options`, `Ble::discover_with_filter`.
- New error variants: `NotConnected`, `SubscribeFailed`, `InvalidParam`.
- Examples `effects`, `timing`, `rgbw`, `batch` (plus reworked `demo` and
  `scan` on the typed API); new `docs/EXAMPLES.md`.
- Hardware notes in `docs/PROTOCOL.md`: `0x85` readback echo and `0x76`
  broadcast on the `ELK-BLEDDM` clone, GATT findings for `FFF3`/`FFF4`.

### Changed

- All examples start dim (brightness ≤ 80) and use short 6–8 s scans.
- README API section rewritten for the typed signatures; protocol summary
  now mentions the `FFF4` notify characteristic.
- `Cargo.toml` version bumped to `0.2.0`.

### Fixed

- `music_amplitude(black)` no longer lights the strip gray at brightness.
- Scan results no longer contain one entry per advertisement burst.
