//! The corpus notation (D74): one line per block, then the links, then `jumbo`. Uses the
//! public API only, so the local differential run can reuse it.

use std::fmt::Write as _;

use crate::{
    Block, BlockKind, Container, Document, HeadingLevel, Inline, InlineKind, LinkKind, Navigation,
    Span, Style, TimestampStyle,
};

pub(crate) fn outline(doc: &Document, source: &str) -> String {
    let mut lines: Vec<String> = doc.blocks().iter().map(|b| block(b, source)).collect();
    for (i, link) in doc.links().iter().enumerate() {
        let mut line = format!("link{i}: <{}>", link.url);
        match &link.kind {
            LinkKind::Plain => {}
            LinkKind::Masked { title } => {
                line.push_str(" masked");
                if let Some(title) = title {
                    let _ = write!(line, " title={}", quoted(title));
                }
            }
            LinkKind::Email => line.push_str(" email"),
            LinkKind::Phone => line.push_str(" phone"),
        }
        if link.suppress_embed {
            line.push_str(" suppress");
        }
        lines.push(line);
    }
    if doc.jumbo() {
        lines.push("jumbo".into());
    }
    lines.join("\n")
}

fn block(block: &Block, source: &str) -> String {
    let mut head: Vec<String> = block
        .containers
        .iter()
        .map(|c| match c {
            Container::Quote => ">".into(),
            Container::ListItem { marker: false, .. } => ".".into(),
            Container::ListItem {
                ordered: true,
                number,
                ..
            } => format!("{number}."),
            Container::ListItem { ordered: false, .. } => "-".into(),
        })
        .collect();
    let mut kind = match &block.kind {
        BlockKind::Paragraph => "p".to_owned(),
        BlockKind::Heading(HeadingLevel::One) => "h1".into(),
        BlockKind::Heading(HeadingLevel::Two) => "h2".into(),
        BlockKind::Heading(HeadingLevel::Three) => "h3".into(),
        BlockKind::Subtext => "sub".into(),
        BlockKind::CodeBlock { language } => {
            format!("code[{}]", language.map_or("", |l| text(source, l)))
        }
    };
    if let Some(spoiler) = block.spoiler {
        let _ = write!(kind, "{{sp{}}}", spoiler.0);
    }
    head.push(kind);
    let runs = if matches!(block.kind, BlockKind::CodeBlock { .. }) {
        let code: String = block
            .inlines
            .iter()
            .filter_map(|i| match i.kind {
                InlineKind::Code(span) => Some(text(source, span)),
                _ => None,
            })
            .collect();
        vec![quoted(&code)]
    } else {
        runs(&block.inlines, source)
    };
    let head = head.join(" ");
    if runs.is_empty() {
        format!("{head}:")
    } else {
        format!("{head}: {}", runs.join(" "))
    }
}

fn runs(inlines: &[Inline], source: &str) -> Vec<String> {
    let mut out: Vec<(String, Run)> = Vec::new();
    for inline in inlines {
        let attrs = attrs(inline);
        let run = match &inline.kind {
            InlineKind::Text(span) => Run::Text(text(source, *span).to_owned()),
            InlineKind::Code(span) => Run::Code(text(source, *span).to_owned(), inline.source),
            other => Run::Other(token(other, source)),
        };
        match (out.last_mut(), &run) {
            (Some((a, Run::Text(t))), Run::Text(more)) if *a == attrs => t.push_str(more),
            (Some((a, Run::Code(t, s))), Run::Code(more, source)) if *a == attrs && s == source => {
                t.push_str(more);
            }
            _ => out.push((attrs, run)),
        }
    }
    out.into_iter()
        .map(|(attrs, run)| {
            let body = match run {
                Run::Text(t) => quoted(&t),
                Run::Code(t, _) => format!("code{}", quoted(&t)),
                Run::Other(t) => t,
            };
            if attrs.is_empty() || body == "br" {
                body
            } else {
                format!("{{{attrs}}}{body}")
            }
        })
        .collect()
}

enum Run {
    Text(String),
    Code(String, Span),
    Other(String),
}

