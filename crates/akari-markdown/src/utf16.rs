//! UTF-8 byte offsets and UTF-16 offsets of one source, for hosts whose strings count UTF-16.

/// Converts between UTF-8 byte offsets and UTF-16 offsets of one source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Utf16Map {
    /// Each character outside ASCII: its byte offset, its UTF-16 offset and both lengths.
    wide: Box<[Wide]>,
    bytes: u32,
    units: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Wide {
    byte: u32,
    unit: u32,
    bytes: u8,
    units: u8,
}

impl Utf16Map {
    pub fn new(source: &str) -> Self {
        let mut wide = Vec::new();
        let mut unit = 0u32;
        for (byte, c) in source.char_indices() {
            if !c.is_ascii() {
                wide.push(Wide {
                    byte: byte as u32,
                    unit,
                    bytes: c.len_utf8() as u8,
                    units: c.len_utf16() as u8,
                });
            }
            unit += c.len_utf16() as u32;
        }
        Self {
            wide: wide.into(),
            bytes: source.len() as u32,
            units: unit,
        }
    }

    /// Clamped to the source; inside a character, its start.
    pub fn to_utf16(&self, byte: u32) -> u32 {
        let byte = byte.min(self.bytes);
        let i = self.wide.partition_point(|w| w.byte <= byte);
        match i.checked_sub(1).map(|i| self.wide[i]) {
            None => byte,
            Some(w) if byte < w.byte + u32::from(w.bytes) => w.unit,
            Some(w) => w.unit + u32::from(w.units) + (byte - w.byte - u32::from(w.bytes)),
        }
    }

    /// Rounds down to a character boundary; clamped to the source.
    pub fn to_byte(&self, unit: u32) -> u32 {
        let unit = unit.min(self.units);
        let i = self.wide.partition_point(|w| w.unit <= unit);
        match i.checked_sub(1).map(|i| self.wide[i]) {
            None => unit,
            Some(w) if unit < w.unit + u32::from(w.units) => w.byte,
            Some(w) => w.byte + u32::from(w.bytes) + (unit - w.unit - u32::from(w.units)),
        }
    }
}
