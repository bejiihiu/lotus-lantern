//! Power + brightness fade. Starts dim, stays safe.
//!
//! ```sh
//! cargo run --example power
//! ```

use std::time::Duration;

use lotus_lantern::{Ble, Lamp};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    println!("discovering lamp...");
    let ble = Ble::new().await?;
    let found = ble.discover(Duration::from_secs(15)).await?;
    println!("found: {} {:?}", found.addr, found.name);

    let lamp = Lamp::connect(&ble, &found.addr, &found.name).await?;
    println!("connected, encrypted={}", lamp.is_encrypted());

    lamp.light_on(true).await?;
    println!("on ok");

    // снизу вверх: сначала тускло, потом разгоняемся)
    lamp.fade_brightness(30, 150, 12, 0, Duration::from_millis(150))
        .await?;
    println!("fade up ok");
    tokio::time::sleep(Duration::from_secs(1)).await;

    lamp.fade_brightness(150, 40, 12, 0, Duration::from_millis(150))
        .await?;
    println!("fade down ok");

    lamp.light_on(false).await?;
    println!("off ok");
    lamp.close().await?;
    println!("done");
    Ok(())
}