fn attrs(inline: &Inline) -> String {
    let mut attrs = Vec::new();
    for (style, name) in [
        (Style::BOLD, "b"),
        (Style::ITALIC, "i"),
        (Style::UNDERLINE, "u"),
        (Style::STRIKE, "s"),
    ] {
        if inline.style.contains(style) {
            attrs.push(name.to_owned());
        }
    }
    if let Some(spoiler) = inline.spoiler {
        attrs.push(format!("sp{}", spoiler.0));
    }
    if let Some(link) = inline.link {
        attrs.push(format!("l{}", link.0));
    }
    attrs.join(" ")
}

fn token(kind: &InlineKind, source: &str) -> String {
    match kind {
        InlineKind::Break => "br".into(),
        InlineKind::UserMention(id) => format!("@user:{id}"),
        InlineKind::RoleMention(id) => format!("@role:{id}"),
        InlineKind::ChannelMention(id) => format!("#chan:{id}"),
        InlineKind::Everyone => "@everyone".into(),
        InlineKind::Here => "@here".into(),
        InlineKind::Command { name, id } => format!("/cmd{}:{id}", quoted(text(source, *name))),
        InlineKind::CustomEmoji { name, id, animated } => format!(
            "{}emoji{}:{id}",
            if *animated { "a" } else { "" },
            quoted(text(source, *name))
        ),
        InlineKind::Timestamp { seconds, style } => format!("time:{seconds}:{}", letter(*style)),
        InlineKind::Navigation(nav) => match nav {
            Navigation::Customize => "nav:customize".into(),
            Navigation::Browse => "nav:browse".into(),
            Navigation::Guide => "nav:guide".into(),
            Navigation::LinkedRoles(None) => "nav:linked-roles".into(),
            Navigation::LinkedRoles(Some(id)) => format!("nav:linked-roles:{id}"),
        },
        InlineKind::Text(_) | InlineKind::Code(_) => unreachable!("handled by runs"),
    }
}

fn letter(style: TimestampStyle) -> char {
    match style {
        TimestampStyle::ShortTime => 't',
        TimestampStyle::MediumTime => 'T',
        TimestampStyle::ShortDate => 'd',
        TimestampStyle::LongDate => 'D',
        TimestampStyle::LongDateShortTime => 'f',
        TimestampStyle::FullDateShortTime => 'F',
        TimestampStyle::ShortDateShortTime => 's',
        TimestampStyle::ShortDateMediumTime => 'S',
        TimestampStyle::Relative => 'R',
    }
}

pub(crate) fn text(source: &str, span: Span) -> &str {
    &source[span.start as usize..span.end as usize]
}

