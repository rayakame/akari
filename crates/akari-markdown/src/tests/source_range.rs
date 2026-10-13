use super::corpus::{cases, corpus_dir, files};
use crate::source_range;

fn select(source: &str, part: &str, nth: usize) -> std::ops::Range<usize> {
    let start = source.match_indices(part).nth(nth).unwrap().0;
    start..start + part.len()
}

#[test]
fn the_whole_selection_maps_to_the_whole_source() {
    for (file, text) in files(&corpus_dir()) {
        for case in cases(&file, &text) {
            let source = &case.input;
            assert_eq!(
                source_range(source, 0..source.len()),
                0..source.len(),
                "{file} / {}",
                case.name
            );
        }
    }
}

#[test]
fn the_source_range_never_splits_a_token() {
    let source = "a <t:0:R> b <@12> c <:e:1> d https://x.co/p e [link](https://y.co) f \\* g";
    for (inside, whole) in [
        ("t:0", "<t:0:R>"),
        ("@1", "<@12>"),
        (":e:", "<:e:1>"),
        ("x.co", "https://x.co/p"),
        ("ink", "[link](https://y.co)"),
        ("*", "\\*"),
    ] {
        let at = select(source, inside, 0);
        let whole = select(source, whole, 0);
        assert_eq!(source_range(source, at.clone()), whole.clone(), "{inside}");
        assert_eq!(
            source_range(source, at.start..at.start + 1),
            whole.clone(),
            "{inside} start"
        );
        assert_eq!(
            source_range(source, whole.start + 1..whole.end),
            whole,
            "{inside} from inside"
        );
    }
}

#[test]
fn a_selection_across_tokens_takes_both_whole() {
    let source = "x <@12> y https://x.co z";
    let from = source.find("@1").unwrap();
    let to = source.find("x.c").unwrap();
    assert_eq!(
        source_range(source, from..to),
        source.find('<').unwrap()..source.find(" z").unwrap()
    );
}

#[test]
fn styles_aren_t_repaired() {
    let source = "**bold** and _it_";
    assert_eq!(
        source_range(source, select(source, "ol", 0)),
        select(source, "ol", 0)
    );
    let from = source.find('l').unwrap();
    let to = source.find("it").unwrap() + 1;
    assert_eq!(source_range(source, from..to), from..to);
}

#[test]
fn a_selection_inside_code_is_the_code() {
    let source = "a `bcd` e\n```py\nxyz\n```";
    assert_eq!(
        source_range(source, select(source, "cd", 0)),
        select(source, "cd", 0)
    );
    assert_eq!(
        source_range(source, select(source, "yz", 0)),
        select(source, "yz", 0)
    );
}

#[test]
fn offsets_are_clamped_to_characters_and_the_source() {
    let source = "aé😀b";
    let e = source.find('é').unwrap();
    let face = source.find('😀').unwrap();
    assert_eq!(source_range(source, e + 1..face + 2), e..face + 4);
    assert_eq!(source_range(source, 0..99), 0..source.len());
    let (from, to) = (3, 1);
    assert_eq!(source_range(source, from..to), 3..3);
    assert_eq!(source_range(source, 99..120), source.len()..source.len());
}
