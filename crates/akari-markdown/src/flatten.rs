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
    // A work list, not recursion: content nests 64 deep and hosts parse on small stacks.
    let mut tasks = vec![Task::Nodes(nodes.iter(), ctx)];
    while let Some(task) = tasks.pop() {
        match task {
            Task::Nodes(mut rest, ctx) => {
                if let Some(node) = rest.next() {
                    tasks.push(Task::Nodes(rest, ctx));
                    flat.node(node, ctx, &mut tasks);
                }
            }
            Task::Open {
                container,
                children,
                source,
                ctx,
            } => {
                if let Some(container) = container {
                    flat.containers.push((container, false));
                }
                flat.close();
                tasks.push(Task::Close {
                    source,
                    ctx,
                    before: flat.blocks.len(),
                    pop: container.is_some(),
                });
                tasks.push(Task::Nodes(children.iter(), ctx));
            }
            Task::Close {
                source,
                ctx,
                before,
                pop,
            } => {
                flat.close_container(source, ctx, before);
                if pop {
                    flat.containers.pop();
                }
            }
        }
    }
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

enum Task<'n> {
    Nodes(std::slice::Iter<'n, Node>, Context),
    Open {
        container: Option<Container>,
        children: &'n [Node],
        source: Span,
        ctx: Context,
    },
    Close {
        source: Span,
        ctx: Context,
        before: usize,
        pop: bool,
    },
}

#[derive(Clone, Copy)]
struct Context {
    kind: BlockKind,
    style: Style,
    spoiler: Option<SpoilerId>,
    link: Option<LinkId>,
}

impl Flat {
    fn node<'n>(&mut self, node: &'n Node, ctx: Context, tasks: &mut Vec<Task<'n>>) {
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
            } => tasks.push(Task::Nodes(
                children.iter(),
                Context {
                    style: ctx.style.with(*style),
                    ..ctx
                },
            )),
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
                tasks.push(Task::Nodes(children.iter(), ctx));
            }
            Node::TestingCode { children } => tasks.push(Task::Nodes(children.iter(), ctx)),
            Node::Emoji { source, .. } => self.piece(Piece::Plain(*source), ctx),
            Node::Quote { children, source } => tasks.push(Task::Open {
                container: Some(Container::Quote),
                children,
                source: *source,
                ctx,
            }),
            Node::Heading {
                level,
                children,
                source,
            } => tasks.push(Task::Open {
                container: None,
                children,
                source: *source,
                ctx: Context {
                    kind: BlockKind::Heading(*level),
                    ..ctx
                },
            }),
            Node::Subtext { children, source } => tasks.push(Task::Open {
                container: None,
                children,
                source: *source,
                ctx: Context {
                    kind: BlockKind::Subtext,
                    ..ctx
                },
            }),
            Node::List { start, items, .. } => {
                for (i, (children, source)) in items.iter().enumerate().rev() {
                    tasks.push(Task::Open {
                        container: Some(Container::ListItem {
                            ordered: start.is_some(),
                            number: start.unwrap_or(1).saturating_add(i as u32),
                            marker: true,
                        }),
                        children,
                        source: *source,
                        ctx,
                    });
                }
            }
            Node::Spoiler { children, .. } => {
                let id = SpoilerId(self.spoilers.min(u16::MAX - 1));
                self.spoilers = self.spoilers.saturating_add(1);
                tasks.push(Task::Nodes(
                    children.iter(),
                    Context {
                        spoiler: Some(id),
                        ..ctx
                    },
                ));
            }
        }
    }

    // At least one block, empty if need be, so an empty item or quote still shows.
    fn close_container(&mut self, source: Span, ctx: Context, before: usize) {
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