pub(crate) fn quoted(s: &str) -> String {
    let mut out = String::from('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() || (c.is_whitespace() && c != ' ') || invisible(c) => {
                let _ = write!(out, "\\u{{{:x}}}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn invisible(c: char) -> bool {
    matches!(
        u32::from(c),
        0xAD | 0x34F
            | 0x61C
            | 0x115F
            | 0x1160
            | 0x17B4
            | 0x17B5
            | 0x180E
            | 0x200B
            | 0x200C
            | 0x200E
            | 0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x2064
            | 0x2066..=0x206F
            | 0x3164
            | 0xFEFF
            | 0xFFA0
    )
}

#[test]
fn the_outline_writes_every_kind() {
    use crate::{Link, LinkId, SpoilerId};

    let source = "a b c :e: d https://x.co t";
    let s = |start: u32, end: u32| Span { start, end };
    let run = |kind, style, spoiler, link, source| Inline {
        kind,
        style,
        spoiler,
        link,
        source,
    };
    let block = |kind, containers: Vec<Container>, spoiler, inlines: Vec<Inline>, source| Block {
        kind,
        containers: containers.into(),
        spoiler,
        inlines: inlines.into(),
        source,
    };
    let doc = Document::new(
        vec![
            block(
                BlockKind::Paragraph,
                vec![],
                None,
                vec![
                    run(InlineKind::Text(s(0, 1)), Style::BOLD, None, None, s(0, 1)),
                    run(InlineKind::Break, Style::default(), None, None, s(1, 2)),
                    run(
                        InlineKind::Text(s(2, 3)),
                        Style(2 | 8),
                        Some(SpoilerId(0)),
                        None,
                        s(2, 3),
                    ),
                ],
                s(0, 3),
            ),
            block(
                BlockKind::Heading(HeadingLevel::Two),
                vec![
                    Container::Quote,
                    Container::ListItem {
                        ordered: true,
                        number: 3,
                        marker: true,
                    },
                ],
                None,
                vec![
                    run(
                        InlineKind::Code(s(4, 5)),
                        Style::default(),
                        None,
                        None,
                        s(3, 6),
                    ),
                    run(
                        InlineKind::UserMention(1),
                        Style::UNDERLINE,
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::RoleMention(2),
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::ChannelMention(3),
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(InlineKind::Everyone, Style::default(), None, None, s(0, 1)),
                    run(InlineKind::Here, Style::default(), None, None, s(0, 1)),
                    run(
                        InlineKind::Command {
                            name: s(0, 3),
                            id: 4,
                        },
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::CustomEmoji {
                            name: s(7, 8),
                            id: 5,
                            animated: false,
                        },
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::CustomEmoji {
                            name: s(7, 8),
                            id: 6,
                            animated: true,
                        },
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::Timestamp {
                            seconds: -1,
                            style: TimestampStyle::Relative,
                        },
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::Navigation(Navigation::LinkedRoles(Some(7))),
                        Style::default(),
                        None,
                        None,
                        s(0, 1),
                    ),
                    run(
                        InlineKind::Text(s(12, 24)),
                        Style::default(),
                        None,
                        Some(LinkId(0)),
                        s(12, 24),
                    ),
                ],
                s(0, 25),
            ),
            block(
                BlockKind::Subtext,
                vec![Container::ListItem {
                    ordered: false,
                    number: 1,
                    marker: false,
                }],
                None,
                vec![],
                s(0, 0),
            ),
            block(
                BlockKind::CodeBlock {
                    language: Some(s(25, 26)),
                },
                vec![],
                Some(SpoilerId(1)),
                vec![
                    run(
                        InlineKind::Code(s(0, 2)),
                        Style::default(),
                        None,
                        None,
                        s(0, 2),
                    ),
                    run(
                        InlineKind::Code(s(2, 3)),
                        Style::default(),
                        None,
                        None,
                        s(2, 3),
                    ),
                ],
                s(0, 3),
            ),
        ],
        vec![
            (
                Link {
                    url: "https://x.co/".into(),
                    kind: LinkKind::Plain,
                    suppress_embed: false,
                },
                s(12, 24),
            ),
            (
                Link {
                    url: "https://y.co/".into(),
                    kind: LinkKind::Masked {
                        title: Some("t \"q\"".into()),
                    },
                    suppress_embed: true,
                },
                s(0, 0),
            ),
        ],
        true,
        2,
    );
    assert_eq!(
        outline(&doc, source),
        [
            r#"p: {b}"a" br {i s sp0}"b""#,
            r#"> 3. h2: code"c" {u}@user:1 @role:2 #chan:3 @everyone @here /cmd"a b":4 emoji"e":5 aemoji"e":6 time:-1:R nav:linked-roles:7 {l0}"https://x.co""#,
            ". sub:",
            r#"code[t]{sp1}: "a b""#,
            "link0: <https://x.co/>",
            r#"link1: <https://y.co/> masked title="t \"q\"" suppress"#,
            "jumbo",
        ]
        .join("\n")
    );
}

#[test]
fn quoting_escapes_what_a_line_cant_show() {
    assert_eq!(quoted("a\"b\\c"), r#""a\"b\\c""#);
    assert_eq!(
        quoted("\n\r\t\u{0}\u{2028}\u{a0}\u{200b} é😀\u{200d}"),
        "\"\\n\\r\\t\\u{0}\\u{2028}\\u{a0}\\u{200b} é😀\u{200d}\""
    );
}
