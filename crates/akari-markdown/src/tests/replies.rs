// Discord's inline-reply rule set (canary 671549, `parseInlineReply` / INLINE_REPLY_RULES): the
// message's rules without block quotes, code blocks and line breaks; a fenced code block is
// inline code, and every line break shows as a space.

use proptest::prelude::*;

use crate::{BlockKind, Document, InlineKind, Style, line_text, parse, parse_inline_reply};

// What the line shows: its text and code, tokens as "•".
fn shown(doc: &Document, source: &str) -> String {
    let text = line_text(source);
    let mut out = String::new();
    for block in doc.blocks() {
        for inline in block.inlines.iter() {
            match inline.kind {
                InlineKind::Text(span) => out.push_str(&text[span.range()]),
                InlineKind::Code(span) => {
                    out.push('`');
                    out.push_str(&text[span.range()]);
                    out.push('`');
                }
                InlineKind::Break => out.push('\n'),
                _ => out.push('•'),
            }
        }
    }
    out
}

fn line(source: &str) -> String {
    let doc = parse_inline_reply(source);
    assert!(doc.blocks().len() <= 1, "{source:?}: {:?}", doc.blocks());
    for block in doc.blocks() {
        assert_eq!(block.kind, BlockKind::Paragraph, "{source:?}");
        assert!(block.containers.is_empty(), "{source:?}");
        assert!(!block.inlines.is_empty(), "{source:?}");
    }
    shown(&doc, source)
}

#[test]
fn styles_code_and_spoilers_stay() {
    let source = "**b** *i* __u__ ~~s~~ `c` ||hidden||";
    let doc = parse_inline_reply(source);

    assert_eq!(line(source), "b i u s `c` hidden");
    let inlines = &doc.blocks()[0].inlines;
    let styled = |style| inlines.iter().any(|inline| inline.style.contains(style));
    assert!(styled(Style::BOLD) && styled(Style::ITALIC));
    assert!(styled(Style::UNDERLINE) && styled(Style::STRIKE));
    assert_eq!(doc.spoiler_count(), 1);
    assert!(inlines.last().unwrap().spoiler.is_some());
    assert!(inlines[0].spoiler.is_none());
}

#[test]
fn mentions_emoji_and_timestamps_stay() {
    let doc = parse_inline_reply("<@1> <@&2> <#3> @everyone @here <:wave:5> <t:1700000000:R>");

    let kinds: Vec<InlineKind> = doc.blocks()[0]
        .inlines
        .iter()
        .map(|inline| inline.kind)
        .filter(|kind| !matches!(kind, InlineKind::Text(_)))
        .collect();
    assert!(
        matches!(
            kinds[..],
            [
                InlineKind::UserMention(1),
                InlineKind::RoleMention(2),
                InlineKind::ChannelMention(3),
                InlineKind::Everyone,
                InlineKind::Here,
                InlineKind::CustomEmoji { id: 5, .. },
                InlineKind::Timestamp { .. },
            ]
        ),
        "{kinds:?}"
    );
}

#[test]
fn links_stay_links() {
    let source = "# [docs](https://a.example)\nsee https://b.example";
    let doc = parse_inline_reply(source);

    assert_eq!(line(source), "docs see https://b.example");
    assert_eq!(doc.links().len(), 2);
    let linked: Vec<u16> = doc.blocks()[0]
        .inlines
        .iter()
        .filter_map(|inline| inline.link.map(|id| id.0))
        .collect();
    assert_eq!(linked, [0, 1]);
    assert_eq!(&*doc.links()[0].url, "https://a.example/");
    assert_eq!(&*doc.links()[1].url, "https://b.example/");
}

#[test]
fn emoji_are_never_jumbo() {
    assert!(parse("😀 <:wave:5>").jumbo());
    assert!(!parse_inline_reply("😀 <:wave:5>").jumbo());
}

#[test]
fn line_breaks_are_one_space() {
    for (source, expected) in [
        ("a\nb", "a b"),
        ("a\n\n\nb", "a b"),
        ("a\r\nb", "a b"),
        ("a   \nb", "a b"),
        ("\n\na\n\n", "a"),
        ("**a\nb**", "a b"),
        ("a\n", "a"),
    ] {
        assert_eq!(line(source), expected, "{source:?}");
    }
    let doc = parse_inline_reply("a\nb");
    assert!(
        doc.blocks()[0]
            .inlines
            .iter()
            .all(|inline| inline.kind != InlineKind::Break)
    );
}

#[test]
fn a_line_break_beside_spaces_is_still_one_space() {
    for (source, expected) in [
        ("a \n b", "a b"),
        ("a\n b", "a b"),
        ("a\n   b", "a   b"),
        ("a \r\n b", "a b"),
        ("**a**\n *b*", "a b"),
        ("# a\n b", "a b"),
        ("a\n\n b", "a b"),
    ] {
        assert_eq!(line(source), expected, "{source:?}");
    }
}

