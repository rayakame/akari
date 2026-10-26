// Discord's embed rule sets (canary 649848, modules 46054 and 999915): titles and field names
// without code blocks, line breaks, mentions or subtext, and without links when the title is one;
// descriptions and field values with the message's rules, lists and headings only for messages
// after 2023-03-22, never headings in a field's value.

use crate::{BlockKind, Container, Document, EmbedText, InlineKind, parse, parse_embed};

fn kinds(doc: &Document) -> Vec<BlockKind> {
    doc.blocks().iter().map(|block| block.kind).collect()
}

fn has_inline(doc: &Document, test: impl Fn(&InlineKind) -> bool) -> bool {
    doc.blocks()
        .iter()
        .flat_map(|block| block.inlines.iter())
        .any(|inline| test(&inline.kind))
}

fn in_list(doc: &Document) -> bool {
    doc.blocks().iter().any(|block| {
        block
            .containers
            .iter()
            .any(|c| matches!(c, Container::ListItem { .. }))
    })
}

fn in_quote(doc: &Document) -> bool {
    doc.blocks()
        .iter()
        .any(|block| block.containers.contains(&Container::Quote))
}

const MIXED: &str = "**bold** <@1> <@&2> <#3> [x](https://example.com) https://example.org";

#[test]
fn a_title_keeps_styles_but_no_mentions_or_blocks() {
    let title = parse_embed(MIXED, EmbedText::Title { links: true });

    assert!(has_inline(&title, |kind| matches!(
        kind,
        InlineKind::Text(_)
    )));
    assert!(
        title.blocks()[0]
            .inlines
            .iter()
            .any(|inline| inline.style.contains(crate::Style::BOLD)),
        "bold"
    );
    assert!(!has_inline(&title, |kind| matches!(
        kind,
        InlineKind::UserMention(_) | InlineKind::RoleMention(_) | InlineKind::ChannelMention(_)
    )));
    assert_eq!(
        title.links().len(),
        2,
        "links stay while the title isn't one"
    );
    let everyone = parse_embed("@everyone @here", EmbedText::Title { links: true });
    assert!(!has_inline(&everyone, |kind| matches!(
        kind,
        InlineKind::Everyone | InlineKind::Here
    )));
    for source in [
        "```\ncode\n```",
        "# heading",
        "- item",
        "-# small",
        "> quote",
    ] {
        let doc = parse_embed(source, EmbedText::Title { links: true });
        assert_eq!(kinds(&doc), [BlockKind::Paragraph], "{source}");
        assert!(!in_list(&doc) && !in_quote(&doc), "{source}");
    }
}

#[test]
fn a_linked_title_has_no_links_of_its_own() {
    let title = parse_embed(MIXED, EmbedText::Title { links: false });

    assert!(title.links().is_empty());
    for source in [
        "<https://example.com>",
        "<mail@example.com>",
        "<+1 555 0100>",
    ] {
        assert!(
            !parse(source).links().is_empty(),
            "a link in a message: {source}"
        );
        let title = parse_embed(source, EmbedText::Title { links: false });
        assert!(title.links().is_empty(), "{source}");
    }
}

#[test]
fn a_description_has_the_messages_rules() {
    let description = parse_embed(MIXED, EmbedText::Description { lists: true });

    assert_eq!(description, parse(MIXED));
    let code = parse_embed("```\ncode\n```", EmbedText::Description { lists: false });
    assert!(matches!(kinds(&code)[0], BlockKind::CodeBlock { .. }));
}

#[test]
fn lists_and_headings_follow_the_messages_date() {
    let source = "# heading\n- item";
    let new = parse_embed(source, EmbedText::Description { lists: true });
    assert!(matches!(kinds(&new)[0], BlockKind::Heading(_)));
    assert!(in_list(&new));

    let old = parse_embed(source, EmbedText::Description { lists: false });
    assert!(
        kinds(&old).iter().all(|kind| *kind == BlockKind::Paragraph),
        "{:?}",
        kinds(&old)
    );
    assert!(!in_list(&old));
    for list in ["* item", "1. item"] {
        assert!(
            in_list(&parse_embed(list, EmbedText::Description { lists: true })),
            "{list}"
        );
        assert!(
            !in_list(&parse_embed(list, EmbedText::Description { lists: false })),
            "{list}"
        );
    }
}

#[test]
fn a_fields_value_never_has_headings() {
    let value = parse_embed("# heading\n- item", EmbedText::FieldValue { lists: true });

    assert!(
        kinds(&value)
            .iter()
            .all(|kind| !matches!(kind, BlockKind::Heading(_)))
    );
    assert!(in_list(&value), "lists stay");
}

// Module 521981: exactly one top-level link, bare, in angle brackets or masked.
#[test]
fn a_single_link_is_the_whole_content() {
    for source in [
        "https://example.com/a.gif",
        "<https://example.com/a.gif>",
        "[gif](https://example.com/a.gif)",
        "[||gif|| here](https://example.com/a.gif)",
    ] {
        assert!(parse(source).is_single_link(), "{source}");
    }
    for source in [
        "",
        "look https://example.com/a.gif",
        "https://example.com/a https://example.com/b",
        "||https://example.com/a.gif||",
        "**https://example.com/a.gif**",
        "> https://example.com/a.gif",
        "https://example.com/a.gif\n```\ncode\n```",
    ] {
        assert!(!parse(source).is_single_link(), "{source}");
    }
}

// Module 521981 `u`: any link inside a spoiler, anywhere in the content.
#[test]
fn a_spoilered_link_is_found_anywhere() {
    for source in [
        "||https://example.com||",
        "see ||the [clip](https://example.com)||",
        "> quote ||https://example.com||",
    ] {
        assert!(parse(source).has_spoilered_link(), "{source}");
    }
    for source in [
        "||spoiler|| https://example.com",
        "https://example.com",
        "||no link||",
    ] {
        assert!(!parse(source).has_spoilered_link(), "{source}");
    }
}
