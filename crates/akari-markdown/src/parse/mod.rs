//! A port of Discord's rule matching: at each position the first rule that matches wins, and a
//! rule's inner text is parsed again with what's known about its surroundings (`Ctx`).

mod code;
mod emphasis;
mod lines;
mod links;
mod memo;
mod phishing;
mod text;
mod tokens;

use std::borrow::Cow;
use std::ops::Range;

use crate::document::{HeadingLevel, InlineKind, Span, Style};
use memo::Frame;

pub(crate) struct Parser {
    pub(crate) steps: u64,
}

/// The text one parse runs over: the source itself, or text that parse-discord builds from it
/// (a quote's lines without `> `), mapped back to source offsets.
pub(crate) struct Input<'a> {
    pub(crate) text: Cow<'a, str>,
    map: Map,
}

enum Map {
    Source,
    /// `(text offset, source offset)` where each contiguous piece starts.
    Segments(Vec<(usize, usize)>),
}

impl<'a> Input<'a> {
    pub(crate) fn source(source: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(source),
            map: Map::Source,
        }
    }

    /// The text of `ranges` one after another, mapped to where each piece came from.
    pub(crate) fn derive(&self, ranges: &[Range<usize>]) -> Input<'static> {
        let mut text = String::new();
        let mut segments = Vec::new();
        for range in ranges {
            if segments.is_empty() || !range.is_empty() {
                let offset = text.len();
                self.split(range.clone(), |piece| {
                    segments.push((offset + piece.start - range.start, self.start(piece.start)));
                });
                if range.is_empty() {
                    segments.push((offset, self.start(range.start)));
                }
            }
            text.push_str(&self.text[range.clone()]);
        }
        Input {
            text: Cow::Owned(text),
            map: Map::Segments(segments),
        }
    }

    /// `range` in pieces that are contiguous in the source.
    fn split(&self, range: Range<usize>, mut each: impl FnMut(Range<usize>)) {
        let Map::Segments(segments) = &self.map else {
            if !range.is_empty() {
                each(range);
            }
            return;
        };
        let mut from = range.start;
        let first = segments.partition_point(|&(t, _)| t <= from);
        for &(t, _) in &segments[first..] {
            if t >= range.end {
                break;
            }
            each(from..t);
            from = t;
        }
        if from < range.end {
            each(from..range.end);
        }
    }

    fn segment(segments: &[(usize, usize)], at: usize, left: bool) -> usize {
        let i = segments.partition_point(|&(t, _)| if left { t < at } else { t <= at });
        match i.checked_sub(1).and_then(|i| segments.get(i)) {
            Some(&(t, s)) => s + (at - t),
            None => segments.first().map_or(at, |&(_, s)| s),
        }
    }

    fn start(&self, at: usize) -> usize {
        match &self.map {
            Map::Source => at,
            Map::Segments(segments) => Self::segment(segments, at, false),
        }
    }

    fn end(&self, at: usize) -> usize {
        match &self.map {
            Map::Source => at,
            Map::Segments(segments) => Self::segment(segments, at, true),
        }
    }

    /// The source of `range`; for derived text, from its first to its last character.
    pub(crate) fn span(&self, range: Range<usize>) -> Span {
        if range.is_empty() {
            let at = self.start(range.start);
            return Span::new(at..at);
        }
        Span::new(self.start(range.start)..self.end(range.end))
    }

    /// The source of `range` in contiguous pieces.
    pub(crate) fn pieces(&self, range: Range<usize>, mut each: impl FnMut(Span)) {
        self.split(range, |piece| each(self.span(piece)));
    }
}

/// How deep rules may nest; a delimiter that would open the next level stays text (D73).
pub(crate) const MAX_DEPTH: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineStart {
    /// Nothing before this on its line.
    Nothing,
    /// Only spaces before this on its line.
    Space,
    Text,
}

