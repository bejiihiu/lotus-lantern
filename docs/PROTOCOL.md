# Lotus Lantern (宝莲灯) BLE Protocol

Reverse-engineered from `wl.smartled.apk` (XAPK 6-5-03) by
**[Rxflex](https://github.com/Rxflex)** — source:
`com.easylink.colorful.service.BluetoothLEService`. Every byte below is
his discovery; this doc is copied from his Go client
([Rxflex/LotusLantern](https://github.com/Rxflex/LotusLantern)) with a
Rust outline.

## Transport

BLE GATT. App also broadcasts via BLE advertising for multi-device push, but every command also goes through GATT writes — desktop API can ignore advertising path.

| Item | Value |
|---|---|
| Service UUID | `0000fff0-0000-1000-8000-00805f9b34fb` |
| Write characteristic UUID | `0000fff3-0000-1000-8000-00805f9b34fb` |
| Notify characteristic UUID | `0000fff4-0000-1000-8000-00805f9b34fb` |
| Write type | `WriteWithoutResponse` |
| Connection delay | sleep 1s after connect, then `discover_services()` |

### GATT findings on the `ELK-BLEDDM` clone (verified on hardware)

- `FFF3` exposes READ + WRITE. A plain GATT read of `FFF3` returns a
  20-byte ASCII serial (e.g. `YH10273854K162`-style), not command data —
  so the stock `7E 09 85 b0..b5` timing row is *not* readable there on
  this clone.
- `FFF4` exposes NOTIFY. Timing replies and the unsolicited timer
  broadcast below both arrive as `FFF4` notifications.
- `TimingInfo::parse` still accepts a real `7E 09 85 b0..b5` row for
  firmware that does answer properly; the client tries plain read, then
  request-write + read, then the notification wait, in that order.

Supported device-name prefixes: `ELK-`, `ELK~`, `LED LIGHT STRIP`, `XSL-`. App also listens for advertising-mode devices with prefix `NAME_BROADCAST_FILTER` (look up in `Global.java` if needed).

## Frame format

All commands are 9 bytes:

```
0x7E  LEN  CMD  P1  P2  P3  P4  P5  0xEF
```

- `0x7E` (126) — start byte
- `LEN` — payload length marker (4–8 in observed commands)
- `CMD` — command type (table below)
- `P1..P5` — params, padded with `0xFF` when unused
- `0xEF` (239) — end byte

## Encryption

Two layers — both **optional** for stock devices. Skip both unless device name contains literal `"ELK-*"` (asterisk is part of marker).

### Layer 1 — GATT write encryption (only if name contains `ELK-*`)

Triggers only when `CMD ∈ {1, 3, 4}`. Before encryption, header swap:
- `frame[0] = 0xAA`
- `frame[8] = 0x55`

Then 9-byte plaintext → 21-byte ciphertext via `EncryptionDecryptionKt.encryptBytes`:

```
random[12] ← random bytes
keystream[9]:
  for i in 0..9:
    ks[i] = (rnd[i]*27 & 0xFF) ^ ((rnd[(i+3)%12]+55) & 0xFF)
            ^ (rnd[(i+7)%12] >> 2 & 0xFF) ^ ((i*85) & 0xFF)
ciphertext[0..9]  = plaintext[i] ^ ks[i]
ciphertext[9..21] = rnd[i] ^ presetKey[i % 16]

presetKey = {0x2A, 0x7F, 0xC1, 0x94, 0x33, 0xDE, 0x45, 0xE0,
             0x8B, 0x11, 0x5C, 0xA6, 0x09, 0xF2, 0x7D, 0xB8}
```

### Layer 2 — Advertising XOR (broadcast path only — desktop won't use)

`com.easylink.colorful.utils.EncryptUtil.encode(buf, 1, 24, counter)` — XORs each byte with all 20 key bytes sequentially:

```
key = {0x59,0x4C,0x5A,0x4B,0x35,0x31,0x21,0x29,0x3E,0x48,
       0x40,0x76,0x64,0x62,0x51,0x44,0x5E,0x44,0x3F, counter}
```

Manufacturer ID for advertise = `0xBEE8` (48872).

## Command table (CMD = frame[2])

All from `BluetoothLEService.java`. `--` = unused, send `0xFF` or `0x00` per source.

| CMD | Action | Frame |
|---|---|---|
| `0x01` | Brightness | `7E 04 01 brightness lightMode FF FF 00 EF` |
| `0x02` | Mode speed | `7E 04 02 speed FF FF FF 00 EF` |
| `0x02` | Laser speed | `7E 04 02 speed 04 FF FF FF EF` |
| `0x03` | Mode | `7E 05 03 (mode\|0x80) 03 FF FF 00 EF` |
| `0x03` | External-mic EQ | `7E 05 03 (eq\|0x80) 04 FF FF 00 EF` |
| `0x03` | Laser lamp mode | `7E 05 03 mode 08 FF FF FF EF` |
| `0x04` | RGBW status | `7E 04 04 mask lightMode whiteFlag FF 00 EF` |
| `0x04` | Light ON/OFF | `7E 04 04 on 00 on FF 00 EF` (on = 0/1) |
| `0x05` | RGB color | `7E 07 05 03 R G B 0x10 EF` |
| `0x05` | Color temperature | `7E 06 05 02 warm cold FF 0x08 EF` |
| `0x05` | Single color | `7E 05 05 01 colorByte FF FF 0x08 EF` |
| `0x05` | Laser on/off | `7E 05 05 01 onOff FF FF 0x10 EF` |
| `0x05` | Music amplitude RGB | `7E 07 05 03 R G B 0x20 EF` |
| `0x06` | External mic sensitive | `7E 04 06 level FF FF FF 00 EF` |
| `0x07` | External mic on/off | `7E 04 07 onOff FF FF FF 00 EF` |
| `0x76` | Countdown timer | `7E 07 76 ts0 ts1 ts2 timingMode FF EF` |
| `0x81` | RGB pin order | `7E 06 81 R G B FF 00 EF` (default int 66051 = 0x010203) |
| `0x82` | Timing status | `7E 08 82 ts0 ts1 ts2 timingMode weeks EF` |
| `0x83` | System time set | `7E 07 83 ts0 ts1 ts2 weeks FF EF` |
| `0x85` (read) | Timing info readback | response `7E 09 85 b0 b1 b2 b3 b4 b5` (read characteristic) |

`ts0/1/2` = `time & 0xFF, time>>8 & 0xFF, time>>16 & 0xFF` where `time = Utils.getTimeStamp(hour, minute, weeks)`.

## Timing readback (`0x85`) on the `ELK-BLEDDM` clone (verified on hardware)

The documented flow is "write a read request, get `7E 09 85 b0..b5`
back". This clone does something else:

1. The client writes the read request to `FFF3`:
   `7E 04 85 00 FF FF FF 00 EF` (see `timing_read_request_frame`).
2. A GATT read of `FFF3` right after returns an **echo of the request
   bytes verbatim** — not a `7E 09 85 …` row. Do not parse the echo as
   timing data.
3. A freshly programmed countdown timer arrives **unsolicited** as an
   `FFF4` notification in countdown-write layout:
   `7E 07 76 lo mid hi mode FF EF`, where `lo/mid/hi` is the little-endian
   device timestamp (`seconds-until-fire * 10`, same packing as
   `Utils.getTimeStamp`) and `mode` is the timing-mode byte.

In short: on this clone there is no `0x85` reply to parse — the `0x76`
broadcast *is* the confirmation. Other firmware may still answer with the
documented `7E 09 85 b0..b5` row, which is why `TimingInfo::parse` accepts
both 9-byte shapes (see `src/timing.rs`).

`Mode` byte gets `+128` (sets high bit) — i.e. modes are sent as `0x80..0xFF`.

`RGB color` reorders depending on `RGB pin order` (`0x81`) — handled device-side.

## Rust implementation outline

```rust
use lotus_lantern::{Ble, Lamp};
use std::time::Duration;
use uuid::Uuid;

const SERVICE: Uuid = lotus_lantern::SERVICE_UUID; // 0000fff0-…
const WRITE: Uuid = lotus_lantern::WRITE_CHAR_UUID; // 0000fff3-…

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(15)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;
    lamp.set_color_rgb(255, 0, 128).await?;
    lamp.set_brightness(180, 0).await?;
    lamp.light_on(true).await?;
    Ok(())
}
```

Pure frame builders (`lotus_lantern::brightness_frame`, …) are `const fn`
returning `[u8; 9]` — match the table above byte-for-byte; the device may
reject a mismatched LEN.

`WriteWithoutResponse` works on most clones. Some firmware needs ~50 ms
gap between writes; use `Lamp::send_batch(frames, delay)` to throttle.

## What still unknown

- Exact `lightMode` and `mode` enum values — app builds them from UI. Sniff app once with HCI snoop log if you need exact mapping.
- `NAME_BROADCAST_FILTER` value — irrelevant unless using broadcast.
- `Utils.newColor(brightness, color)` mixing formula — ported as `blend_brightness` (HSV V-swap), golden-tested on grays/black.

Capture real BLE traffic with `adb shell setprop persist.bluetooth.btsnooplog true` + Wireshark to validate exact bytes per UI action.
