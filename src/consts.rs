//! GATT ids + device-name filters, ported from `lamp.go` / `Global.java`.
//!
//! Service `0xFFF0`, write characteristic `0xFFF3`
//! (`0000fffX-0000-1000-8000-00805f9b34fb`).

use uuid::Uuid;

/// GATT service hosting the write characteristic.
pub const SERVICE_UUID: Uuid = Uuid::from_u128(0x0000_fff0_0000_1000_8000_0080_5f9b_34fb);
/// GATT characteristic every command frame is written to.
pub const WRITE_CHAR_UUID: Uuid = Uuid::from_u128(0x0000_fff3_0000_1000_8000_0080_5f9b_34fb);

/// Most common clone prefix.
pub const NAME_FILTER: &str = "ELK-";
/// Wavy variant, same protocol.
pub const NAME_WAVY_FILTER: &str = "ELK~";
/// Strip variant, untested but supported.
pub const NAME_LED_LIGHT_STRIP: &str = "LED LIGHT STRIP";
/// New-strength variant, untested but supported.
pub const NAME_NEW_STRENGTH: &str = "XSL-";
/// Literal marker (asterisk included) enabling the XOR cipher.
pub const ENCRYPTION_MARKER: &str = "ELK-*";

/// All supported name prefixes, checked in order.
pub const NAME_PREFIXES: [&str; 4] = [
    NAME_FILTER,
    NAME_WAVY_FILTER,
    NAME_LED_LIGHT_STRIP,
    NAME_NEW_STRENGTH,
];

/// `true` when the device name contains the literal `ELK-*` marker
/// (`BluetoothLEService.isEncryptedDevice`). Only then do commands
/// with `CMD ∈ {1, 3, 4}` get the 9 → 21 byte XOR treatment.
#[inline]
#[must_use]
pub const fn is_encrypted_device(name: &str) -> bool {
    // str::contains is not const, so loop over bytes manually)
    let (hay, needle) = (name.as_bytes(), ENCRYPTION_MARKER.as_bytes());
    if needle.len() > hay.len() {
        return false;
    }
    let mut i = 0;
    while i + needle.len() <= hay.len() {
        let mut j = 0;
        while j < needle.len() && hay[i + j] == needle[j] {
            j += 1;
        }
        if j == needle.len() {
            return true;
        }
        i += 1;
    }
    false
}

/// `true` when the advertised name matches a known lamp prefix.
#[inline]
#[must_use]
pub fn is_supported_name(name: &str) -> bool {
    NAME_PREFIXES.iter().any(|p| name.starts_with(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypted_marker_needs_literal_asterisk() {
        assert!(is_encrypted_device("ELK-*ABC"));
        assert!(!is_encrypted_device("ELK-BLEDOM123"));
        assert!(!is_encrypted_device(""));
    }

    #[test]
    fn supported_prefixes() {
        for name in ["ELK-BLEDOM_123", "ELK~wavy", "LED LIGHT STRIP_1", "XSL-99"] {
            assert!(is_supported_name(name), "{name}");
        }
        assert!(!is_supported_name("Hue-Go"));
        assert!(!is_supported_name(""));
    }

    #[test]
    fn uuids_match_protocol() {
        assert_eq!(
            SERVICE_UUID.hyphenated().to_string(),
            "0000fff0-0000-1000-8000-00805f9b34fb"
        );
        assert_eq!(
            WRITE_CHAR_UUID.hyphenated().to_string(),
            "0000fff3-0000-1000-8000-00805f9b34fb"
        );
    }
}
