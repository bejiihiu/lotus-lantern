//! Controller clock + countdown timer.
//!
//! ```sh
//! cargo run --example clock
//! ```

use chrono::Timelike;
use lotus_lantern::{pack_hour_minute, Ble, Lamp};
use std::time::Duration;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(15)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    // пушим время контроллеру: час/минута локальные, weeks=0 = разово)
    let now = chrono::Local::now();
    let hm = pack_hour_minute(u8::try_from(now.hour())?, u8::try_from(now.minute())?, 0);
    lamp.send_system_time(hm, 0).await?;
    println!("system time ok");

    // таймер на +1 минуту от сейчас, разовый)
    let (h, m) = (
        u8::try_from(now.hour() % 24)?,
        u8::try_from((now.minute() + 1) % 60)?,
    );
    lamp.set_countdown(h, m, 0, 0).await?;
    println!("countdown {h:02}:{m:02} ok");

    lamp.close().await?;
    println!("done");
    Ok(())
}
