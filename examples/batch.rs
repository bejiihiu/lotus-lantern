//! Evening scene in one throttled batch.
//!
//! What it does: builds a static "evening scene" (warm color, low
//! brightness) as prebuilt 9-byte frames and sends them via `send_batch`
//! with a 120 ms gap so cheap clones keep up.
//!
//! Run: `cargo run --example batch`
//!
//! Safety: scene peaks at brightness 70 — warm dim glow, no full-white
//! flashes. Throttle keeps the link stable (clones drop writes in bursts).

use std::time::Duration;

use lotus_lantern::{brightness_frame, color_rgb_frame, light_on_frame, Ble, DiscoveredLamp, Lamp};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found: DiscoveredLamp = ble.discover(Duration::from_secs(8)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    // вечерняя сцена: вкл, тёплый янтарь, яркость вверх лесенкой)
    let frames = [
        light_on_frame(true),
        color_rgb_frame(255, 170, 90),
        brightness_frame(30, 0),
        brightness_frame(50, 0),
        brightness_frame(70, 0),
    ];
    lamp.send_batch(&frames, Duration::from_millis(120)).await?;
    println!("evening scene ok (5 frames, 120 ms apart)");

    lamp.close().await?;
    println!("done");
    Ok(())
}
