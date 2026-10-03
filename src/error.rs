//! Crate-wide error type.

use btleplug::platform::PeripheralId;
use thiserror::Error;

/// Crate-wide error type.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    /// No Bluetooth adapter was found on this machine.
    #[error("no Bluetooth adapter found")]
    NoAdapter,
    /// Scan finished without a matching lamp in range.
    #[error("no Lotus Lantern lamp found")]
    LampNotFound,
    /// A known address never showed up during the pre-connect scan.
    #[error("device not seen in scan: powered/in range? ({0})")]
    DeviceNotSeen(String),
    /// GATT discovery did not yield service `0xFFF0`.
    #[error("FFF0 service not found")]
    ServiceNotFound,
    /// Service `0xFFF0` has no `0xFFF3` write characteristic.
    #[error("FFF3 characteristic not found")]
    CharacteristicNotFound,
    /// Peripheral vanished mid-operation.
    #[error("peripheral {0:?} disappeared")]
    PeripheralGone(PeripheralId),
    /// Write failed even after the reconnect retries.
    #[error("send failed after retries")]
    SendFailed,
    /// Operation hit its deadline.
    #[error("operation timed out")]
    Timeout,
    /// Underlying btleplug failure.
    #[error("bluetooth: {0}")]
    Bluetooth(#[from] btleplug::Error),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
