//! Built-in effects via `set_effect(mode, speed)` + HSV rainbow.
//!
//! ```sh
//! cargo run --example effects
//! ```

use std::time::Duration;

use lotus_lantern::{Ble, Lamp};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(15)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    lamp.light_on(true).await?;
    lamp.set_brightness(60, 0).await?;
    println!("warm-up dim ok");

    for mode in 0..6u8 {
        lamp.set_effect(mode, 128).await?;
        println!("effect {mode} ok");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // радуга через hsv: полный круг за 36 шагов)
    for step in 0..36u16 {
        let h = step * 10;
        lamp.set_hsv(h, 255, 120).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!("hsv rainbow ok");

    lamp.close().await?;
    println!("done");
    Ok(())
}
