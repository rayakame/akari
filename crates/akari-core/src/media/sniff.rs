// What a downloaded file is, from its first bytes: Akari keeps only images it can name.

use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, ErrorKind, Read, Seek};
use std::path::Path;

// Past any header: a JPEG's size comes after its metadata.
const HEAD: usize = 256 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Png,
    Gif,
    WebP,
    Jpeg,
}

impl Format {
    pub(crate) fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Gif => "gif",
            Self::WebP => "webp",
            Self::Jpeg => "jpg",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Inspected {
    pub(crate) format: Format,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) animated: bool,
}

pub(crate) fn inspect(bytes: &[u8]) -> Option<Inspected> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        png(bytes)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        gif(bytes)
    } else if bytes.get(..4) == Some(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        webp(bytes)
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        jpeg(bytes)
    } else {
        None
    }
}

/// What a file too heavy to hold in memory is: a GIF's blocks are walked on disk, any other
/// format is named by the file's first 256 KB. An error is a read that failed.
pub(crate) fn inspect_file(path: &Path) -> std::io::Result<Option<Inspected>> {
    let mut reader = BufReader::with_capacity(HEAD, File::open(path)?);
    let head = reader.fill_buf()?;
    if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        gif_blocks(&mut reader)
    } else {
        Ok(inspect(head))
    }
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_be_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from(u16::from_le_bytes(
        bytes.get(at..at + 2)?.try_into().ok()?,
    )))
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let part = bytes.get(at..at + 3)?;
    Some(u32::from(part[0]) | (u32::from(part[1]) << 8) | (u32::from(part[2]) << 16))
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

// IHDR first; an acTL chunk before the first IDAT makes it an APNG.
fn png(bytes: &[u8]) -> Option<Inspected> {
    if bytes.get(12..16)? != b"IHDR" {
        return None;
    }
    let (width, height) = (be32(bytes, 16)?, be32(bytes, 20)?);
    let mut at = 8;
    let mut animated = false;
    while let (Some(length), Some(kind)) = (be32(bytes, at), bytes.get(at + 4..at + 8)) {
        match kind {
            b"acTL" => {
                animated = true;
                break;
            }
            b"IDAT" | b"IEND" => break,
            _ => {}
        }
        at = at
            .checked_add(12)?
            .checked_add(usize::try_from(length).ok()?)?;
    }
    Some(Inspected {
        format: Format::Png,
        width,
        height,
        animated,
    })
}

// Counts frames by walking the blocks; a cut-off file counts what it has.
fn gif(bytes: &[u8]) -> Option<Inspected> {
    gif_blocks(&mut Cursor::new(bytes)).ok().flatten()
}

// `None` for a file that ends where it mustn't; an error for a read that failed.
fn gif_blocks(reader: &mut (impl BufRead + Seek)) -> std::io::Result<Option<Inspected>> {
    let mut header = [0; 11];
    if !filled(reader, &mut header)? {
        return Ok(None);
    }
    let (Some(width), Some(height)) = (le16(&header, 6), le16(&header, 8)) else {
        return Ok(None);
    };
    let flags = header[10];
    reader.seek_relative(2)?;
    if flags & 0x80 != 0 {
        reader.seek_relative(3 << ((flags & 7) + 1))?;
    }
    let mut frames = 0;
    while let Some(block) = next_byte(reader)? {
        match block {
            0x21 => {
                reader.seek_relative(1)?;
                if !skip_sub_blocks(reader)? {
                    return Ok(None);
                }
            }
            0x2c => {
                frames += 1;
                if frames > 1 {
                    break;
                }
                let mut descriptor = [0; 9];
                if !filled(reader, &mut descriptor)? {
                    return Ok(None);
                }
                let local = descriptor[8];
                if local & 0x80 != 0 {
                    reader.seek_relative(3 << ((local & 7) + 1))?;
                }
                reader.seek_relative(1)?;
                if !skip_sub_blocks(reader)? {
                    return Ok(None);
                }
            }
            _ => break,
        }
    }
    Ok(Some(Inspected {
        format: Format::Gif,
        width,
        height,
        animated: frames > 1,
    }))
}

