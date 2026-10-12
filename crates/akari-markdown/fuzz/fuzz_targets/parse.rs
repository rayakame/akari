#![no_main]

use akari_markdown::{InlineKind, STEPS_PER_BYTE, Span, parse_with_steps, source_range};
use libfuzzer_sys::fuzz_target;

fn check(source: &str, what: &str, span: Span) {
    assert!(span.start <= span.end && span.end as usize <= source.len(), "{what} {span:?}");
    assert!(source.is_char_boundary(span.start as usize), "{what} {span:?}");
    assert!(source.is_char_boundary(span.end as usize), "{what} {span:?}");
}

fuzz_target!(|source: &str| {
    let (doc, steps) = parse_with_steps(source);
    assert!(steps <= STEPS_PER_BYTE * (source.len() as u64 + 1), "{steps} steps for {} bytes", source.len());
    for block in doc.blocks() {
        check(source, "block", block.source);
        for inline in block.inlines.iter() {
            check(source, "inline", inline.source);
            if let InlineKind::Text(span) | InlineKind::Code(span) = inline.kind {
                check(source, "shown", span);
            }
        }
    }
    assert_eq!(source_range(source, 0..source.len()), 0..source.len());
});
