// Ported from ThumbHash (https://github.com/evanw/thumbhash), with Discord's parameters.
// Copyright (c) 2023 Evan Wallace. MIT License; the text is in THIRD-PARTY-NOTICES.md.

use std::f64::consts::PI;

use base64::Engine as _;
use base64::alphabet::STANDARD;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};

/// A thumbhash decoded as Discord's client decodes it: at most 32 pixels on the longest side,
/// for drawing under an image until it loads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, row by row from the top.
    pub rgba: Vec<u8>,
}

// atob takes the hash with or without padding.
const BASE64: GeneralPurpose = GeneralPurpose::new(
    &STANDARD,
    GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
);

// Discord's softening: luma only up to the first harmonics, chroma a little stronger.
const DETAIL: u32 = 1;
const POP: f64 = 1.1;

/// Decodes an attachment's `placeholder` (base64; only `placeholder_version` 1). `None` when it
/// isn't a thumbhash.
pub fn placeholder(thumbhash: &str) -> Option<Placeholder> {
    let hash = BASE64.decode(thumbhash.trim()).ok()?;
    decode(&hash)
}

// Discord's decoder (canary 649848, a worker in 792086589bac0680.js), thumbhash's with `detail`,
// `pop` and the luma AC halved; bytes truncated as a Uint8Array stores them.
fn decode(hash: &[u8]) -> Option<Placeholder> {
    let byte = |index: usize| hash.get(index).copied().map(u32::from);
    let header = byte(0)? | (byte(1)? << 8) | (byte(2)? << 16);
    let more = byte(3)? | (byte(4)? << 8);
    let l_dc = f64::from(header & 63) / 63.0;
    let p_dc = f64::from((header >> 6) & 63) / 31.5 - 1.0;
    let q_dc = f64::from((header >> 12) & 63) / 31.5 - 1.0;
    let l_scale = f64::from((header >> 18) & 31) / 31.0 / 2.0;
    let has_alpha = header >> 23 != 0;
    let p_scale = f64::from((more >> 3) & 63) / 63.0 * POP;
    let q_scale = f64::from((more >> 9) & 63) / 63.0 * POP;
    let landscape = more >> 15 != 0;
    let long = if has_alpha { 5 } else { 7 };
    let lx = if landscape { long } else { more & 7 }.max(3);
    let ly = if landscape { more & 7 } else { long }.max(3);
    let (a_dc, a_scale) = if has_alpha {
        let alpha = byte(5)?;
        (f64::from(alpha & 15) / 15.0, f64::from(alpha >> 4) / 15.0)
    } else {
        (1.0, 0.0)
    };

    let mut nibbles = Nibbles {
        hash,
        start: if has_alpha { 6 } else { 5 },
        index: 0,
    };
    let l_ac = nibbles.channel(lx, ly, l_scale)?;
    let p_ac = nibbles.channel(3, 3, p_scale)?;
    let q_ac = nibbles.channel(3, 3, q_scale)?;
    let a_ac = if has_alpha {
        nibbles.channel(5, 5, a_scale)?
    } else {
        Vec::new()
    };

    let ratio = aspect_ratio(hash)?;
    let width = (if ratio > 1.0 { 32.0 } else { 32.0 * ratio }).round();
    let height = (if ratio > 1.0 { 32.0 / ratio } else { 32.0 }).round();
    if !(1.0..=32.0).contains(&width) || !(1.0..=32.0).contains(&height) {
        return None;
    }
    let (width, height) = (width as u32, height as u32);
    let min_terms = if has_alpha { 5 } else { 3 };
    let mut fx = vec![0.0; lx.max(min_terms) as usize];
    let mut fy = vec![0.0; ly.max(min_terms) as usize];
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let (mut l, mut p, mut q, mut a) = (l_dc, p_dc, q_dc, a_dc);
            for (cx, value) in fx.iter_mut().enumerate() {
                *value = (PI / f64::from(width) * (f64::from(x) + 0.5) * cx as f64).cos();
            }
            for (cy, value) in fy.iter_mut().enumerate() {
                *value = (PI / f64::from(height) * (f64::from(y) + 0.5) * cy as f64).cos();
            }
            let mut j = 0;
            for cy in 0..ly {
                let fy2 = 2.0 * fy[cy as usize];
                let mut cx = u32::from(cy == 0);
                while cx * ly < lx * (ly - cy) {
                    if cx <= DETAIL && cy <= DETAIL {
                        l += l_ac[j] * fx[cx as usize] * fy2;
                    }
                    cx += 1;
                    j += 1;
                }
            }
            let mut j = 0;
            for (cy, fy) in fy.iter().take(3).enumerate() {
                for fx in &fx[usize::from(cy == 0)..3 - cy] {
                    let f = fx * 2.0 * fy;
                    p += p_ac[j] * f;
                    q += q_ac[j] * f;
                    j += 1;
                }
            }
            if has_alpha {
                let mut j = 0;
                for (cy, fy) in fy.iter().take(5).enumerate() {
                    for fx in &fx[usize::from(cy == 0)..5 - cy] {
                        a += a_ac[j] * fx * 2.0 * fy;
                        j += 1;
                    }
                }
            }
            let b = l - 2.0 / 3.0 * p;
            let r = (3.0 * l - b + q) / 2.0;
            let g = r - q;
            rgba.extend([r, g, b, a].map(channel_byte));
        }
    }
    Some(Placeholder {
        width,
        height,
        rgba,
    })
}

