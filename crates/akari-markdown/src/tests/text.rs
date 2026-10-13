use super::outline::text;
use super::spans;
use crate::{InlineKind, Span, parse};

#[test]
fn spans_point_into_the_source() {
    for source in [
        "",
        "a",
        "a\nb",
        "a   \n\n b  ",
        "\\*a\\* ¯\\_(ツ)_/¯ \\",
        "é😀\u{200d}👍🏽\n\r\n\u{2028}",
    ] {
        let doc = parse(source);
        for (what, span) in spans(&doc) {
            assert!(
                span.start <= span.end && span.end as usize <= source.len(),
                "{what} {span:?} outside {source:?}"
            );
            assert!(
                source.is_char_boundary(span.start as usize)
                    && source.is_char_boundary(span.end as usize),
                "{what} {span:?} splits a character of {source:?}"
            );
        }
    }
}

#[test]
fn escapes_split_text_runs() {
    let source = "a\\*b";
    let doc = parse(source);
    let inlines = &doc.blocks()[0].inlines;
    let shown: Vec<(&str, &str)> = inlines
        .iter()
        .map(|i| match i.kind {
            InlineKind::Text(s) => (text(source, s), text(source, i.source)),
            _ => panic!("only text expected: {i:?}"),
        })
        .collect();
    assert_eq!(shown, [("a", "a"), ("*", "\\*"), ("b", "b")]);
}

#[test]
fn line_endings_and_controls() {
    let source = "a  \r\nb\rc\u{0}\u{2028}\td";
    let doc = parse(source);
    let kinds: Vec<(String, Span)> = doc.blocks()[0]
        .inlines
        .iter()
        .map(|i| match i.kind {
            InlineKind::Text(s) => (format!("{:?}", text(source, s)), i.source),
            InlineKind::Break => ("br".to_owned(), i.source),
            _ => panic!("only text and breaks expected: {i:?}"),
        })
        .collect();
    assert_eq!(
        kinds,
        [
            ("\"a  \"".to_owned(), Span { start: 0, end: 3 }),
            ("br".to_owned(), Span { start: 3, end: 5 }),
            (
                format!("{:?}", "b\rc\u{0}\u{2028}\td"),
                Span {
                    start: 5,
                    end: source.len() as u32
                }
            ),
        ]
    );
}

#[test]
fn trailing_spaces_belong_to_the_break() {
    let source = "a   \nb";
    let doc = parse(source);
    let inlines = &doc.blocks()[0].inlines;
    assert_eq!(inlines[0].kind, InlineKind::Text(Span { start: 0, end: 1 }));
    assert_eq!(inlines[1].kind, InlineKind::Break);
    assert_eq!(inlines[1].source, Span { start: 1, end: 5 });
}

#[test]
fn a_paragraph_covers_its_source() {
    let source = "a\nb\n";
    let doc = parse(source);
    assert_eq!(doc.blocks().len(), 1);
    assert_eq!(doc.blocks()[0].source, Span { start: 0, end: 4 });
}
