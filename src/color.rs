//! Color helpers, ported from `Utils.newColor` in `utils.go`.
//!
//! `newColor(brightness, color)` keeps the hue/saturation of a packed
//! `0xRRGGBB` color and swaps the V channel for `brightness`. Used by the
//! music-react path. Pure math — no BLE, no hardware risk.

// single-letter channel names are the domain language here (r/g/b/h/s/v),
// same as the Java original — not laziness.

/// HSV (`h` 0..=360, `s`/`v` 0..=255) to RGB channels.
///
/// integer math, no float — результат сразу годится в `set_color_rgb`.
/// серая зона (`s == 0`) даёт `v, v, v` без оглядки на `h`.
#[allow(clippy::many_single_char_names, clippy::cast_possible_truncation)]
#[must_use]
pub fn hsv_to_rgb(h: u16, s: u8, v: u8) -> (u8, u8, u8) {
    if s == 0 {
        return (v, v, v);
    }
    let h = h % 360;
    let region = h / 60;
    let rem = h % 60;
    // ramp up/down внутри сектора, всё в u32 чтоб не переполниться)
    let (s32, v32, rem32) = (u32::from(s), u32::from(v), u32::from(rem));
    let p = ((v32 * (255 - s32)) / 255) as u8;
    let q = ((v32 * (255 - (s32 * rem32) / 60)) / 255) as u8;
    let t = ((v32 * (255 - (s32 * (60 - rem32)) / 60)) / 255) as u8;
    match region {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    }
}

/// Шаги плавного перехода яркости `from` → `to` за `steps` шагов.
///
/// чистая математика для `fade_brightness` — крайние точки включены,
/// `steps == 0` даёт просто `[to]`.
#[allow(clippy::cast_sign_loss)]
#[must_use]
pub fn brightness_steps(from: u8, to: u8, steps: u8) -> Vec<u8> {
    if steps == 0 {
        return vec![to];
    }
    // шаг ≤ 255, так что i32 хватает с запасом; касты u8->i32 точные)
    let n = i32::from(steps);
    let (f, t) = (i32::from(from), i32::from(to));
    (0..=n)
        .map(|i| {
            let v = f + (t - f) * i / n;
            v.clamp(0, 255) as u8
        })
        .collect()
}

/// Replace the V channel of `rgb` (`0xRRGGBB`) with `brightness`.
///
/// Returns packed `0xAARRGGBB` like the Java original.
#[allow(clippy::many_single_char_names, clippy::cast_possible_truncation)]
#[must_use]
pub fn blend_brightness(brightness: u8, rgb: u32) -> u32 {
    let r = f64::from((rgb >> 16) & 0xFF);
    let g = f64::from((rgb >> 8) & 0xFF);
    let b = f64::from(rgb & 0xFF);

    let max_v = r.max(g).max(b);
    let min_v = r.min(g).min(b);
    let delta = max_v - min_v;

    // float equality mirrors the Java branch structure exactly;
    // values come from the same discrete byte grid, epsilon would diverge)
    #[allow(clippy::float_cmp)]
    let h = if delta == 0.0 {
        0.0
    } else if max_v == r {
        60.0 * rem((g - b) / delta, 6.0)
    } else if max_v == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    let s = if max_v == 0.0 { 0.0 } else { delta / max_v };

    let v = f64::from(brightness);
    let c = v * s;
    let x = c * (1.0 - (rem(h / 60.0, 2.0) - 1.0).abs());
    let m = v - c;
    let (rr, gg, bb) = match h {
        h if h < 60.0 => (c, x, 0.0),
        h if h < 120.0 => (x, c, 0.0),
        h if h < 180.0 => (0.0, c, x),
        h if h < 240.0 => (0.0, x, c),
        h if h < 300.0 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let (r8, g8, b8) = (
        clamp((rr + m) as i32, 0, 255),
        clamp((gg + m) as i32, 0, 255),
        clamp((bb + m) as i32, 0, 255),
    );
    (0xFF << 24) | (r8 << 16) | (g8 << 8) | b8
}

fn rem(a: f64, b: f64) -> f64 {
    // mirrors Go's trunc-based mod, keeps negatives wrapping into [0, b).
    // trunc toward zero is what the Java/Go originals do, hence the cast)
    #[allow(clippy::cast_possible_truncation)]
    let r = a - f64::from((a / b) as i32) * b;
    if r < 0.0 {
        r + b
    } else {
        r
    }
}

// inputs are clamped to [lo, hi] with lo/hi >= 0 by every caller,
// so the i32->u32 step never goes negative)
#[allow(clippy::cast_sign_loss)]
const fn clamp(v: i32, lo: i32, hi: i32) -> u32 {
    if v < lo {
        lo as u32
    } else if v > hi {
        hi as u32
    } else {
        v as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn black_stays_black() {
        assert_eq!(blend_brightness(0, 0x00FF_0000), 0xFF00_0000);
    }

    #[test]
    fn gray_scales_with_brightness() {
        // achromatic input: hue=0, sat=0 -> output is brightness gray)
        assert_eq!(blend_brightness(255, 0x0080_8080), 0xFFFF_FFFF);
        assert_eq!(blend_brightness(128, 0x0080_8080), 0xFF80_8080);
    }

    #[test]
    fn output_always_opaque_and_bounded() {
        for (bri, col) in [
            (1, 0x00FF_0000),
            (200, 0x0000_FF00),
            (80, 0x0000_00FF),
            (255, 0x0012_3456),
        ] {
            let out = blend_brightness(bri, col);
            assert_eq!(out >> 24, 0xFF, "alpha stays opaque");
            // V-channel swap: no channel can exceed the new brightness
            for shift in [16, 8, 0] {
                assert!(((out >> shift) & 0xFF) <= u32::from(bri));
            }
        }
    }

    #[test]
    fn hsv_primaries() {
        assert_eq!(hsv_to_rgb(0, 255, 255), (255, 0, 0));
        assert_eq!(hsv_to_rgb(120, 255, 255), (0, 255, 0));
        assert_eq!(hsv_to_rgb(240, 255, 255), (0, 0, 255));
    }

    #[test]
    fn hsv_gray_ignores_hue() {
        assert_eq!(hsv_to_rgb(123, 0, 77), (77, 77, 77));
        assert_eq!(hsv_to_rgb(0, 0, 0), (0, 0, 0));
    }

    #[test]
    fn hsv_wraps_360_to_red() {
        assert_eq!(hsv_to_rgb(360, 255, 255), hsv_to_rgb(0, 255, 255));
    }

    #[test]
    fn brightness_steps_include_endpoints() {
        assert_eq!(brightness_steps(10, 20, 2), vec![10, 15, 20]);
        assert_eq!(brightness_steps(20, 10, 2), vec![20, 15, 10]);
        assert_eq!(brightness_steps(5, 99, 0), vec![99]);
    }
}
