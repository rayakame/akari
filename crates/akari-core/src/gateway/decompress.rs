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
}

impl ZstdStream {
    pub(crate) fn new(limit: usize) -> Result<Self, DecompressError> {
        Ok(Self {
            decoder: Decoder::new().map_err(DecompressError::Zstd)?,
            buffer: Vec::with_capacity(INITIAL.min(limit.saturating_add(1))),
            limit,
        })
    }

    // After an error the stream is out of step with Discord's; drop the connection.
    pub(crate) fn decompress<R>(
        &mut self,
        message: &[u8],
        read: impl FnOnce(&[u8]) -> R,
    ) -> Result<R, DecompressError> {
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
