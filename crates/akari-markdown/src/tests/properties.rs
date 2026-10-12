use proptest::prelude::*;

use super::bounds::assert_bounded;
use super::spans;
use crate::{Document, InlineKind, parse, source_range};

fn markdown() -> impl Strategy<Value = String> {
    let delimiter = prop::sample::select(
        "*_~|`>#-[]()<@&:!t0123456789 \n\\.\"'/"
            .chars()
            .collect::<Vec<_>>(),
    );
    let letter = prop::sample::select("abcehpst".chars().collect::<Vec<_>>());
    let c = prop_oneof![8 => delimiter, 3 => letter, 1 => any::<char>()];
    prop::collection::vec(c, 0..200).prop_map(|cs| cs.into_iter().collect())
}

fn shown(doc: &Document, source: &str) -> String {
    let mut out = String::new();
    for (i, block) in doc.blocks().iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        for inline in block.inlines.iter() {
            match inline.kind {
                InlineKind::Text(span) | InlineKind::Code(span) => {
                    out.push_str(&source[span.range()])
                }
                InlineKind::Break => out.push('\n'),
                _ => out.push_str(&source[inline.source.range()]),
            }
        }
    }
    out
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn parsing_never_panics_and_stays_bounded(source in markdown()) {
        assert_bounded(&source);
    }

    #[test]
    fn ranges_are_on_char_boundaries_ordered_and_inside(source in markdown()) {
        let doc = parse(&source);
        for (what, span) in spans(&doc) {
            prop_assert!(span.start <= span.end && span.end as usize <= source.len(), "{what} {span:?}");
            prop_assert!(source.is_char_boundary(span.start as usize), "{what} {span:?}");
            prop_assert!(source.is_char_boundary(span.end as usize), "{what} {span:?}");
        }
        for pair in doc.blocks().windows(2) {
            prop_assert!(pair[0].source.start <= pair[1].source.start, "blocks out of order");
        }
        for block in doc.blocks() {
            for pair in block.inlines.windows(2) {
                prop_assert!(pair[0].source.start <= pair[1].source.start, "inlines out of order");
            }
        }
    }

    #[test]
    fn plain_words_parse_to_themselves(source in "[a-z]{1,10}( [a-z]{1,10}){0,20}") {
        let doc = parse(&source);
        prop_assert_eq!(doc.blocks().len(), 1);
        prop_assert_eq!(shown(&doc, &source), source);
    }

    #[test]
    fn escaped_text_shows_as_typed(text in markdown().prop_map(|s| s.replace(['\n', '\r'], ""))) {
        let escaped: String = text
            .chars()
            .flat_map(|c| {
                let escape = !(c.is_ascii_alphanumeric() || c.is_whitespace());
                escape.then_some('\\').into_iter().chain([c])
            })
            .collect();
        let doc = parse(&escaped);
        prop_assert!(doc.blocks().len() <= 1);
        prop_assert_eq!(shown(&doc, &escaped), text);
    }

    #[test]
    fn the_whole_selection_maps_to_the_whole_source(source in markdown()) {
        prop_assert_eq!(source_range(&source, 0..source.len()), 0..source.len());
    }

    #[test]
    fn a_source_range_holds_the_selection_and_is_stable(source in markdown(), a in 0usize..250, b in 0usize..250) {
        let (a, b) = (a.min(b), a.max(b));
        let range = source_range(&source, a..b);
        let (start, end) = (a.min(source.len()), b.min(source.len()));
        prop_assert!(range.start <= start && end <= range.end, "{range:?} for {a}..{b}");
        prop_assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
        prop_assert_eq!(source_range(&source, range.clone()), range);
    }
}

#[test]
fn long_runs() {
    let word = "a".repeat(4000);
    assert_bounded(&word);
    let doc = parse(&word);
    assert_eq!(doc.blocks().len(), 1);
    assert_eq!(doc.blocks()[0].inlines.len(), 1);

    let url = "https://example.com/".to_owned() + &"x".repeat(3980);
    assert_bounded(&url);
    let doc = parse(&url);
    assert_eq!(doc.links().len(), 1);
    assert_eq!(doc.blocks()[0].inlines.len(), 1);

    let newlines = "\n".repeat(2000);
    assert_bounded(&newlines);
    let doc = parse(&newlines);
    assert_eq!(doc.blocks().len(), 1);
    assert_eq!(doc.blocks()[0].inlines.len(), 1999);

    let quote = "> a\n".repeat(2000);
    assert_bounded(&quote);
    let doc = parse(&quote);
    assert_eq!(doc.blocks().len(), 1);
    assert_eq!(doc.blocks()[0].containers.len(), 1);
    assert_eq!(doc.blocks()[0].inlines.len(), 3999);
}
