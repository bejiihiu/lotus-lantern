//! Adapter handle + discovery, ported from `Discover` in `lamp.go`.
//!
//! Event-driven: scans sleep in `tokio::time`, never spin. Idle cost is
//! ~zero CPU — the radio does the work, we just await advertisements.

use std::collections::HashMap;
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

/// Knobs for [`Ble::scan_with_options`] and [`Ble::discover_with_filter`].
///
/// ```no_run
/// use std::time::Duration;
/// use lotus_lantern::{Ble, ScanOptions};
///
/// # async fn demo(ble: &Ble) -> lotus_lantern::Result<()> {
/// let opts = ScanOptions { rssi_min: Some(-70) };
/// let lamps = ble.scan_with_options(Duration::from_secs(6), opts).await?;
/// let _ = lamps.len();
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, Copy, Default)]
pub struct ScanOptions {
    /// Skip lamps weaker than this RSSI in dBm (e.g. `-70` keeps only
    /// nearby ones). `None` disables the filter. Lamps with unknown RSSI
    /// (`None`) are dropped while the filter is active.
    pub rssi_min: Option<i16>,
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
    ///
    /// Results are deduplicated by address (strongest RSSI wins) and sorted
    /// strongest-first. No RSSI floor — see [`Ble::scan_with_options`].
    pub async fn scan(&self, timeout_: Duration) -> Result<Vec<DiscoveredLamp>> {
        self.scan_filtered(timeout_, |_| true).await
    }

    /// Scan with an RSSI floor plus dedup by address.
    ///
    /// Same as [`Ble::scan`], but lamps weaker than
    /// `options.rssi_min` are dropped before dedup.
    ///
    /// # Errors
    /// Propagates [`Error::Bluetooth`] from the adapter.
    pub async fn scan_with_options(
        &self,
        timeout_: Duration,
        options: ScanOptions,
    ) -> Result<Vec<DiscoveredLamp>> {
        self.scan_filtered_with_options(timeout_, options, |_| true)
            .await
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
        self.scan_filtered_with_options(timeout_, ScanOptions::default(), predicate)
            .await
    }

    /// [`Ble::scan_filtered`] plus [`ScanOptions`] (RSSI floor).
    ///
    /// Dedup by address and strongest-first sort always apply.
    ///
    /// # Errors
    /// Propagates [`Error::Bluetooth`] from the adapter.
    pub async fn scan_filtered_with_options(
        &self,
        timeout_: Duration,
        options: ScanOptions,
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
            if !passes_rssi(props.rssi, options.rssi_min) {
                continue;
            }
            out.push(DiscoveredLamp {
                addr: props.address.to_string(),
                name,
                rssi: props.rssi,
            });
        }
        Ok(dedup_sort(out))
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
        self.discover_with_filter(timeout_, None, predicate).await
    }

    /// First lamp passing `predicate` **and** the RSSI floor.
    ///
    /// `rssi_min` behaves like [`ScanOptions::rssi_min`]: when set, lamps
    /// with weaker or unknown RSSI are skipped while polling.
    ///
    /// ```no_run
    /// use std::time::Duration;
    ///
    /// # async fn demo(ble: &lotus_lantern::Ble) -> lotus_lantern::Result<()> {
    /// let found = ble
    ///     .discover_with_filter(Duration::from_secs(6), Some(-75), |n| {
    ///         n.starts_with("ELK-BLEDOM")
    ///     })
    ///     .await?;
    /// let _ = found.addr;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Errors
    /// Returns [`Error::LampNotFound`] when nothing matches in time, and
    /// propagates [`Error::Bluetooth`] from the adapter.
    pub async fn discover_with_filter(
        &self,
        timeout_: Duration,
        rssi_min: Option<i16>,
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
                if is_supported_name(&name) && predicate(&name) && passes_rssi(props.rssi, rssi_min)
                {
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

/// RSSI gate: `None` floor lets everything through, otherwise the lamp needs
/// a known RSSI at or above the floor.
fn passes_rssi(rssi: Option<i16>, rssi_min: Option<i16>) -> bool {
    match (rssi, rssi_min) {
        (_, None) => true,
        (Some(r), Some(min)) => r >= min,
        (None, Some(_)) => false,
    }
}

/// Dedup by address (strongest RSSI wins), strongest-first, unknown signal
/// last. Платформа отдаёт дубли, если лампа светит в эфир часто)
fn dedup_sort(lamps: Vec<DiscoveredLamp>) -> Vec<DiscoveredLamp> {
    // по адресу: у кого rssi сильнее, тот и остаётся)
    let mut by_addr: HashMap<String, DiscoveredLamp> = HashMap::new();
    for lamp in lamps {
        by_addr
            .entry(lamp.addr.clone())
            .and_modify(|kept| {
                if strength(&lamp) > strength(kept) {
                    *kept = lamp.clone();
                }
            })
            .or_insert(lamp);
    }
    let mut out: Vec<DiscoveredLamp> = by_addr.into_values().collect();
    // strongest first, unknown signal last)
    out.sort_by_key(|l| std::cmp::Reverse(l.rssi.unwrap_or(i16::MIN)));
    out
}

/// Sort key that puts `None` RSSI below any real reading.
fn strength(lamp: &DiscoveredLamp) -> i32 {
    lamp.rssi.map_or(i32::MIN, i32::from)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lamp(addr: &str, rssi: Option<i16>) -> DiscoveredLamp {
        DiscoveredLamp {
            addr: addr.to_owned(),
            name: "ELK-BLEDOM_x".to_owned(),
            rssi,
        }
    }

    #[test]
    fn rssi_gate() {
        assert!(passes_rssi(Some(-50), None));
        assert!(passes_rssi(None, None));
        assert!(passes_rssi(Some(-50), Some(-70)));
        assert!(!passes_rssi(Some(-80), Some(-70)));
        // без сигнала через фильтр не пролезть)
        assert!(!passes_rssi(None, Some(-70)));
    }

    #[test]
    fn dedup_keeps_strongest_and_sorts() {
        let out = dedup_sort(vec![
            lamp("AA", Some(-80)),
            lamp("AA", Some(-50)),
            lamp("BB", None),
            lamp("CC", Some(-60)),
        ]);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].addr, "AA");
        assert_eq!(out[0].rssi, Some(-50));
        assert_eq!(out[1].addr, "CC");
        // unknown signal last)
        assert_eq!(out[2].addr, "BB");
    }
}
