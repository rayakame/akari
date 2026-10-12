//! The parse tree as a flat list of blocks, each with its runs.

use crate::document::{
    Block, BlockKind, Container, Document, Inline, InlineKind, Span, SpoilerId, Style,
};
use crate::parse::{Node, Piece};

pub(crate) fn flatten(nodes: &[Node]) -> Document {
    let mut flat = Flat::default();
    let ctx = Context {
        kind: BlockKind::Paragraph,
        style: Style::default(),
        spoiler: None,
    };
    flat.nodes(nodes, ctx);
    flat.close();
    Document::new(flat.blocks, Vec::new(), false, flat.spoilers)
}

#[derive(Default)]
struct Flat {
    blocks: Vec<Block>,
    containers: Vec<Container>,
    open: Option<Open>,
    spoilers: u16,
}

/// The paragraph, heading or subtext line being filled.
struct Open {
    kind: BlockKind,
    containers: Box<[Container]>,
    /// The spoiler around the block when it opened, until a run outside it arrives.
    spoiler: Option<SpoilerId>,
    inlines: Vec<Inline>,
    source: Span,
}

#[derive(Clone, Copy)]
struct Context {
    kind: BlockKind,
    style: Style,
    spoiler: Option<SpoilerId>,
}

impl Flat {
    fn nodes(&mut self, nodes: &[Node], ctx: Context) {
        for node in nodes {
            self.node(node, ctx);
        }
    }

    fn node(&mut self, node: &Node, ctx: Context) {
        match node {
            Node::Text(pieces) => {
                for piece in pieces {
                    self.piece(*piece, ctx);
                }
            }
            Node::Code { code, source } => {
                for &piece in code {
                    self.inline(InlineKind::Code(piece), *source, ctx);
                }
            }
            Node::CodeBlock {
                language,
                code,
                source,
            } => {
                self.close();
                let inlines = code.iter().map(|&piece| Inline {
                    kind: InlineKind::Code(piece),
                    style: Style::default(),
                    spoiler: ctx.spoiler,
                    link: None,
                    source: piece,
                });
                self.blocks.push(Block {
                    kind: BlockKind::CodeBlock {
                        language: *language,
                    },
                    containers: self.containers.clone().into(),
                    spoiler: ctx.spoiler,
                    inlines: inlines.collect(),
                    source: *source,
                });
            }
            Node::Style {
                style, children, ..
            } => self.nodes(
                children,
                Context {
                    style: ctx.style.with(*style),
                    ..ctx
                },
            ),
            Node::Spoiler { children, .. } => {
                let id = SpoilerId(self.spoilers.min(u16::MAX - 1));
                self.spoilers = self.spoilers.saturating_add(1);
                self.nodes(
                    children,
                    Context {
                        spoiler: Some(id),
                        ..ctx
                    },
                );
            }
        }
    }

    fn piece(&mut self, piece: Piece, ctx: Context) {
        match piece {
            Piece::Plain(span) => {
                if let Some(open) = &mut self.open
                    && let Some(last) = open.inlines.last_mut()
                    && let InlineKind::Text(shown) = &mut last.kind
                    && last.source == *shown
                    && shown.end == span.start
                    && last.style == ctx.style
                    && last.spoiler == ctx.spoiler
                {
                    shown.end = span.end;
                    last.source.end = span.end;
                    open.source.end = open.source.end.max(span.end);
                    return;
                }
                self.inline(InlineKind::Text(span), span, ctx);
            }
            Piece::Escape { display, source } => {
                self.inline(InlineKind::Text(display), source, ctx)
            }
            Piece::Break(span) => self.inline(InlineKind::Break, span, ctx),
        }
    }

    fn inline(&mut self, kind: InlineKind, source: Span, ctx: Context) {
        let open = self.open.get_or_insert_with(|| Open {
            kind: ctx.kind,
            containers: self.containers.clone().into(),
            spoiler: ctx.spoiler,
            inlines: Vec::new(),
            source,
        });
        if open.spoiler != ctx.spoiler {
            open.spoiler = None;
        }
        open.source.start = open.source.start.min(source.start);
        open.source.end = open.source.end.max(source.end);
        open.inlines.push(Inline {
            kind,
            style: ctx.style,
            spoiler: ctx.spoiler,
            link: None,
            source,
        });
    }

    fn close(&mut self) {
        let Some(mut open) = self.open.take() else {
            return;
        };
        if open
            .inlines
            .last()
            .is_some_and(|i| i.kind == InlineKind::Break)
        {
            open.inlines.pop();
        }
        self.blocks.push(Block {
            kind: open.kind,
            containers: open.containers,
            spoiler: open.spoiler,
            inlines: open.inlines.into(),
            source: open.source,
        });
    }
}
