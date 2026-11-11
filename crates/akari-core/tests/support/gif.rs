/// What a GIF from `animated` is made of.
pub struct Shape {
    pub width: u16,
    pub height: u16,
    pub frames: usize,
    /// The file's size, to the byte.
    pub bytes: usize,
}

const TRANSPARENT: u8 = 255;

/// An animated GIF that decodes, `shape.bytes` to the byte: full frames kept underneath the next
/// (disposal 1), noise on transparency after the first, 40 ms apart (the last 120), looping.
pub fn animated(shape: &Shape) -> Vec<u8> {
    let (width, height) = (usize::from(shape.width), usize::from(shape.height));
    let pixels = width * height;
    let mut bytes = b"GIF89a".to_vec();
    bytes.extend(shape.width.to_le_bytes());
    bytes.extend(shape.height.to_le_bytes());
    bytes.extend([0xf7, 0, 0]);
    for index in 0..=255u8 {
        bytes.extend([index, index.wrapping_mul(7), index.wrapping_mul(13)]);
    }
    bytes.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
    // Room for the comment and for a frame that came out heavier than the last.
    let target = shape.bytes.saturating_sub(8 * 1024);
    let mut seed = 0x2545_f491_u32;
    let mut cost = 1.5_f64;
    for frame in 0..shape.frames {
        let left = shape.frames - frame;
        let share = target.saturating_sub(bytes.len()) / left;
        let noisy = if frame == 0 {
            0
        } else {
            ((share.saturating_sub(64 + pixels / 2048)) as f64 / cost) as usize
        }
        .min(pixels);
        let mut canvas = vec![if frame == 0 { 0 } else { TRANSPARENT }; pixels];
        let start = (frame * 7 * width) % pixels;
        for offset in 0..noisy {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            canvas[(start + offset) % pixels] = ((seed >> 24) as u8).min(TRANSPARENT - 1);
        }
        let before = bytes.len();
        let transparency = u8::from(frame > 0);
        let delay = if frame + 1 == shape.frames { 12 } else { 4 };
        bytes.extend([0x21, 0xf9, 4, 0x04 | transparency, delay, 0, TRANSPARENT, 0]);
        bytes.push(0x2c);
        bytes.extend([0, 0, 0, 0]);
        bytes.extend(shape.width.to_le_bytes());
        bytes.extend(shape.height.to_le_bytes());
        bytes.push(0);
        lzw(&canvas, &mut bytes);
        if noisy > 0 {
            cost = (bytes.len() - before) as f64 / noisy as f64;
        }
    }
    let left = shape
        .bytes
        .checked_sub(bytes.len() + 1)
        .unwrap_or_else(|| panic!("the frames alone weigh more than {}", shape.bytes));
    comment(left, &mut bytes);
    bytes.push(0x3b);
    assert_eq!(bytes.len(), shape.bytes);
    bytes
}

/// `gif` with a comment of `more` bytes before its trailer: the same frames, heavier, as the
/// media proxy's copy of a GIF is.
pub fn heavier(gif: &[u8], more: usize) -> Vec<u8> {
    let mut bytes = gif[..gif.len() - 1].to_vec();
    comment(more, &mut bytes);
    bytes.push(0x3b);
    bytes
}

// A comment extension of exactly `size` bytes; nothing for 0.
fn comment(size: usize, bytes: &mut Vec<u8>) {
    if size == 0 {
        return;
    }
    assert!(size >= 5, "a comment takes at least 5 bytes");
    bytes.extend([0x21, 0xfe]);
    // Sub-blocks of a length byte and up to 255 bytes, then the terminator.
    let mut left = size - 3;
    while left > 0 {
        let block = match left {
            257 => 255,
            left => left.min(256),
        };
        bytes.push((block - 1) as u8);
        bytes.extend(std::iter::repeat_n(b'.', block - 1));
        left -= block;
    }
    bytes.push(0);
}

fn lzw(pixels: &[u8], bytes: &mut Vec<u8>) {
    const CLEAR: u32 = 256;
    const END: u32 = 257;
    let mut data = Vec::new();
    let (mut buffer, mut bits) = (0_u64, 0_u32);
    let mut width = 9;
    let mut emit = |code: u32, width: u32, data: &mut Vec<u8>| {
        buffer |= u64::from(code) << bits;
        bits += width;
        while bits >= 8 {
            data.push(buffer as u8);
            buffer >>= 8;
            bits -= 8;
        }
    };
    // Code by prefix and pixel, 0 for none; `used` lists what a clear has to forget.
    let mut table = vec![0_u16; 4096 << 8];
    let mut used = Vec::with_capacity(4096);
    let mut next = END + 1;
    emit(CLEAR, width, &mut data);
    let mut prefix = u32::from(pixels[0]);
    for &pixel in &pixels[1..] {
        let key = (prefix << 8 | u32::from(pixel)) as usize;
        if table[key] != 0 {
            prefix = u32::from(table[key]);
            continue;
        }
        emit(prefix, width, &mut data);
        if next == 4096 {
            emit(CLEAR, width, &mut data);
            for key in used.drain(..) {
                table[key] = 0;
            }
            next = END + 1;
            width = 9;
        } else {
            if next >= 1 << width {
                width += 1;
            }
            table[key] = next as u16;
            used.push(key);
            next += 1;
        }
        prefix = u32::from(pixel);
    }
    emit(prefix, width, &mut data);
    // A decoder adds a code for the last one too, and reads END at the width that gives.
    if next >= 1 << width && width < 12 {
        width += 1;
    }
    emit(END, width, &mut data);
    // Flushes what's left of the last byte.
    emit(0, 7, &mut data);
    bytes.push(8);
    for block in data.chunks(255) {
        bytes.push(block.len() as u8);
        bytes.extend(block);
    }
    bytes.push(0);
}
