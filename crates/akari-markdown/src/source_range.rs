use std::ops::Range;

use crate::document::{InlineKind, Span};

/// The part of `source` behind `selection` (UTF-8 byte offsets, clamped to the source and widened
/// to whole characters), widened so it never cuts a token, an escape or a link in half. Styles'
/// delimiters and line prefixes aren't added; a selection inside code is the code.
pub fn source_range(source: &str, selection: Range<usize>) -> Range<usize> {
    let start = source.floor_char_boundary(selection.start.min(source.len()));
    let end = source
        .ceil_char_boundary(selection.end.min(source.len()))
        .max(start);
    let doc = crate::parse(source);
    let atoms = doc
        .blocks()
        .iter()
        .flat_map(|block| block.inlines.iter())
        .filter_map(|inline| match inline.kind {
            InlineKind::Text(shown) if shown != inline.source => Some(inline.source),
            InlineKind::Text(_) | InlineKind::Break | InlineKind::Code(_) => None,
            _ => Some(inline.source),
        })
        .chain(doc.link_spans().iter().copied());
    let (mut from, mut to) = (start, end);
    for Span { start: a, end: b } in atoms {
        let (a, b) = (a as usize, b as usize);
        if a < start && start < b {
            from = from.min(a);
        }
        if a < end && end < b {
            to = to.max(b);
        }
    }
    from..to
}
