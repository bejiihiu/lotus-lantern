//! Built-in effects via typed `EffectMode` + HSV rainbow.
//!
//! What it does: warms up dim, steps through every named effect with
//! `set_effect(mode, speed)`, then runs a short HSV rainbow at low value.
//!
//! Run: `cargo run --example effects`
//!
//! Safety: brightness 60 the whole time, rainbow value 80 max. Strobe modes
//! flash — look away if you are photosensitive.

use std::time::Duration;

use lotus_lantern::{Ble, EffectMode, Lamp, LightMode};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(8)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    lamp.light_on(true).await?;
    lamp.set_brightness(60, LightMode::Mode0).await?;
    println!("warm-up dim ok");

    let modes: &[(&str, EffectMode)] = &[
        ("jump", EffectMode::Jump),
        ("strobe", EffectMode::Strobe),
        ("breathe", EffectMode::Breathe),
        ("warning", EffectMode::Warning),
    ];
    for (label, mode) in modes {
        lamp.set_effect(*mode, 128).await?;
        println!("effect {label} ok");
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    // радуга через hsv: полный круг за 36 шагов, value низкий)
    for step in 0..36u16 {
        let h = step * 10;
        lamp.set_hsv(h, 255, 80).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    println!("hsv rainbow ok");

    lamp.close().await?;
    println!("done");
    Ok(())
}
