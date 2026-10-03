//! White balance + pin order + RGBW mask.
//!
//! What it does: warms up dim, nudges the warm/cold white balance with
//! `set_color_temperature`, resets the RGB pin order to the default
//! `0x010203`, then toggles the RGBW channel mask via `set_rgbw_status`.
//!
//! Run: `cargo run --example rgbw`
//!
//! Safety: brightness 50 max; pin order is reset to the stock default, not
//! randomized — colors cannot get stuck permuted after this example.

use std::time::Duration;

use lotus_lantern::{Ble, Lamp, LightMode, DEFAULT_PIN_SEQUENCE};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(8)).await?;
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;

    lamp.light_on(true).await?;
    lamp.set_brightness(50, LightMode::Mode0).await?;
    println!("warm-up dim ok");

    // баланс белого: тёплый пониже, холодный повыше)
    lamp.set_color_temperature(30, 120).await?;
    println!("color temperature ok");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // пин-порядок в дефолт — если цвета были перепутаны, тут чинятся)
    lamp.set_pin_sequence(DEFAULT_PIN_SEQUENCE).await?;
    println!("pin sequence default ok");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // rgbw-маска как в стоковом приложении, белый флаг из младшего бита)
    lamp.set_rgbw_status(0x01_01_01, LightMode::Mode2).await?;
    println!("rgbw mask ok");
    tokio::time::sleep(Duration::from_secs(1)).await;

    // обратно в обычный rgb-режим)
    lamp.set_rgbw_status(0, LightMode::Mode0).await?;
    println!("rgbw off ok");

    lamp.close().await?;
    println!("done");
    Ok(())
}