// Whether `buffer` was filled before the end.
fn filled(reader: &mut impl Read, buffer: &mut [u8]) -> std::io::Result<bool> {
    match reader.read_exact(buffer) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == ErrorKind::UnexpectedEof => Ok(false),
        Err(err) => Err(err),
    }
}

fn next_byte(reader: &mut impl Read) -> std::io::Result<Option<u8>> {
    let mut byte = [0];
    Ok(filled(reader, &mut byte)?.then_some(byte[0]))
}

// Whether the blocks ended with their terminator.
fn skip_sub_blocks(reader: &mut (impl BufRead + Seek)) -> std::io::Result<bool> {
    loop {
        match next_byte(reader)? {
            None => return Ok(false),
            Some(0) => return Ok(true),
            Some(length) => reader.seek_relative(i64::from(length))?,
        }
    }
}

fn webp(bytes: &[u8]) -> Option<Inspected> {
    let (width, height, animated) = match bytes.get(12..16)? {
        b"VP8X" => {
            let flags = *bytes.get(20)?;
            (
                le24(bytes, 24)? + 1,
                le24(bytes, 27)? + 1,
                flags & 0x02 != 0,
            )
        }
        b"VP8L" => {
            if *bytes.get(20)? != 0x2f {
                return None;
            }
            let bits = le32(bytes, 21)?;
            ((bits & 0x3fff) + 1, ((bits >> 14) & 0x3fff) + 1, false)
        }
        b"VP8 " => {
            if bytes.get(23..26)? != [0x9d, 0x01, 0x2a] {
                return None;
            }
            (le16(bytes, 26)? & 0x3fff, le16(bytes, 28)? & 0x3fff, false)
        }
        _ => return None,
    };
    Some(Inspected {
        format: Format::WebP,
        width,
        height,
        animated,
    })
}

