#![no_main]

use akari_markdown::source_range;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: (&str, u16, u16)| {
    let (source, a, b) = input;
    let (a, b) = (usize::from(a.min(b)), usize::from(a.max(b)));
    let range = source_range(source, a..b);
    let (start, end) = (a.min(source.len()), b.min(source.len()));
    assert!(range.start <= start && end <= range.end, "{range:?} for {a}..{b}");
    assert!(source.is_char_boundary(range.start) && source.is_char_boundary(range.end));
    assert_eq!(source_range(source, range.clone()), range);
});
