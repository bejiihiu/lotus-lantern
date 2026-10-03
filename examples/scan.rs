//! List nearby lamps, strongest signal first.
//!
//! What it does: scans for a few seconds and prints every supported lamp
//! with address, advertised name and RSSI. Read-only, touches no strip state.
//!
//! Run: `cargo run --example scan`
//!
//! Safety: no writes at all — safe to run even on a strip you just unboxed.

use std::time::Duration;

use lotus_lantern::Ble;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let ble = Ble::new().await?;
    println!("scanning 6s...");
    let lamps = ble.scan(Duration::from_secs(6)).await?;
    if lamps.is_empty() {
        println!("no lamps found — power the strip and move closer");
    }
    for lamp in lamps {
        let rssi = lamp.rssi.map_or_else(|| "?".to_owned(), |r| r.to_string());
        println!("{}  rssi={rssi}  {:?}", lamp.addr, lamp.name);
    }
    Ok(())
}
