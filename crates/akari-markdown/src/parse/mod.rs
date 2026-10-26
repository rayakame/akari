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
    pub(crate) rules: Rules,
}

/// Which of Discord's rules apply: all of them in a message, fewer in an embed's text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Rules {
    /// Code blocks, quotes and subtext.
    pub(crate) blocks: bool,
    /// Users, roles, channels, @everyone and @here, and `<id:…>` navigation.
    pub(crate) mentions: bool,
    pub(crate) links: bool,
    pub(crate) lists: bool,
    pub(crate) headings: bool,
}

impl Rules {
    pub(crate) const MESSAGE: Rules = Rules {
        blocks: true,
        mentions: true,
        links: true,
        lists: true,
        headings: true,
    };
}

// parse-discord parses some text it builds (a quote's lines without "> "), mapped back here.
pub(crate) struct Input<'a> {
    pub(crate) text: Cow<'a, str>,
    map: Map,
}

enum Map {
    Source,
    Segments(Vec<(usize, usize)>),
}

impl<'a> Input<'a> {
    pub(crate) fn source(source: &'a str) -> Self {
        Self {
            text: Cow::Borrowed(source),
            map: Map::Source,
        }
    }

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

    pub(crate) fn span(&self, range: Range<usize>) -> Span {
        if range.is_empty() {
            let at = self.start(range.start);
            return Span::new(at..at);
        }
        Span::new(self.start(range.start)..self.end(range.end))
    }

    pub(crate) fn pieces(&self, range: Range<usize>, mut each: impl FnMut(Span)) {
        self.split(range, |piece| each(self.span(piece)));
    }
}

// D73: a delimiter that would open the next level stays text.
pub(crate) const MAX_DEPTH: u8 = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LineStart {
    Nothing,
    Space,
    Text,
}

