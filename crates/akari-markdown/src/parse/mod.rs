//! A port of Discord's rule matching: at each position the first rule that matches wins, and a
//! rule's inner text is parsed again with what's known about its surroundings (`Ctx`).

mod code;
mod memo;
mod text;

use std::borrow::Cow;
use std::ops::Range;

use crate::document::Span;
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
}

impl<'a> Input<'a> {
    pub(crate) fn source(source: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(source),
            map: Map::Source,
        }
    }

    fn start(&self, at: usize) -> usize {
        match &self.map {
            Map::Source => at,
        }
    }

    fn end(&self, at: usize) -> usize {
        match &self.map {
            Map::Source => at,
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
        each(self.span(range));
    }
}

/// What a rule's inner text knows about its surroundings (parse-discord's `Context`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    /// The match this text came from ended with a newline.
    pub(crate) trailing_newline: bool,
    /// Parsed to decide whether a masked link's text is allowed.
    pub(crate) testing_link: bool,
    /// A masked link's text.
    pub(crate) link_text: bool,
}

impl Ctx {
    pub(crate) const ROOT: Ctx = Ctx {
        trailing_newline: false,
        testing_link: false,
        link_text: false,
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

    pub(crate) fn region(&mut self, input: &Input, range: Range<usize>, ctx: Ctx) -> Vec<Node> {
        let bytes = input.text.as_bytes();
        let mut nodes = Vec::new();
        let mut text_from = range.start;
        let mut at = range.start;
        let mut frame = Frame::default();
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
        _ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        match input.text.as_bytes()[at] {
            b'`' => self
                .code_block(input, at, range, frame)
                .or_else(|| self.inline_code(input, at, range, frame)),
            _ => None,
        }
    }
}
