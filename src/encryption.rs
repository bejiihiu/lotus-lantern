//! XOR cipher for `ELK-*` devices, ported from `encryption.go`
//! (`EncryptionDecryptionKt`).
//!
//! Applies **only** when the device name contains the literal `ELK-*`
//! marker **and** the command byte is in `{1, 3, 4}`. Stock `ELK-BLEDOM`
//! clones skip it entirely.
//!
//! Safety note: the transform only scrambles the 9 protocol bytes into the
//! 21-byte form the firmware expects. It never changes *what* the command
//! means — a brightness-80 stays a brightness-80.

use rand::Rng;

/// Keystream preset from the Kotlin source.
pub const PRESET_KEY: [u8; 16] = [
    0x2A, 0x7F, 0xC1, 0x94, 0x33, 0xDE, 0x45, 0xE0, 0x8B, 0x11, 0x5C, 0xA6, 0x09, 0xF2, 0x7D, 0xB8,
];

/// `true` for the commands the firmware expects encrypted on `ELK-*` lamps.
#[inline]
#[must_use]
pub const fn is_encryptable_cmd(cmd: u8) -> bool {
    cmd == 1 || cmd == 3 || cmd == 4
}

fn generate_keystream(random: &[u8; 12], keystream: &mut [u8; 9]) {
    for (i, ks) in keystream.iter_mut().enumerate() {
        // &0xFF masking is the Kotlin semantics; u16 holds every
        // intermediate, the final byte is the low 8 bits by construction)
        #[allow(clippy::cast_possible_truncation)]
        let byte = {
            let a = (u16::from(random[i]) * 27) & 0xFF;
            let b = (u16::from(random[(i + 3) % 12]) + 55) & 0xFF;
            let c = (u16::from(random[(i + 7) % 12]) >> 2) & 0xFF;
            #[allow(clippy::cast_possible_truncation)]
            let d = (i * 85) & 0xFF;
            (a ^ b ^ c ^ d as u16) as u8
        };
        *ks = byte;
    }
}

fn xor_random(random: &[u8; 12], out: &mut [u8; 12]) {
    for (i, o) in out.iter_mut().enumerate() {
        *o = random[i] ^ PRESET_KEY[i % PRESET_KEY.len()];
    }
}

/// Encrypt `plaintext` (already header-swapped to `AA … 55`) into
/// `ciphertext` using the given 12 random bytes. Zero-copy core, fully
/// deterministic — tests pin exact vectors.
pub fn encrypt_with_random(plaintext: &[u8; 9], random: &[u8; 12], ciphertext: &mut [u8; 21]) {
    let mut ks = [0u8; 9];
    generate_keystream(random, &mut ks);
    for i in 0..9 {
        ciphertext[i] = plaintext[i] ^ ks[i];
    }
    let mut rnd = [0u8; 12];
    xor_random(random, &mut rnd);
    ciphertext[9..21].copy_from_slice(&rnd);
}

/// Encrypt with fresh OS randomness (production path).
pub fn encrypt_into(plaintext: &[u8; 9], ciphertext: &mut [u8; 21]) {
    let mut random = [0u8; 12];
    rand::rng().fill_bytes(&mut random);
    encrypt_with_random(plaintext, &random, ciphertext);
}

/// Swap the envelope to the pre-encryption form (`7E … EF` → `AA … 55`).
#[inline]
#[must_use]
pub const fn header_swapped(mut frame: [u8; 9]) -> [u8; 9] {
    frame[0] = 0xAA;
    frame[8] = 0x55;
    frame
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encryptable_set_matches_go() {
        for cmd in [1u8, 3, 4] {
            assert!(is_encryptable_cmd(cmd));
        }
        for cmd in [0u8, 2, 5, 6, 7, 0x76, 0x81, 0x82, 0x83] {
            assert!(!is_encryptable_cmd(cmd));
        }
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn keystream_and_xor_match_reference() {
        // random 0..12, verified by hand from the Kotlin formula
        // (e.g. i=0: 0^58^1^0=59; i=2: (54^60)^2^170=162)
        let random: [u8; 12] = core::array::from_fn(|i| i as u8);
        let mut ks = [0u8; 9];
        generate_keystream(&random, &mut ks);
        assert_eq!(ks, [59, 119, 162, 145, 4, 17, 28, 175, 50]);

        let mut rnd = [0u8; 12];
        xor_random(&random, &mut rnd);
        let mut expected = [0u8; 12];
        for (i, e) in expected.iter_mut().enumerate() {
            *e = (i as u8) ^ PRESET_KEY[i % 16];
        }
        assert_eq!(rnd, expected);
    }

    #[test]
    #[allow(clippy::cast_possible_truncation)]
    fn roundtrip_recovers_plaintext() {
        let plain = crate::frame::brightness_frame(180, 0);
        let swapped = header_swapped(plain);
        let random: [u8; 12] = core::array::from_fn(|i| (i * 17 + 3) as u8);
        let mut cipher = [0u8; 21];
        encrypt_with_random(&swapped, &random, &mut cipher);
        assert_eq!(cipher.len(), 21);
        // first 9 bytes are keystream-masked, last 12 carry the masked random)
        let mut ks = [0u8; 9];
        generate_keystream(&random, &mut ks);
        for i in 0..9 {
            assert_eq!(cipher[i] ^ ks[i], swapped[i]);
        }
    }

    #[test]
    fn header_swap_only_touches_envelope() {
        let f = crate::frame::color_rgb_frame(1, 2, 3);
        let s = header_swapped(f);
        assert_eq!((s[0], s[8]), (0xAA, 0x55));
        assert_eq!(&s[1..8], &f[1..8]);
    }
}