// parse-discord's Context.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Ctx {
    pub(crate) line_start: LineStart,
    // The match ended with a newline, so the text's final spaces aren't shown.
    pub(crate) trailing_newline: bool,
    pub(crate) quote: bool,
    pub(crate) list_depth: u8,
    pub(crate) testing_link: bool,
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
    Link {
        data: links::LinkData,
        shown: Vec<Span>,
        children: Vec<Node>,
        source: Span,
    },
    // parse-discord parses inline code's content while it checks a masked link's text.
    TestingCode {
        children: Vec<Node>,
    },
    Emoji {
        source: Span,
        shortcode: bool,
    },
    Token {
        kind: InlineKind,
        source: Span,
    },
    List {
        start: Option<u32>,
        items: Vec<(Vec<Node>, Span)>,
        source: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Piece {
    Plain(Span),
    Escape { display: Span, source: Span },
    // Covers the \r and the spaces before the newline, which aren't shown.
    Break(Span),
}

pub(crate) fn literal(input: &Input, range: Range<usize>) -> Node {
    let bytes = input.text.as_bytes();
    let mut pieces = Vec::new();
    let mut from = range.start;
    for at in range.clone() {
        if bytes[at] == b'\n' {
            let start = if at > from && bytes[at - 1] == b'\r' {
                at - 1
            } else {
                at
            };
            if start > from {
                input.pieces(from..start, |span| pieces.push(Piece::Plain(span)));
            }
            pieces.push(Piece::Break(input.span(start..at + 1)));
            from = at + 1;
        }
    }
    if from < range.end {
        input.pieces(from..range.end, |span| pieces.push(Piece::Plain(span)));
    }
    Node::Text(pieces)
}

struct Match {
    node: Node,
    // A URL's unbalanced ")" is read again as text.
    next: usize,
    // One per child list, in order; region parses them once the rule has returned.
    nested: Vec<Content>,
}

impl Match {
    fn new(node: Node, next: usize) -> Self {
        Self {
            node,
            next,
            nested: Vec::new(),
        }
    }
}

// Parsed by region rather than by its rule, so a level of nesting costs one frame.
struct Content {
    input: Option<Input<'static>>,
    range: Range<usize>,
    ctx: Ctx,
}

impl Content {
    fn new(range: Range<usize>, ctx: Ctx) -> Self {
        Self {
            input: None,
            range,
            ctx,
        }
    }
}

impl Node {
    fn fill(&mut self, i: usize, nodes: Vec<Node>) {
        match self {
            Node::Style { children, .. }
            | Node::Spoiler { children, .. }
            | Node::Quote { children, .. }
            | Node::Heading { children, .. }
            | Node::Subtext { children, .. }
            | Node::Link { children, .. }
            | Node::TestingCode { children } => *children = nodes,
            Node::List { items, .. } => {
                if let Some((children, _)) = items.get_mut(i) {
                    *children = nodes;
                }
            }
            _ => {}
        }
    }
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
        Self::with(Rules::MESSAGE)
    }

    pub(crate) fn with(rules: Rules) -> Self {
        Self { steps: 0, rules }
    }

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
        Some(Match::new(node, at + len))
    }

    // `:\w+:` is consumed whole, as parse-discord does, and shown as text (D66).
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
        Some(Match::new(node, end))
    }

    pub(crate) fn region(&mut self, input: &Input, range: Range<usize>, ctx: Ctx) -> Vec<Node> {
        let bytes = input.text.as_bytes();
        let mut nodes = Vec::new();
        let mut text_from = range.start;
        let mut at = range.start;
        let mut frame = Frame::boxed(range.start);
        while at < range.end {
            self.steps += 1;
            if ctx.escapes() && bytes[at] == b'\\' && at + 1 < range.end {
                at += 1 + char_len(&input.text, at + 1);
                continue;
            }
            let Some(Match {
                mut node,
                next,
                nested,
            }) = self.rule(input, at, &range, ctx, &mut frame)
            else {
                at += char_len(&input.text, at);
                continue;
            };
            self.text(input, text_from..at, ctx, false, &mut nodes);
            for (i, content) in nested.into_iter().enumerate() {
                let children = match &content.input {
                    Some(own) => self.region(own, content.range, content.ctx),
                    None => self.region(input, content.range, content.ctx),
                };
                node.fill(i, children);
            }
            nodes.push(node);
            at = next;
            text_from = at;
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
        if ctx.link_text {
            return self.rule_in_link_text(input, at, range, ctx, frame);
        }
        let nest = ctx.depth < MAX_DEPTH;
        let rules = self.rules;
        // Fallback chains live in their own calls: a link check runs inside this frame.
        match input.text.as_bytes()[at] {
            b'`' => self.backtick(input, at, range, ctx, frame),
            b'*' => self.asterisk(input, at, range, ctx, frame),
            b'_' if nest => self.underscore(input, at, range, ctx, frame),
            b'~' if nest => self.strike(input, at, range, ctx, frame),
            b'|' if nest => self.spoiler(input, at, range, ctx, frame),
            b'<' => self.angle(input, at, range, frame),
            b'@' if rules.mentions => self.everyone(input, at, range),
            b'h' | b's' if rules.links => self.plain_url(input, at, range, ctx),
            b'[' if nest && rules.links => self.masked_link(input, at, range, ctx, frame),
            b'-' if nest => self.dash(input, at, range, ctx),
            b'#' => self.hash(input, at, range, ctx),
            b'>' if nest && rules.blocks => self.quote(input, at, range, ctx),
            b'0'..=b'9' => self.digit(input, at, range, ctx),
            b':' => self.shortcode(input, at, range),
            0x80.. => self.emoji(input, at, range),
            _ => None,
        }
    }

    // parse-discord's allowed_in_links_source.
    fn rule_in_link_text(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let nest = ctx.depth < MAX_DEPTH;
        match input.text.as_bytes()[at] {
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
        }
    }

    fn backtick(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        self.rules
            .blocks
            .then(|| self.code_block(input, at, range, frame))
            .flatten()
            .or_else(|| self.inline_code(input, at, range, ctx, frame))
    }

    fn asterisk(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        ctx: Ctx,
        frame: &mut Frame,
    ) -> Option<Match> {
        let nest = ctx.depth < MAX_DEPTH;
        let lists = nest && self.rules.lists;
        nest.then(|| self.star(input, at, range, ctx, frame))
            .flatten()
            .or_else(|| self.emoji(input, at, range))
            .or_else(|| lists.then(|| self.list(input, at, range, ctx)).flatten())
    }

    fn angle(
        &mut self,
        input: &Input,
        at: usize,
        range: &Range<usize>,
        frame: &mut Frame,
    ) -> Option<Match> {
        let Rules {
            mentions, links, ..
        } = self.rules;
        self.angle_in_link(input, at, range)
            .or_else(|| self.angle_mention(input, at, range, mentions))
            .or_else(|| {
                links
                    .then(|| self.angle_link(input, at, range, frame))
                    .flatten()
            })
            .or_else(|| links.then(|| self.email(input, at, range, frame)).flatten())
            .or_else(|| links.then(|| self.phone(input, at, range)).flatten())
    }

    fn dash(&mut self, input: &Input, at: usize, range: &Range<usize>, ctx: Ctx) -> Option<Match> {
        let Rules { blocks, lists, .. } = self.rules;
        blocks
            .then(|| self.subtext(input, at, range, ctx))
            .flatten()
            .or_else(|| lists.then(|| self.list(input, at, range, ctx)).flatten())
    }

    fn hash(&mut self, input: &Input, at: usize, range: &Range<usize>, ctx: Ctx) -> Option<Match> {
        let headings = ctx.depth < MAX_DEPTH && self.rules.headings;
        self.emoji(input, at, range).or_else(|| {
            headings
                .then(|| self.heading(input, at, range, ctx))
                .flatten()
        })
    }

    fn digit(&mut self, input: &Input, at: usize, range: &Range<usize>, ctx: Ctx) -> Option<Match> {
        let lists = ctx.depth < MAX_DEPTH && self.rules.lists;
        self.emoji(input, at, range)
            .or_else(|| lists.then(|| self.list(input, at, range, ctx)).flatten())
    }
}
