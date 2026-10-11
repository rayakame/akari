use std::io;

use zstd::stream::raw::{Decoder, InBuffer, Operation as _, OutBuffer};

// Most messages are a few KiB; a buffer that grew for READY shrinks back to this.
const RETAINED: usize = 256 * 1024;
const INITIAL: usize = 16 * 1024;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DecompressError {
    #[error("a gateway message is larger than {limit} bytes")]
    TooLarge { limit: usize },
    #[error("zstd failed")]
    Zstd(#[source] io::Error),
}

pub(crate) struct ZstdStream {
    decoder: Decoder<'static>,
    buffer: Vec<u8>,
    limit: usize,
    window_logged: bool,
}

const MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

// RFC 8878 3.1.1.1: the history a decoder keeps for the frame, which Discord chooses.
pub(crate) fn window_size(frame: &[u8]) -> Option<u64> {
    let (magic, rest) = frame.split_first_chunk::<4>()?;
    if *magic != MAGIC {
        return None;
    }
    let (&descriptor, rest) = rest.split_first()?;
    if descriptor & 0x20 == 0 {
        let &window = rest.first()?;
        let base = 1u64 << (10 + u32::from(window >> 3));
        return Some(base + base / 8 * u64::from(window & 7));
    }
    let dictionary = match descriptor & 3 {
        0 => 0,
        1 => 1,
        2 => 2,
        _ => 4,
    };
    let content = match descriptor >> 6 {
        0 => 1,
        1 => 2,
        2 => 4,
        _ => 8,
    };
    let field = rest.get(dictionary..dictionary + content)?;
    let mut bytes = [0u8; 8];
    bytes[..content].copy_from_slice(field);
    let size = u64::from_le_bytes(bytes);
    Some(if content == 2 { size + 256 } else { size })
}

impl ZstdStream {
    pub(crate) fn new(limit: usize) -> Result<Self, DecompressError> {
        Ok(Self {
            decoder: Decoder::new().map_err(DecompressError::Zstd)?,
            buffer: Vec::with_capacity(INITIAL.min(limit.saturating_add(1))),
            limit,
            window_logged: false,
        })
    }

    // After an error the stream is out of step with Discord's; drop the connection.
    pub(crate) fn decompress<R>(
        &mut self,
        message: &[u8],
        read: impl FnOnce(&[u8]) -> R,
    ) -> Result<R, DecompressError> {
        if !self.window_logged {
            self.window_logged = true;
            if let Some(size) = window_size(message) {
                tracing::debug!(size_kib = size / 1024, "zstd window");
            }
        }
        self.buffer.clear();
        let result = self.fill(message).map(|()| read(&self.buffer));
        if self.buffer.capacity() > RETAINED {
            self.buffer.clear();
            self.buffer.shrink_to(RETAINED);
        }
        result
    }

    fn fill(&mut self, message: &[u8]) -> Result<(), DecompressError> {
        let mut input = InBuffer::around(message);
        loop {
            if self.buffer.len() == self.buffer.capacity() {
                let room = self.limit.saturating_add(1) - self.buffer.len();
                let grow = self.buffer.capacity().max(INITIAL).min(room);
                self.buffer.reserve_exact(grow);
            }
            let position = self.buffer.len();
            {
                let mut output = OutBuffer::around_pos(&mut self.buffer, position);
                self.decoder
                    .run(&mut input, &mut output)
                    .map_err(DecompressError::Zstd)?;
            }
            if self.buffer.len() > self.limit {
                return Err(DecompressError::TooLarge { limit: self.limit });
            }
            // zstd has flushed everything it holds once it stops short of a full buffer.
            if input.pos() == message.len() && self.buffer.len() < self.buffer.capacity() {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
pub(crate) struct ZstdCompressor(zstd::stream::raw::Encoder<'static>);

#[cfg(test)]
impl ZstdCompressor {
    pub(crate) fn new() -> Self {
        Self(zstd::stream::raw::Encoder::new(3).unwrap())
    }

    pub(crate) fn message(&mut self, data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len() / 2 + 64);
        let mut input = InBuffer::around(data);
        while input.pos() < data.len() {
            Self::grow(&mut out);
            let position = out.len();
            self.0
                .run(&mut input, &mut OutBuffer::around_pos(&mut out, position))
                .unwrap();
        }
        loop {
            Self::grow(&mut out);
            let position = out.len();
            if self
                .0
                .flush(&mut OutBuffer::around_pos(&mut out, position))
                .unwrap()
                == 0
            {
                return out;
            }
        }
    }

    fn grow(out: &mut Vec<u8>) {
        if out.len() == out.capacity() {
            out.reserve(out.capacity().max(1024));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame_with_window_log(log: u32) -> Vec<u8> {
        use zstd::stream::raw::CParameter;
        let mut encoder = zstd::stream::raw::Encoder::new(3).unwrap();
        encoder.set_parameter(CParameter::WindowLog(log)).unwrap();
        ZstdCompressor(encoder).message(&messages(1, 4096)[0])
    }

    #[test]
    fn the_window_size_is_read_from_the_frame_header() {
        for log in [17, 20, 23] {
            assert_eq!(
                window_size(&frame_with_window_log(log)),
                Some(1 << log),
                "{log}"
            );
        }
        assert_eq!(window_size(b"not zstd"), None);
        assert_eq!(window_size(&[0x28, 0xb5, 0x2f, 0xfd]), None);
    }

    #[test]
    fn a_window_descriptor_with_a_mantissa_adds_eighths() {
        // Exponent 10 (2^20), mantissa 3: 2^20 + 3 × 2^17.
        let header = [0x28, 0xb5, 0x2f, 0xfd, 0x00, (10 << 3) | 3];
        assert_eq!(window_size(&header), Some((1 << 20) + 3 * (1 << 17)));
    }

    #[test]
    fn a_single_segment_frame_uses_its_content_size() {
        // Single segment, a 1-byte content size of 200, no dictionary.
        let header = [0x28, 0xb5, 0x2f, 0xfd, 0x20, 200];
        assert_eq!(window_size(&header), Some(200));
    }

    fn messages(count: usize, size: usize) -> Vec<Vec<u8>> {
        (0..count)
            .map(|n| {
                let mut json = format!("{{\"op\":0,\"s\":{n},\"t\":\"TEST\",\"d\":\"");
                while json.len() < size {
                    let filler = format!("entry-{n}-{} ", json.len());
                    json.push_str(&filler);
                }
                json.push_str("\"}");
                json.into_bytes()
            })
            .collect()
    }

    #[test]
    fn messages_share_one_stream() {
        let mut compressor = ZstdCompressor::new();
        let mut stream = ZstdStream::new(1024 * 1024).unwrap();

        for message in messages(5, 2000) {
            let compressed = compressor.message(&message);
            let out = stream.decompress(&compressed, <[u8]>::to_vec).unwrap();
            assert_eq!(out, message);
        }
    }

    #[test]
    fn a_later_message_needs_the_connections_context() {
        let mut compressor = ZstdCompressor::new();
        let all = messages(2, 2000);
        compressor.message(&all[0]);
        let second = compressor.message(&all[1]);

        let mut fresh = ZstdStream::new(1024 * 1024).unwrap();

        assert!(matches!(
            fresh.decompress(&second, <[u8]>::to_vec),
            Err(DecompressError::Zstd(_))
        ));
    }

    #[test]
    fn a_large_message_grows_the_buffer_and_it_shrinks_back() {
        let mut compressor = ZstdCompressor::new();
        let mut stream = ZstdStream::new(8 * 1024 * 1024).unwrap();
        let large = messages(1, 3 * 1024 * 1024).remove(0);
        let small = messages(1, 100).remove(0);

        let out = stream
            .decompress(&compressor.message(&large), <[u8]>::to_vec)
            .unwrap();
        assert_eq!(out, large);
        assert!(stream.buffer.capacity() <= RETAINED);

        let out = stream
            .decompress(&compressor.message(&small), <[u8]>::to_vec)
            .unwrap();
        assert_eq!(out, small);
    }

    #[test]
    fn output_over_the_limit_is_a_typed_error() {
        let message = vec![b'a'; 1001];

        let mut over = ZstdStream::new(1000).unwrap();
        let result = over.decompress(&ZstdCompressor::new().message(&message), |_| ());
        assert!(matches!(
            result,
            Err(DecompressError::TooLarge { limit: 1000 })
        ));

        let mut exact = ZstdStream::new(1001).unwrap();
        let out = exact
            .decompress(&ZstdCompressor::new().message(&message), <[u8]>::to_vec)
            .unwrap();
        assert_eq!(out.len(), 1001);
    }

    #[test]
    fn corrupt_input_is_an_error() {
        let mut stream = ZstdStream::new(1024).unwrap();

        let result = stream.decompress(b"definitely not zstd", |_| ());

        assert!(matches!(result, Err(DecompressError::Zstd(_))));
    }
}
