//! Color-cycle demo on the first lamp found. Port of `examples/demo`.
//!
//! Safe by design: starts dim (brightness 40), pauses between steps, and
//! sends only stock-app bytes. If anything looks wrong, kill the process —
//! the strip keeps its last state, nothing keeps writing on its own.
//!
//! ```sh
//! cargo run --example demo
//! ```

use std::time::Duration;

use lotus_lantern::{Ble, Lamp};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(15)).await?;
    println!("found: {} {:?}", found.addr, found.name);

    println!("connecting...");
    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;
    println!("connected, running color cycle (starts dim)");

    // warm up dim so a fresh strip never jumps to full white)
    lamp.light_on(true).await?;
    println!("on ok");
    lamp.set_brightness(40, 0).await?;
    println!("warm-up dim ok");
    tokio::time::sleep(Duration::from_millis(1200)).await;

    let steps: &[(&str, u8, u8, u8)] = &[
        ("red", 255, 0, 0),
        ("green", 0, 255, 0),
        ("blue", 0, 0, 255),
        ("yellow", 255, 255, 0),
        ("magenta", 255, 0, 255),
        ("cyan", 0, 255, 255),
        ("white", 255, 255, 255),
    ];
    for (label, r, g, b) in steps {
        lamp.set_color_rgb(*r, *g, *b).await?;
        println!("{label} ok");
        tokio::time::sleep(Duration::from_millis(1500)).await;
    }

    lamp.set_brightness(80, 0).await?;
    println!("dim ok");
    tokio::time::sleep(Duration::from_secs(1)).await;
    lamp.set_brightness(255, 0).await?;
    println!("bright ok");

    lamp.close().await?;
    println!("done");
    Ok(())
}
