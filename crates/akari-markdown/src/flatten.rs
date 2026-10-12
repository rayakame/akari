use crate::document::{
    Block, BlockKind, Container, Document, Inline, InlineKind, Link, LinkId, Span, SpoilerId, Style,
};
use crate::parse::{Node, Piece};

pub(crate) fn flatten(nodes: &[Node], jumbo: bool) -> Document {
    let mut flat = Flat::default();
    let ctx = Context {
        kind: BlockKind::Paragraph,
        style: Style::default(),
        spoiler: None,
        link: None,
    };
    flat.nodes(nodes, ctx);
    flat.close();
    Document::new(flat.blocks, flat.links, jumbo, flat.spoilers)
}

#[derive(Default)]
struct Flat {
    blocks: Vec<Block>,
    links: Vec<(Link, Span)>,
    containers: Vec<(Container, bool)>,
    open: Option<Open>,
    spoilers: u16,
}

struct Open {
    kind: BlockKind,
    containers: Box<[Container]>,
    spoiler: Option<SpoilerId>,
    inlines: Vec<Inline>,
    source: Span,
}

#[derive(Clone, Copy)]
struct Context {
    kind: BlockKind,
    style: Style,
    spoiler: Option<SpoilerId>,
    link: Option<LinkId>,
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
                let containers = self.block_containers();
                self.blocks.push(Block {
                    kind: BlockKind::CodeBlock {
                        language: *language,
                    },
                    containers,
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
            Node::Token { kind, source } => self.inline(*kind, *source, ctx),
            Node::Link {
                data,
                shown,
                children,
                source,
            } => {
                // Past 65,535 links, a link shows as its text.
                let link = u16::try_from(self.links.len())
                    .ok()
                    .filter(|&id| id < u16::MAX);
                if link.is_some() {
                    self.links.push((
                        Link {
                            url: data.url.clone().into(),
                            kind: data.kind.clone(),
                            suppress_embed: data.suppress_embed,
                        },
                        *source,
                    ));
                }
                let ctx = Context {
                    link: link.map(LinkId),
                    ..ctx
                };
                for &span in shown {
                    let whole = if shown.len() == 1 { *source } else { span };
                    self.inline(InlineKind::Text(span), whole, ctx);
                }
                self.nodes(children, ctx);
            }
            Node::TestingCode { children } => self.nodes(children, ctx),
            Node::Emoji { source, .. } => self.piece(Piece::Plain(*source), ctx),
            Node::Quote { children, source } => {
                self.containers.push((Container::Quote, false));
                self.container(children, *source, ctx);
                self.containers.pop();
            }
            Node::Heading {
                level,
                children,
                source,
            } => self.container(
                children,
                *source,
                Context {
                    kind: BlockKind::Heading(*level),
                    ..ctx
                },
            ),
            Node::Subtext { children, source } => self.container(
                children,
                *source,
                Context {
                    kind: BlockKind::Subtext,
                    ..ctx
                },
            ),
            Node::List { start, items, .. } => {
                for (i, (children, source)) in items.iter().enumerate() {
                    let item = Container::ListItem {
                        ordered: start.is_some(),
                        number: start.unwrap_or(1).saturating_add(i as u32),
                        marker: true,
                    };
                    self.containers.push((item, false));
                    self.container(children, *source, ctx);
                    self.containers.pop();
                }
            }
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

    // At least one block, empty if need be, so an empty item or quote still shows.
    fn container(&mut self, children: &[Node], source: Span, ctx: Context) {
        self.close();
        let before = self.blocks.len();
        self.nodes(children, ctx);
        self.close();
        if self.blocks.len() == before {
            let containers = self.block_containers();
            self.blocks.push(Block {
                kind: ctx.kind,
                containers,
                spoiler: ctx.spoiler,
                inlines: Box::default(),
                source,
            });
        }
    }

    fn block_containers(&mut self) -> Box<[Container]> {
        self.containers
            .iter_mut()
            .map(|(container, shown)| match container {
                Container::ListItem {
                    ordered, number, ..
                } => {
                    let marker = !*shown;
                    *shown = true;
                    Container::ListItem {
                        ordered: *ordered,
                        number: *number,
                        marker,
                    }
                }
                other => *other,
            })
            .collect()
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
                    && last.link == ctx.link
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
        if self.open.is_none() {
            let containers = self.block_containers();
            self.open = Some(Open {
                kind: ctx.kind,
                containers,
                spoiler: ctx.spoiler,
                inlines: Vec::new(),
                source,
            });
        }
        let Some(open) = &mut self.open else {
            return;
        };
        if open.spoiler != ctx.spoiler {
            open.spoiler = None;
        }
        open.source.start = open.source.start.min(source.start);
        open.source.end = open.source.end.max(source.end);
        open.inlines.push(Inline {
            kind,
            style: ctx.style,
            spoiler: ctx.spoiler,
            link: ctx.link,
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
