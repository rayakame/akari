//! Discord-flavored markdown: the rules Discord's clients apply to message content.

mod document;
mod flatten;
mod parse;

pub use document::{
    Block, BlockKind, Container, Document, HeadingLevel, Inline, InlineKind, Link, LinkId,
    LinkKind, Navigation, Span, SpoilerId, Style, TimestampStyle,
};

use parse::{Ctx, Input, Parser};

/// Parses message content as Discord does. Every input parses; one longer than `u32::MAX`
/// bytes gives an empty document.
pub fn parse(source: &str) -> Document {
    parse_counting(source).0
}

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
    (flatten::flatten(&nodes), parser.steps)
}

#[cfg(test)]
mod tests;