/// What a rule's inner text knows about its surroundings (parse-discord's `Context`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    pub(crate) line_start: LineStart,
    /// The match this text came from ended with a newline.
    pub(crate) trailing_newline: bool,
    pub(crate) quote: bool,
    pub(crate) list_depth: u8,
    /// Parsed to decide whether a masked link's text is allowed.
    pub(crate) testing_link: bool,
    /// A masked link's text.
    pub(crate) link_text: bool,
    pub(crate) depth: u8,
}

impl Ctx {
    pub(crate) const ROOT: Ctx = Ctx {
        line_start: LineStart::Nothing,
        trailing_newline: false,
        quote: false,
        list_depth: 0,
        testing_link: false,
        link_text: false,
        depth: 0,
    };

    fn escapes(self) -> bool {
        !self.testing_link && !self.link_text
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    Text(Vec<Piece>),
    /// The code in contiguous pieces of source.
    Code {
        code: Vec<Span>,
        source: Span,
    },
    CodeBlock {
        language: Option<Span>,
        code: Vec<Span>,
        source: Span,
    },
    Style {
        style: Style,
        children: Vec<Node>,
        source: Span,
    },
    Spoiler {
        children: Vec<Node>,
        source: Span,
    },
    Quote {
        children: Vec<Node>,
        source: Span,
    },
    Heading {
        level: HeadingLevel,
        children: Vec<Node>,
        source: Span,
    },
    Subtext {
        children: Vec<Node>,
        source: Span,
    },
    /// A link: shows `shown` (a URL's text), or `children` (a masked link's text).
    Link {
        data: links::LinkData,
        shown: Vec<Span>,
        children: Vec<Node>,
        source: Span,
    },
    /// Inline code inside a masked link's text while it's checked: its content counts.
    TestingCode {
        children: Vec<Node>,
    },
    /// A Unicode emoji, or a known `:shortcode:` (shown as its text).
    Emoji {
        source: Span,
        shortcode: bool,
    },
    /// A mention, command, custom emoji, timestamp or navigation token.
    Token {
        kind: InlineKind,
        source: Span,
    },
    /// `start`: the first number of an ordered list.
    List {
        start: Option<u32>,
        items: Vec<(Vec<Node>, Span)>,
        source: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Piece {
    Plain(Span),
    /// `\*`: shows the character after the backslash.
    Escape {
        display: Span,
        source: Span,
    },
    /// A newline, with the `\r` and the spaces before it that aren't shown.
    Break(Span),
}

struct Match {
    node: Node,
    /// Where parsing continues; usually the match's end.
    next: usize,
}

pub(crate) fn char_len(text: &str, at: usize) -> usize {
    match text.as_bytes().get(at) {
        Some(&b) if b < 0x80 => 1,
        Some(&b) if b >= 0xF0 => 4,
        Some(&b) if b >= 0xE0 => 3,
        Some(_) => 2,
        None => 0,
    }
}

impl Parser {
    pub(crate) fn new() -> Self {
        Self { steps: 0 }
    }

    /// A Unicode emoji; ©, ® and ™ match but stay text.
    fn emoji(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let len = crate::emoji::emoji_len(&input.text[..range.end], at, &mut self.steps)?;
        let source = input.span(at..at + len);
        let node = if crate::emoji::exempt(&input.text[at..at + len]) {
            Node::Text(vec![Piece::Plain(source)])
        } else {
            Node::Emoji {
                source,
                shortcode: false,
            }
        };
        Some(Match {
            node,
            next: at + len,
        })
    }

    /// `:(\w+):`, consumed whole; shown as its text (D66).
    fn shortcode(&mut self, input: &Input, at: usize, range: &Range<usize>) -> Option<Match> {
        let text = &input.text[at + 1..range.end];
        let name_len: usize = text
            .chars()
            .take_while(|&c| c.is_alphanumeric() || c == '_')
            .map(char::len_utf8)
            .sum();
        self.steps += name_len as u64;
        if name_len == 0 || !text[name_len..].starts_with(':') {
            return None;
        }
        let end = at + name_len + 2;
        let source = input.span(at..end);
        let node = if crate::emoji::is_shortcode(&text[..name_len]) {
            Node::Emoji {
                source,
                shortcode: true,
            }
        } else {
            let mut pieces = Vec::new();
            input.pieces(at..end, |span| pieces.push(Piece::Plain(span)));
            Node::Text(pieces)
        };
        Some(Match { node, next: end })
    }

    pub(crate) fn region(&mut self, input: &Input, range: Range<usize>, ctx: Ctx) -> Vec<Node> {
        let bytes = input.text.as_bytes();
        let mut nodes = Vec::new();
        let mut text_from = range.start;
        let mut at = range.start;
        let mut frame = Frame {
            start: range.start,
            ..Frame::default()
        };
        while at < range.end {
            self.steps += 1;
            if ctx.escapes() && bytes[at] == b'\\' && at + 1 < range.end {
                at += 1 + char_len(&input.text, at + 1);
                continue;
            }
            match self.rule(input, at, &range, ctx, &mut frame) {
                Some(m) => {
                    self.text(input, text_from..at, ctx, false, &mut nodes);
                    nodes.push(m.node);
                    at = m.next;
                    text_from = at;
                }
                None => at += char_len(&input.text, at),
            }
        }
        self.text(input, text_from..range.end, ctx, true, &mut nodes);
        nodes
    }

    fn rule(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let nest = ctx.depth < MAX_DEPTH;
        let byte = input.text.as_bytes()[at];
        if ctx.link_text {
            // Inside a masked link's text, only these rules apply (parse-discord's
            // allowed_in_links_source).
            return match byte {
                b'*' => nest
                    .then(|| self.star(input, at, range, ctx, frame))
                    .flatten()
                    .or_else(|| self.emoji(input, at, range)),
                b'_' if nest => self.underscore(input, at, range, ctx, frame),
                b'~' if nest => self.strike(input, at, range, ctx, frame),
                b'|' if nest => self.spoiler(input, at, range, ctx, frame),
                b'`' => self.inline_code(input, at, range, ctx, frame),
                b'<' => self.angle_in_link(input, at, range),
                b':' => self.shortcode(input, at, range),
                b'#' | b'0'..=b'9' | 0x80.. => self.emoji(input, at, range),
                _ => None,
            };
        }
        match byte {
            b'`' => self
                .code_block(input, at, range, frame)
                .or_else(|| self.inline_code(input, at, range, ctx, frame)),
            b'*' => nest
                .then(|| self.star(input, at, range, ctx, frame))
                .flatten()
                .or_else(|| self.emoji(input, at, range))
                .or_else(|| self.list(input, at, range, ctx).filter(|_| nest)),
            b'_' if nest => self.underscore(input, at, range, ctx, frame),
            b'~' if nest => self.strike(input, at, range, ctx, frame),
            b'|' if nest => self.spoiler(input, at, range, ctx, frame),
            b'<' => self
                .angle_in_link(input, at, range)
                .or_else(|| self.angle_mention(input, at, range))
                .or_else(|| self.angle_link(input, at, range, frame))
                .or_else(|| self.email(input, at, range))
                .or_else(|| self.phone(input, at, range)),
            b'@' => self.everyone(input, at, range),
            b'h' | b's' => self.plain_url(input, at, range),
            b'[' if nest => self.masked_link(input, at, range, ctx, frame),
            b'-' if nest => self
                .subtext(input, at, range, ctx)
                .or_else(|| self.list(input, at, range, ctx)),
            b'#' => self
                .emoji(input, at, range)
                .or_else(|| self.heading(input, at, range, ctx).filter(|_| nest)),
            b'>' if nest => self.quote(input, at, range, ctx),
            b'0'..=b'9' => self
                .emoji(input, at, range)
                .or_else(|| self.list(input, at, range, ctx).filter(|_| nest)),
            b':' => self.shortcode(input, at, range),
            0x80.. => self.emoji(input, at, range),
            _ => None,
        }
    }
}