// The first start-of-frame segment has the size.
fn jpeg(bytes: &[u8]) -> Option<Inspected> {
    let mut at = 2;
    loop {
        while *bytes.get(at)? == 0xff && *bytes.get(at + 1)? == 0xff {
            at += 1;
        }
        if *bytes.get(at)? != 0xff {
            return None;
        }
        let marker = *bytes.get(at + 1)?;
        let length = usize::try_from(be16(bytes, at + 2)?).ok()?;
        if matches!(marker, 0xc0..=0xcf) && !matches!(marker, 0xc4 | 0xc8 | 0xcc) {
            return Some(Inspected {
                format: Format::Jpeg,
                width: be16(bytes, at + 7)?,
                height: be16(bytes, at + 5)?,
                animated: false,
            });
        }
        at += 2 + length;
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    // The smallest headers each format needs, with a size of 300 × 200.
    pub(crate) fn png_bytes(width: u32, height: u32, animated: bool) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        let chunk = |bytes: &mut Vec<u8>, kind: &[u8], data: &[u8]| {
            bytes.extend((data.len() as u32).to_be_bytes());
            bytes.extend(kind);
            bytes.extend(data);
            bytes.extend([0; 4]);
        };
        let mut header = width.to_be_bytes().to_vec();
        header.extend(height.to_be_bytes());
        header.extend([8, 6, 0, 0, 0]);
        chunk(&mut bytes, b"IHDR", &header);
        if animated {
            chunk(&mut bytes, b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        }
        chunk(&mut bytes, b"IDAT", &[0; 8]);
        chunk(&mut bytes, b"IEND", &[]);
        bytes
    }

    pub(crate) fn webp_bytes(width: u32, height: u32, animated: bool) -> Vec<u8> {
        let mut bytes = b"RIFF\0\0\0\0WEBPVP8X".to_vec();
        bytes.extend(10u32.to_le_bytes());
        bytes.extend([if animated { 0x12 } else { 0x10 }, 0, 0, 0]);
        bytes.extend(&(width - 1).to_le_bytes()[..3]);
        bytes.extend(&(height - 1).to_le_bytes()[..3]);
        bytes
    }

    fn gif_bytes(frames: usize) -> Vec<u8> {
        let mut bytes = b"GIF89a".to_vec();
        bytes.extend(300u16.to_le_bytes());
        bytes.extend(200u16.to_le_bytes());
        bytes.extend([0x80, 0, 0]);
        bytes.extend([0; 6]);
        for _ in 0..frames {
            bytes.extend([0x21, 0xf9, 4, 0, 10, 0, 0, 0]);
            bytes.extend([0x2c, 0, 0, 0, 0, 0x2c, 1, 0xc8, 0, 0]);
            bytes.extend([2, 2, 0x4c, 0x01, 0]);
        }
        bytes.push(0x3b);
        bytes
    }

    fn jpeg_bytes() -> Vec<u8> {
        let mut bytes = vec![0xff, 0xd8, 0xff, 0xe0, 0, 16];
        bytes.extend(b"JFIF\0\x01\x01\0\0\x01\0\x01\0\0");
        bytes.extend([0xff, 0xc0, 0, 17, 8, 0, 200, 1, 44, 3]);
        bytes.extend([0; 9]);
        bytes
    }

    #[test]
    fn reads_each_formats_size_and_motion() {
        for (bytes, format, animated) in [
            (png_bytes(300, 200, false), Format::Png, false),
            (png_bytes(300, 200, true), Format::Png, true),
            (webp_bytes(300, 200, false), Format::WebP, false),
            (webp_bytes(300, 200, true), Format::WebP, true),
            (gif_bytes(1), Format::Gif, false),
            (gif_bytes(3), Format::Gif, true),
            (jpeg_bytes(), Format::Jpeg, false),
        ] {
            assert_eq!(
                inspect(&bytes),
                Some(Inspected {
                    format,
                    width: 300,
                    height: 200,
                    animated
                }),
                "{format:?} {animated}"
            );
        }
    }

    #[test]
    fn reads_lossy_and_lossless_webp() {
        let mut lossy = b"RIFF\0\0\0\0WEBPVP8 \0\0\0\0\0\0\0\x9d\x01\x2a".to_vec();
        lossy.extend(300u16.to_le_bytes());
        lossy.extend(200u16.to_le_bytes());
        let mut lossless = b"RIFF\0\0\0\0WEBPVP8L\0\0\0\0\x2f".to_vec();
        lossless.extend((299u32 | (199 << 14)).to_le_bytes());

        for bytes in [lossy, lossless] {
            let inspected = inspect(&bytes).unwrap();
            assert_eq!((inspected.width, inspected.height), (300, 200));
        }
    }

    #[test]
    fn the_fixtures_animated_webp_is_animated() {
        let bytes = include_bytes!(
            "../../../../apps/apple/AkariKit/Sources/AkariKit/Resources/animated.webp"
        );

        let inspected = inspect(bytes).unwrap();

        assert_eq!((inspected.width, inspected.height), (32, 32));
        assert!(inspected.animated);
    }

    // A comment of `size` bytes, as sub-blocks.
    fn comment(size: usize) -> Vec<u8> {
        let mut bytes = vec![0x21, 0xfe];
        for block in vec![b'.'; size].chunks(255) {
            bytes.push(block.len() as u8);
            bytes.extend(block);
        }
        bytes.push(0);
        bytes
    }

    fn file_with(bytes: &[u8]) -> (crate::cache::tests::TempDir, std::path::PathBuf) {
        let dir = crate::cache::tests::TempDir::new();
        let path = dir.path().join("image");
        std::fs::write(&path, bytes).unwrap();
        (dir, path)
    }

    #[test]
    fn a_file_reads_as_its_bytes_do() {
        for bytes in [
            png_bytes(300, 200, true),
            webp_bytes(300, 200, true),
            gif_bytes(1),
            gif_bytes(3),
            jpeg_bytes(),
            b"<html>".to_vec(),
            Vec::new(),
        ] {
            let (_dir, path) = file_with(&bytes);

            assert_eq!(inspect_file(&path).unwrap(), inspect(&bytes), "{bytes:?}");
        }
        assert!(inspect_file(std::path::Path::new("/nowhere/at/all")).is_err());
    }

    // The second frame megabytes in: found without the file in memory.
    #[test]
    fn a_files_frames_are_found_however_far_in() {
        let frames = gif_bytes(2);
        let mut bytes = frames[..19].to_vec();
        bytes.extend(comment(3 << 20));
        let first_frame = 19 + 8 + 10 + 5;
        bytes.extend(&frames[19..first_frame]);
        bytes.extend(comment(3 << 20));
        bytes.extend(&frames[first_frame..]);
        let (_dir, path) = file_with(&bytes);

        let inspected = inspect_file(&path).unwrap().unwrap();

        assert!(inspected.animated);
        assert_eq!((inspected.width, inspected.height), (300, 200));
        assert_eq!(Some(inspected), inspect(&bytes));
    }

    #[test]
    fn a_cut_off_file_reads_as_its_bytes_do() {
        let gif = gif_bytes(3);
        for end in 0..gif.len() {
            let (_dir, path) = file_with(&gif[..end]);

            assert_eq!(inspect_file(&path).unwrap(), inspect(&gif[..end]), "{end}");
        }
    }

    // Fails every read from `after` bytes on.
    struct Failing {
        bytes: Cursor<Vec<u8>>,
        after: u64,
    }

    impl Read for Failing {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            if self.bytes.position() >= self.after {
                return Err(std::io::Error::other("the disk failed"));
            }
            let most = (self.after - self.bytes.position()) as usize;
            let most = most.min(buffer.len());
            self.bytes.read(&mut buffer[..most])
        }
    }

    impl Seek for Failing {
        fn seek(&mut self, to: std::io::SeekFrom) -> std::io::Result<u64> {
            self.bytes.seek(to)
        }
    }

    // A read that failed isn't the file's end: nothing is known about its frames.
    #[test]
    fn a_failed_read_names_nothing() {
        let gif = gif_bytes(3);
        let first_frame = 19 + 23;
        for after in [first_frame, first_frame + 4] {
            let mut reader = BufReader::with_capacity(
                8,
                Failing {
                    bytes: Cursor::new(gif.clone()),
                    after: after as u64,
                },
            );

            assert!(gif_blocks(&mut reader).is_err(), "{after}");
        }
        let mut whole = BufReader::with_capacity(
            8,
            Failing {
                bytes: Cursor::new(gif.clone()),
                after: gif.len() as u64 + 1,
            },
        );
        assert_eq!(gif_blocks(&mut whole).unwrap(), inspect(&gif));
    }

    #[test]
    fn anything_else_or_cut_short_is_none() {
        for bytes in [
            &b""[..],
            b"<html>",
            b"\x89PNG\r\n\x1a\n",
            b"RIFF\0\0\0\0WEBPVP8X",
            b"GIF89a\x2c",
            &[0xff, 0xd8, 0xff, 0xe0],
            b"\0\0\0\x1cftypavif",
        ] {
            assert_eq!(inspect(bytes), None, "{bytes:?}");
        }
        let gif = gif_bytes(3);
        let png = png_bytes(300, 200, true);
        for end in 0..gif.len().max(png.len()) {
            let _ = inspect(&gif[..end.min(gif.len())]);
            let _ = inspect(&png[..end.min(png.len())]);
        }
    }
}
