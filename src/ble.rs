//! Adapter handle + discovery, ported from `Discover` in `lamp.go`.
//!
//! Event-driven: scans sleep in `tokio::time`, never spin. Idle cost is
//! ~zero CPU — the radio does the work, we just await advertisements.

use std::time::Duration;

use btleplug::api::{Central, Manager as _, Peripheral as _, ScanFilter};
use btleplug::platform::{Adapter, Manager};
use tokio::time::{sleep, timeout};

use crate::{is_supported_name, Error, Result};

/// A lamp spotted during a scan: address + advertised name.
#[derive(Debug, Clone)]
pub struct DiscoveredLamp {
    /// MAC string (`AA:BB:CC:DD:EE:FF`) on most platforms; opaque id on macOS.
    pub addr: String,
    /// Advertised local name (`ELK-BLEDOM…`, …).
    pub name: String,
    /// Last RSSI in dBm, when reported.
    pub rssi: Option<i16>,
}

/// Bluetooth adapter wrapper. Owns nothing radio-heavy; cheap to clone.
#[derive(Debug, Clone)]
pub struct Ble {
    adapter: Adapter,
}

impl Ble {
    /// Bring up the default adapter.
    ///
    /// # Errors
    /// Returns [`Error::NoAdapter`] when the machine exposes no BLE adapter.
    pub async fn new() -> Result<Self> {
        let manager = Manager::new().await?;
        let mut adapters = manager.adapters().await?;
        adapters
            .pop()
            .map(|a| Self { adapter: a })
            .ok_or(Error::NoAdapter)
    }

    /// Adapter handle for advanced use (custom scan filters, …).
    #[must_use]
    pub fn adapter(&self) -> &Adapter {
        &self.adapter
    }

    /// Scan for `timeout_` and return every matching lamp.
    pub async fn scan(&self, timeout_: Duration) -> Result<Vec<DiscoveredLamp>> {
        self.scan_filtered(timeout_, |_| true).await
    }

    /// Scan + filter by advertised name, strongest signal first.
    ///
    /// `predicate` решает, брать ли лампу (например `|n| n.starts_with("ELK-")`);
    /// сортировка по `rssi` — без сигнала (`None`) в конце.
    ///
    /// # Errors
    /// Propagates [`Error::Bluetooth`] from the adapter.
    pub async fn scan_filtered(
        &self,
        timeout_: Duration,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<Vec<DiscoveredLamp>> {
        self.adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(Error::Bluetooth)?;
        sleep(timeout_).await;
        let peripherals = self.adapter.peripherals().await.map_err(Error::Bluetooth)?;
        let _ = self.adapter.stop_scan().await;

        let mut out = Vec::new();
        for p in peripherals {
            let props = p.properties().await.map_err(Error::Bluetooth)?;
            let Some(props) = props else { continue };
            let name = props.local_name.unwrap_or_default();
            if !is_supported_name(&name) || !predicate(&name) {
                continue;
            }
            out.push(DiscoveredLamp {
                addr: props.address.to_string(),
                name,
                rssi: props.rssi,
            });
        }
        // strongest first, unknown signal last)
        out.sort_by_key(|l| std::cmp::Reverse(l.rssi.unwrap_or(i16::MIN)));
        Ok(out)
    }

    /// Scan for `timeout_` and return every matching lamp, strongest first.
    ///
    /// # Errors
    /// Propagates [`Error::Bluetooth`] from the adapter.
    pub async fn scan_sorted(&self, timeout_: Duration) -> Result<Vec<DiscoveredLamp>> {
        self.scan_filtered(timeout_, |_| true).await
    }

    /// Scan for the *first* matching lamp, like Go's `Discover`.
    ///
    /// Polls the adapter's peripheral list on a sleep loop (no busy wait)
    /// until `timeout_` elapses.
    ///
    /// # Errors
    /// Returns [`Error::LampNotFound`] when nothing matches in time.
    pub async fn discover(&self, timeout_: Duration) -> Result<DiscoveredLamp> {
        self.discover_filtered(timeout_, |_| true).await
    }

    /// First lamp whose advertised name passes `predicate`.
    ///
    /// # Errors
    /// Returns [`Error::LampNotFound`] when nothing matches in time.
    pub async fn discover_filtered(
        &self,
        timeout_: Duration,
        predicate: impl Fn(&str) -> bool,
    ) -> Result<DiscoveredLamp> {
        self.adapter
            .start_scan(ScanFilter::default())
            .await
            .map_err(Error::Bluetooth)?;
        let deadline = tokio::time::Instant::now() + timeout_;
        let found = loop {
            let peripherals = self.adapter.peripherals().await.map_err(Error::Bluetooth)?;
            let mut hit: Option<DiscoveredLamp> = None;
            for p in &peripherals {
                let props = p.properties().await.map_err(Error::Bluetooth)?;
                let Some(props) = props else { continue };
                let name = props.local_name.unwrap_or_default();
                if is_supported_name(&name) && predicate(&name) {
                    hit = Some(DiscoveredLamp {
                        addr: props.address.to_string(),
                        name,
                        rssi: props.rssi,
                    });
                    break;
                }
            }
            if let Some(lamp) = hit {
                break lamp;
            }
            if tokio::time::Instant::now() >= deadline {
                return Err(Error::LampNotFound);
            }
            sleep(Duration::from_millis(250)).await;
        };
        let _ = timeout(Duration::from_secs(1), self.adapter.stop_scan()).await;
        Ok(found)
    }
}
