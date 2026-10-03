//! Public-API golden tests: the bytes a real strip would receive.
//!
//! These duplicate the module unit tests on purpose — they pin the *public*
//! surface, so a refactor can't silently shift a byte on the wire. If any
//! of these fail, do not flash a strip with this build.

use lotus_lantern::{
    brightness_frame, color_rgb_frame, is_encrypted_device, is_supported_name, is_well_formed,
    light_on_frame, mode_frame,
};

#[test]
fn power_and_color_bytes_are_stock_app_bytes() {
    assert_eq!(
        light_on_frame(true),
        [0x7E, 0x04, 0x04, 1, 0x00, 1, 0xFF, 0x00, 0xEF]
    );
    assert_eq!(
        color_rgb_frame(255, 0, 128),
        [0x7E, 0x07, 0x05, 0x03, 255, 0, 128, 0x10, 0xEF]
    );
    assert_eq!(
        brightness_frame(180, 0),
        [0x7E, 0x04, 0x01, 180, 0, 0xFF, 0xFF, 0x00, 0xEF]
    );
    assert_eq!(
        mode_frame(5),
        [0x7E, 0x05, 0x03, 133, 0x03, 0xFF, 0xFF, 0x00, 0xEF]
    );
}

#[test]
fn envelopes_are_well_formed() {
    for f in [
        light_on_frame(false),
        color_rgb_frame(1, 2, 3),
        brightness_frame(0, 0),
        mode_frame(0),
    ] {
        assert!(is_well_formed(&f));
    }
}

#[test]
fn name_filters_match_go_prefixes() {
    assert!(is_supported_name("ELK-BLEDOM_12345"));
    assert!(is_supported_name("LED LIGHT STRIP_1"));
    assert!(!is_supported_name("Philips Hue"));
    // encryption only for the literal ELK-* marker, like the Java side
    assert!(is_encrypted_device("ELK-*secret"));
    assert!(!is_encrypted_device("ELK-BLEDOM_12345"));
}
