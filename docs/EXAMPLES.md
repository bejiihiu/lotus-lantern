# Examples

Six small end-to-end programs, all starting dim (brightness ≤ 80) so a
fresh strip never gets blasted. Each one discovers the first nearby lamp,
so run `scan` first to make sure yours is visible.

| Example | What it shows | Run |
|---|---|---|
| `scan` | Nearby lamps, strongest first. Read-only — the safest first run. | `cargo run --example scan` |
| `demo` | Static color cycle at low brightness (`set_color_rgb`, typed `set_brightness`). | `cargo run --example demo` |
| `effects` | Built-in effects via typed `EffectMode` (`set_effect`) at several speeds, plus an HSV rainbow. | `cargo run --example effects` |
| `timing` | Controller clock (`send_system_time_now`), a PowerOn-only countdown (`set_countdown_now`), and the `0x85` readback (`read_timing_info` + `TimingInfo` parse). | `cargo run --example timing` |
| `rgbw` | Warm/cold white balance (`set_color_temperature`), pin-order reset (`set_pin_sequence`), RGBW mask (`set_rgbw_status`). | `cargo run --example rgbw` |
| `batch` | One "evening scene" (warm amber, stepped brightness) sent as prebuilt frames via `send_batch` with a 120 ms throttle. | `cargo run --example batch` |

Notes:

- `scan` never writes — run it any time, even mid-debug.
- `timing` programs a real one-shot PowerOn timer on the controller (a few
  minutes out). PowerOn is a no-op on an already-lit strip, which is why
  the example uses it instead of PowerOff.
- `rgbw` resets the pin order to the stock default `0x010203`, so colors
  cannot stay permuted after it.
- `batch` is the pattern to copy for scenes: build `[u8; 9]` frames up
  front, then one throttled `send_batch` — cheap clones drop back-to-back
  writes without the gap.
