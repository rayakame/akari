//! Discord-flavored markdown: the rules Discord's clients apply to message content.

// Message content comes from strangers, so no input may panic a client (unwrap and expect are
// denied workspace-wide).
#![cfg_attr(
    not(test),
    deny(
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented
    )
)]

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
pub use emoji::emoji_shortcode;
pub use source_range::source_range;

use parse::{Ctx, Input, Parser, Rules};

/// Parses message content as Discord does. Every input parses; one longer than `u32::MAX`
/// bytes gives an empty document.
pub fn parse(source: &str) -> Document {
    parse_counting(source).0
}

/// Which text of an embed, for its rules (canary 649848, modules 46054 and 999915).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmbedText {
    /// A title or a field's name: styles, emoji and timestamps, no code blocks, mentions, lists,
    /// headings or subtext; `links` false when the title is itself a link.
    Title { links: bool },
    /// The message's rules; lists and headings only for messages after 2023-03-22.
    Description { lists: bool },
    /// As a description, never headings.
    FieldValue { lists: bool },
}

/// Parses a rich embed's text with Discord's rules for it. Emoji are never jumbo there.
pub fn parse_embed(source: &str, text: EmbedText) -> Document {
    let rules = match text {
        EmbedText::Title { links } => Rules {
            blocks: false,
            mentions: false,
            links,
            lists: false,
            headings: false,
        },
        EmbedText::Description { lists } => Rules {
            lists,
            headings: lists,
            ..Rules::MESSAGE
        },
        EmbedText::FieldValue { lists } => Rules {
            lists,
            headings: false,
            ..Rules::MESSAGE
        },
    };
    if u32::try_from(source.len()).is_err() {
        return Document::default();
    }
    let mut parser = Parser::with(rules);
    let nodes = parser.region(&Input::source(source), 0..source.len(), Ctx::ROOT);
    flatten::flatten(&nodes, false)
}

/// A non-rich embed's text as Discord shows it: the string itself, its line breaks kept and a
/// last one dropped.
pub fn plain(source: &str) -> Document {
    if u32::try_from(source.len()).is_err() {
        return Document::default();
    }
    let inline = |kind, range: std::ops::Range<usize>| Inline {
        kind,
        style: Style::default(),
        spoiler: None,
        link: None,
        source: Span::new(range),
    };
    let mut inlines = Vec::new();
    let mut start = 0;
    for (index, _) in source.match_indices('\n') {
        let end = if source[..index].ends_with('\r') {
            index - 1
        } else {
            index
        };
        if end > start {
            inlines.push(inline(InlineKind::Text(Span::new(start..end)), start..end));
        }
        inlines.push(inline(InlineKind::Break, end..index + 1));
        start = index + 1;
    }
    if start < source.len() {
        inlines.push(inline(
            InlineKind::Text(Span::new(start..source.len())),
            start..source.len(),
        ));
    }
    while inlines
        .last()
        .is_some_and(|last| last.kind == InlineKind::Break)
    {
        inlines.pop();
    }
    if inlines.is_empty() {
        return Document::default();
    }
    let block = Block {
        kind: BlockKind::Paragraph,
        containers: Box::new([]),
        spoiler: None,
        inlines: inlines.into(),
        source: Span::new(0..source.len()),
    };
    Document::new(vec![block], Vec::new(), false, 0)
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
