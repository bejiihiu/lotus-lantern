# lotus-lantern (Rust)

Rust port of the Go client **[Rxflex/LotusLantern](https://github.com/Rxflex/LotusLantern)** —
same protocol, same bytes, async `tokio` + [`btleplug`](https://github.com/deviceplug/btleplug) transport.

Async Rust client for **Lotus Lantern** / **BLEDOM** / **ELK-BLEDOM** /
**LED LIGHT STRIP** / **XSL-** family of cheap BLE LED-strip controllers.
Reverse-engineered from the stock Android app (`wl.smartled`, "宝莲灯") —
see [`docs/PROTOCOL.md`](docs/PROTOCOL.md) for the wire format.

No phone, no app, no cloud. Just BLE GATT writes from your machine, now
with Rust's fearless concurrency.

```rust
use std::time::Duration;
use lotus_lantern::{Ble, Lamp};

let ble = Ble::new().await?;
let found = ble.discover(Duration::from_secs(15)).await?;
let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

lamp.light_on(true).await?;
lamp.set_color_rgb(255, 0, 128).await?;
lamp.set_brightness(180, 0).await?;
lamp.set_mode(5).await?;
lamp.close().await?;
```

## Install

```toml
[dependencies]
lotus-lantern = "0.1"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

Platform stack: [`btleplug`](https://github.com/deviceplug/btleplug) +
`tokio`. Works on Linux, macOS, Windows.

## Compatibility

| Device prefix       | Status     | Notes                                  |
|---------------------|------------|----------------------------------------|
| `ELK-BLEDOM*`       | Tested     | Most common clone                      |
| `ELK-*` (encrypted) | Supported  | XOR cipher applied for cmd ∈ {1, 3, 4} |
| `ELK~*` (wavy)      | Supported  | Untested, same protocol                |
| `LED LIGHT STRIP*`  | Supported  | Untested                               |
| `XSL-*`             | Supported  | Untested                               |

## Examples

```bash
# List nearby lamps
cargo run --example scan

# Color cycle on the first one found (starts dim — strip-friendly)
cargo run --example demo
```

> First run on a new strip? The demo warms up at brightness 40 before
> doing anything vivid. Your LEDs will thank you.

## API

### Discovery & connection
- `Ble::new()` — bring up the default adapter.
- `Ble::discover(timeout)` — first matching lamp; `Ble::scan(timeout)` — all of them.
- `Lamp::connect(&ble, addr, name)` — connect to a known lamp.
- `Lamp::close()` — disconnect.
- `Lamp::send_batch(frames, delay)` — send raw 9-byte frames in sequence.

### Power & color
- `light_on(bool)`, `set_color_rgb(r, g, b)` / `set_color(0xRRGGBB)`,
  `set_color_temperature(warm, cold)`, `set_single_color(idx)`,
  `set_brightness(level, light_mode)`, `set_pin_sequence(seq)`.

### Effects / mic / laser / timing / RGBW
- `set_mode(mode)`, `set_mode_speed(speed)`, `music_amplitude(color, brightness)`.
- `set_mic_on_off`, `set_mic_sensitive`, `set_mic_eq_mode`.
- `set_laser`, `set_laser_mode`, `set_laser_speed`.
- `set_countdown`, `send_system_time`, `send_timing_status`.
- `set_rgbw_status(rgbw_on, light_mode)`.

Pure builders (`light_on_frame`, `brightness_frame`, …) are also public:
`const fn`, zero-alloc, golden-tested byte-for-byte against the Go client.

## Will it fry my strip?

Short answer: **no — it can't send anything the stock app wouldn't.**

Every command is bit-identical to what `wl.smartled` transmits
(verified by golden tests in `tests/golden_frames.rs`). Params are typed
`u8`, so oversized values can't wrap around; the XOR cipher only
scrambles bytes, never amplifies current; and the controller firmware
remains the sole authority over LED power. Start at low brightness on a
fresh strip anyway — same advice as with the original app.

## Notes for Windows users

BLEDOM clones interact poorly with the Windows BLE GATT stack
(`bthleenum.sys`). `connect` works around this by:

1. Scanning briefly first so the advertisement cache is fresh.
2. Retrying service discovery (up to 25 times).
3. Reconnecting automatically when a write fails (BLEDOM commonly drops
   the link after 3–10 writes).

If you still get errors: move closer (RSSI > -75 helps), make sure no
phone is connected (one BLE master only), update the BT driver, or plug
in a USB BLE dongle.

## Performance

BLE is I/O-bound — the radio is the bottleneck, not your CPU. This crate
keeps it that way: fixed-size `[u8; 9]` / `[u8; 21]` stack buffers (no
per-write allocation), `const fn` frame builders, event-driven scans via
`tokio::time::sleep` (zero CPU while idle), minimal tokio features.

## Protocol summary

Service `0xFFF0`, write characteristic `0xFFF3`. Each command is 9 bytes:

```text
0x7E  LEN  CMD  P1  P2  P3  P4  P5  0xEF
```

See [`docs/PROTOCOL.md`](docs/PROTOCOL.md) for the full table.

## Credits — the human kind 💛

This whole project exists because one person did the unglamorous,
magnificent work first: **[Rxflex](https://github.com/Rxflex)**, author of
the original Go client [Rxflex/LotusLantern](https://github.com/Rxflex/LotusLantern).
He sat down with cheap clone strips, a stock APK in Chinese, and an ocean
of patience — and reverse-engineered the entire BLE protocol byte by byte,
then published the client and the protocol doc for everyone, free,
MIT-licensed. No vendor docs, no SDK, no help from the manufacturer. Just
curiosity and stubbornness, the two finest engineering virtues.

This Rust crate is a faithful port of his work; every frame builder, every
magic byte, every quirk in the timing math is his discovery. If your room
glows nicely tonight, that's Rxflex's doing. Go star
[his repo](https://github.com/Rxflex/LotusLantern), and if you ever meet
him — the man has earned a beverage of his choice. Thank you, Rxflex. 🪷

Wire format originally extracted from
`com.easylink.colorful.service.BluetoothLEService` in `wl.smartled.apk`.
Original APK © its respective owners and **not** redistributed here.

## License

MIT — see [`LICENSE`](LICENSE).
