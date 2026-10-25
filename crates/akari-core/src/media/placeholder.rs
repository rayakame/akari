/// A thumbhash decoded as Discord's client decodes it: at most 32 pixels on the longest side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Placeholder {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, row by row.
    pub rgba: Vec<u8>,
}

pub fn placeholder(_thumbhash: &str) -> Option<Placeholder> {
    None
}