#[test]
fn a_code_block_of_nothing_but_whitespace_shows_nothing() {
    for (source, expected) in [
        ("```\n\n\n```", ""),
        ("```\n \n```", ""),
        ("```   ```", ""),
        ("a\n```\n\n\n```\nb", "a b"),
        ("a ```\n\n``` b", "a b"),
        ("```\n\n\n```\nb", "b"),
        ("` `", "` `"),
    ] {
        assert_eq!(line(source), expected, "{source:?}");
    }
    assert!(parse_inline_reply("```\n\n\n```").blocks().is_empty());
}

#[test]
fn a_quote_is_no_rule() {
    assert_eq!(line("> quoted"), "> quoted");
    assert_eq!(line(">>> two\nlines"), ">>> two lines");
    assert_eq!(line("> **bold**\nafter"), "> bold after");
}

#[test]
fn a_code_block_is_inline_code_on_one_line() {
    let source = "```rs\nlet a;\nlet b;\n```";
    let doc = parse_inline_reply(source);

    assert_eq!(line(source), "`let a; let b;`");
    assert!(
        matches!(doc.blocks()[0].inlines[..], [ref code] if matches!(code.kind, InlineKind::Code(_)))
    );
    assert_eq!(
        doc.blocks()[0].inlines[0].source.range(),
        0..source.len(),
        "the fences are the code's own delimiters"
    );
    assert_eq!(line("before ```x``` after"), "before `x` after");
    assert_eq!(line("```\n\n**not bold**\n\n```"), "`**not bold**`");
    assert_eq!(line("see\n```\ncode\n```\nend"), "see `code` end");
    assert_eq!(line("a `b` c"), "a `b` c");
    let hidden = parse_inline_reply("||```secret```||");
    assert!(hidden.blocks()[0].inlines[0].spoiler.is_some());
    assert!(hidden.blocks()[0].spoiler.is_some());
}

#[test]
fn headings_subtext_and_lists_read_as_text() {
    for (source, expected) in [
        ("# Title\nbody", "Title body"),
        ("## **Bold** title", "Bold title"),
        ("-# small\nmore", "small more"),
        ("- a\n- b", "a b"),
        ("* a\n* b", "a b"),
        ("1. a\n2. b", "a b"),
        ("- a\n  - nested\n- b", "a nested b"),
        ("intro\n- a\n- b\noutro", "intro a b outro"),
        ("# One\n## Two\n### Three", "One Two Three"),
    ] {
        assert_eq!(line(source), expected, "{source:?}");
    }
}

#[test]
fn a_spoiler_over_lines_stays_one_spoiler() {
    let doc = parse_inline_reply("||a\nb||");

    assert_eq!(doc.spoiler_count(), 1);
    let block = &doc.blocks()[0];
    assert!(block.spoiler.is_some());
    assert!(
        block
            .inlines
            .iter()
            .all(|inline| inline.spoiler == block.spoiler)
    );
    assert_eq!(shown(&doc, "||a\nb||"), "a b");

    let source = "||- a\n- b||";
    let listed = parse_inline_reply(source);
    assert_eq!(shown(&listed, source), "a b");
    assert!(listed.blocks()[0].spoiler.is_some(), "{listed:?}");
}

#[test]
fn nothing_to_show_is_no_block() {
    for source in ["", "\n", "\n\n\n", "   "] {
        assert!(
            parse_inline_reply(source).blocks().iter().all(|block| block
                .inlines
                .iter()
                .all(|inline| matches!(inline.kind, InlineKind::Text(_)))),
            "{source:?}"
        );
        assert!(line(source).trim().is_empty(), "{source:?}");
    }
}

#[test]
fn the_line_text_keeps_every_offset() {
    assert_eq!(line_text("a\r\nb\nc"), "a  b c");
    assert!(matches!(
        line_text("abc"),
        std::borrow::Cow::Borrowed("abc")
    ));
    let source = "é\n😀\r\nz";
    assert_eq!(line_text(source).len(), source.len());
}

#[test]
fn every_corpus_input_is_one_line() {
    use super::corpus::{cases, corpus_dir, files};
    let mut count = 0;
    for (file, text) in files(&corpus_dir()) {
        for case in cases(&file, &text) {
            assert!(
                !line(&case.input).contains(['\n', '\r']),
                "{file} / {}",
                case.name
            );
            count += 1;
        }
    }
    assert!(count > 100, "{count}");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn any_content_is_at_most_one_plain_line(source in super::properties::markdown()) {
        let doc = parse_inline_reply(&source);
        prop_assert!(doc.blocks().len() <= 1);
        prop_assert!(!doc.jumbo());
        let mut links = Vec::new();
        for block in doc.blocks() {
            prop_assert_eq!(block.kind, BlockKind::Paragraph);
            prop_assert!(block.containers.is_empty());
            for inline in block.inlines.iter() {
                prop_assert!(inline.kind != InlineKind::Break);
                prop_assert!(source.get(inline.source.range()).is_some());
                if let InlineKind::Text(span) | InlineKind::Code(span) = inline.kind {
                    prop_assert!(source.get(span.range()).is_some());
                }
                links.extend(inline.link.map(|id| usize::from(id.0)));
            }
        }
        prop_assert!(links.iter().all(|id| *id < doc.links().len()));
        prop_assert!(!shown(&doc, &source).contains(['\n', '\r']));
    }
}
