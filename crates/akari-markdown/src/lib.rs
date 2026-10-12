//! Discord-flavored markdown: the rules Discord's clients apply to message content.

mod cache;
mod document;
mod emoji;
mod flatten;
mod parse;
mod source_range;
pub mod utf16;

pub use document::{
    Block, BlockKind, Container, DiscordChannelLink, Document, HeadingLevel, Inline, InlineKind,
    Link, LinkId, LinkKind, Navigation, Span, SpoilerId, Style, TimestampStyle,
};

pub use cache::Cache;
pub use source_range::source_range;

use parse::{Ctx, Input, Parser};

/// Parses message content as Discord does. Every input parses; one longer than `u32::MAX`
/// bytes gives an empty document.
pub fn parse(source: &str) -> Document {
    parse_counting(source).0
}

/// At most this many steps per byte of input, plus one (D73). About 100 at worst: 64 nested
/// italics, each reading its content again.
#[cfg(any(test, feature = "fuzzing"))]
pub const STEPS_PER_BYTE: u64 = 128;

/// `parse`, and the work it took in steps (D73).
#[cfg(any(test, feature = "fuzzing"))]
pub fn parse_with_steps(source: &str) -> (Document, u64) {
    parse_counting(source)
}

fn parse_counting(source: &str) -> (Document, u64) {
    if u32::try_from(source.len()).is_err() {
        return (Document::default(), 0);
    }
    let mut parser = Parser::new();
    let nodes = parser.region(&Input::source(source), 0..source.len(), Ctx::ROOT);
    let jumbo = emoji::jumbo(&nodes, source);
    (flatten::flatten(&nodes, jumbo), parser.steps)
}

#[cfg(test)]
mod tests;