fn channel_byte(value: f64) -> u8 {
    (255.0 * value.min(1.0)).max(0.0) as u8
}

// From the header alone, before the minimum of three terms.
fn aspect_ratio(hash: &[u8]) -> Option<f64> {
    let side = f64::from(hash.get(3)? & 7);
    let has_alpha = hash.get(2)? & 128 != 0;
    let landscape = hash.get(4)? & 128 != 0;
    let long = if has_alpha { 5.0 } else { 7.0 };
    Some(if landscape { long / side } else { side / long })
}

struct Nibbles<'a> {
    hash: &'a [u8],
    start: usize,
    index: usize,
}

impl Nibbles<'_> {
    fn channel(&mut self, nx: u32, ny: u32, scale: f64) -> Option<Vec<f64>> {
        let mut ac = Vec::new();
        for cy in 0..ny {
            let mut cx = u32::from(cy == 0);
            while cx * ny < nx * (ny - cy) {
                let byte = self.hash.get(self.start + (self.index >> 1))?;
                let nibble = (byte >> ((self.index & 1) << 2)) & 15;
                ac.push((f64::from(nibble) / 7.5 - 1.0) * scale);
                self.index += 1;
                cx += 1;
            }
        }
        Some(ac)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Vector {
        hash: &'static str,
        width: u32,
        height: u32,
        first: [u8; 4],
        middle: [u8; 4],
        last: [u8; 4],
        sum: u64,
        weighted: u64,
    }

    // Akari's own hashes, decoded by Discord's function in its worker (run under node).
    const VECTORS: [Vector; 5] = [
        Vector {
            hash: "6BdS9LIx6ztgCNvPNYoH8Kd5wr5s46fPJx0",
            width: 32,
            height: 18,
            first: [0, 0, 26, 255],
            middle: [201, 255, 0, 255],
            last: [255, 255, 255, 255],
            sum: 370_216,
            weighted: 496_469_116,
        },
        Vector {
            hash: "Fkp9/RUek1pgDF3BNMPLZQ70awbsJojEN3k",
            width: 23,
            height: 32,
            first: [255, 255, 0, 255],
            middle: [138, 255, 0, 255],
            last: [0, 0, 0, 255],
            sum: 480_189,
            weighted: 693_457_994,
        },
        Vector {
            hash: "tyKzo9jJPEo6g+BF+VwazQn1dcOQpwmQEPA",
            width: 32,
            height: 19,
            first: [255, 91, 254, 0],
            middle: [50, 255, 255, 255],
            last: [255, 0, 38, 0],
            sum: 408_538,
            weighted: 470_023_200,
        },
        Vector {
            hash: "RfjllWV/AcRidSQ/oNjJG+hClkSTqe6b9Fs",
            width: 32,
            height: 32,
            first: [0, 213, 0, 255],
            middle: [0, 0, 255, 249],
            last: [255, 255, 0, 255],
            sum: 446_695,
            weighted: 1_091_019_193,
        },
        Vector {
            hash: "HvgRM4rzbHNV25kBMH7SDIWI6HzQEtnrpaM",
            width: 32,
            height: 14,
            first: [53, 54, 131, 255],
            middle: [132, 162, 75, 255],
            last: [131, 109, 182, 255],
            sum: 276_781,
            weighted: 254_252_938,
        },
    ];

    fn pixel(placeholder: &Placeholder, index: usize) -> [u8; 4] {
        let start = index * 4;
        placeholder.rgba[start..start + 4].try_into().unwrap()
    }

    #[test]
    fn decodes_as_discords_client_does() {
        for vector in VECTORS {
            let decoded = placeholder(vector.hash).unwrap();
            let (width, height) = (decoded.width as usize, decoded.height as usize);

            assert_eq!(
                (decoded.width, decoded.height),
                (vector.width, vector.height),
                "{}",
                vector.hash
            );
            assert_eq!(decoded.rgba.len(), width * height * 4);
            assert_eq!(pixel(&decoded, 0), vector.first, "{}", vector.hash);
            assert_eq!(
                pixel(&decoded, height / 2 * width + width / 2),
                vector.middle,
                "{}",
                vector.hash
            );
            assert_eq!(
                pixel(&decoded, width * height - 1),
                vector.last,
                "{}",
                vector.hash
            );
            let sum: u64 = decoded.rgba.iter().map(|&byte| u64::from(byte)).sum();
            let weighted = decoded
                .rgba
                .iter()
                .enumerate()
                .fold(0u64, |total, (index, &byte)| {
                    (total + (index as u64 + 1) * u64::from(byte)) % (1 << 32)
                });
            assert_eq!(
                (sum, weighted),
                (vector.sum, vector.weighted),
                "{}",
                vector.hash
            );
        }
    }

    #[test]
    fn padding_is_optional() {
        let padded = format!("{}=", VECTORS[0].hash);

        assert_eq!(placeholder(&padded), placeholder(VECTORS[0].hash));
    }

    #[test]
    fn anything_else_is_none_never_a_panic() {
        for text in ["", "not base64!", "AAAA", "6BdS9LIx", "6BdS9LIx6ztg"] {
            assert_eq!(placeholder(text), None, "{text:?}");
        }
        // Every truncation of a real hash.
        let bytes = BASE64.decode(VECTORS[2].hash).unwrap();
        for end in 0..bytes.len() {
            let _ = decode(&bytes[..end]);
        }
        // A side of zero would make an empty or endless image.
        let mut flat = bytes.clone();
        flat[3] &= !7;
        assert_eq!(decode(&flat), None);
    }
}
