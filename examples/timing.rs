//! Controller clock + timing readback.
//!
//! What it does: pushes the current local time with `send_system_time_now`,
//! programs a one-shot `PowerOn` timer a few minutes out with
//! `set_countdown_now`, then reads the `0x85` row back with
//! `read_timing_info` and prints the parsed slots.
//!
//! Run: `cargo run --example timing`
//!
//! Safety: starts dim (brightness 40); the demo timer is PowerOn-only so it
//! never darkens anyone's strip. Note the timer stays programmed on the
//! controller — that is the point of the demo.

use std::time::Duration;

use chrono::Timelike as _;
use lotus_lantern::{Ble, Lamp, LightMode, TimingInfo, TimingMode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(8)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    lamp.light_on(true).await?;
    lamp.set_brightness(40, LightMode::Mode0).await?;
    println!("warm-up dim ok");

    // часы контроллера — текущее локальное время)
    lamp.send_system_time_now().await?;
    println!("system time ok");

    // разовый таймер PowerOn на +3 минуты — ленту не гасит)
    let now = chrono::Local::now();
    let total = now.hour() * 60 + now.minute() + 3;
    let (h, m) = (u8::try_from((total / 60) % 24)?, u8::try_from(total % 60)?);
    lamp.set_countdown_now(h, m, 0, TimingMode::PowerOn).await?;
    println!("countdown {h:02}:{m:02} power-on ok");

    // читаем 0x85-строку обратно и печатаем разбор)
    match lamp.read_timing_info().await {
        Ok(info) => print_timing(info),
        Err(e) => println!("timing readback unavailable on this firmware: {e}"),
    }

    lamp.close().await?;
    println!("done");
    Ok(())
}

fn print_timing(info: TimingInfo) {
    println!(
        "timing slots={:02X?} ts={} mode={} weeks={}",
        info.slots,
        info.timestamp(),
        info.timing_mode(),
        info.weeks()
    );
    println!("note: value = seconds-until-fire * 10 (see docs/PROTOCOL.md)");
}
