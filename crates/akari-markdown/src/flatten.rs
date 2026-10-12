//! The parse tree as a flat list of blocks, each with its runs.

use crate::document::{Block, BlockKind, Container, Document, Inline, InlineKind, Span, Style};
use crate::parse::{Node, Piece};

pub(crate) fn flatten(nodes: &[Node]) -> Document {
    let mut flat = Flat::default();
    let ctx = Context {
        kind: BlockKind::Paragraph,
        containers: Vec::new(),
        style: Style::default(),
    };
    flat.nodes(nodes, &ctx);
    flat.close();
    Document::new(flat.blocks, Vec::new(), false, 0)
}

#[derive(Default)]
struct Flat {
    blocks: Vec<Block>,
    open: Option<Open>,
}

struct Open {
    kind: BlockKind,
    containers: Vec<Container>,
    inlines: Vec<Inline>,
    source: Span,
}

struct Context {
    kind: BlockKind,
    containers: Vec<Container>,
    style: Style,
}

impl Flat {
    fn nodes(&mut self, nodes: &[Node], ctx: &Context) {
        for node in nodes {
            match node {
                Node::Text(pieces) => {
                    for piece in pieces {
                        self.piece(*piece, ctx);
                    }
                }
            }
        }
    }

    fn piece(&mut self, piece: Piece, ctx: &Context) {
        let (kind, source) = match piece {
            Piece::Plain(span) => {
                if let Some(last) = self.open.as_mut().and_then(|o| o.inlines.last_mut())
                    && let InlineKind::Text(shown) = &mut last.kind
                    && last.source == *shown
                    && shown.end == span.start
                    && last.style == ctx.style
                {
                    shown.end = span.end;
                    last.source.end = span.end;
                    self.extend(span);
                    return;
                }
                (InlineKind::Text(span), span)
            }
            Piece::Escape { display, source } => (InlineKind::Text(display), source),
            Piece::Break(span) => (InlineKind::Break, span),
        };
        self.inline(
            Inline {
                kind,
                style: ctx.style,
                spoiler: None,
                link: None,
                source,
            },
            ctx,
        );
    }

    fn inline(&mut self, inline: Inline, ctx: &Context) {
        let source = inline.source;
        let open = self.open.get_or_insert_with(|| Open {
            kind: ctx.kind,
            containers: ctx.containers.clone(),
            inlines: Vec::new(),
            source,
        });
        open.inlines.push(inline);
        self.extend(source);
    }

    fn extend(&mut self, span: Span) {
        if let Some(open) = &mut self.open {
            open.source.start = open.source.start.min(span.start);
            open.source.end = open.source.end.max(span.end);
        }
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
            containers: open.containers.into(),
            spoiler: None,
            inlines: open.inlines.into(),
            source: open.source,
        });
    }
}
