//! List nearby lamps. Port of `examples/scan` from the Go client.
//!
//! ```sh
//! cargo run --example scan
//! ```

use std::time::Duration;

use lotus_lantern::Ble;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ble = Ble::new().await?;
    println!("scanning 15s...");
    let lamps = ble.scan(Duration::from_secs(15)).await?;
    if lamps.is_empty() {
        println!("no lamps found — power the strip and move closer");
    }
    for lamp in lamps {
        println!(
            "{}  rssi={}  {:?}",
            lamp.addr,
            lamp.rssi.map_or_else(|| "?".into(), |r| r.to_string()),
            lamp.name
        );
    }
    Ok(())
}
