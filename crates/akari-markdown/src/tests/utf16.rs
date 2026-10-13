use crate::utf16::Utf16Map;

#[test]
fn ascii_is_the_identity() {
    let map = Utf16Map::new("hello *world*");
    for i in 0..=13 {
        assert_eq!(map.to_utf16(i), i);
        assert_eq!(map.to_byte(i), i);
    }
}

#[test]
fn non_ascii_converts_both_ways() {
    let source = "aé😀日b";
    let map = Utf16Map::new(source);
    // a(1) é(2 bytes, 1 unit) 😀(4 bytes, 2 units) 日(3 bytes, 1 unit) b
    for (byte, unit) in [(0, 0), (1, 1), (3, 2), (7, 4), (10, 5), (11, 6)] {
        assert_eq!(map.to_utf16(byte), unit, "byte {byte}");
        assert_eq!(map.to_byte(unit), byte, "unit {unit}");
    }
}

#[test]
fn offsets_inside_a_character_round_down() {
    let map = Utf16Map::new("a😀b");
    assert_eq!(map.to_byte(2), 1, "between the surrogates");
    assert_eq!(map.to_utf16(2), 1, "inside the UTF-8 bytes");
    assert_eq!(map.to_utf16(4), 1);
}

#[test]
fn offsets_past_the_end_clamp() {
    let map = Utf16Map::new("é😀");
    assert_eq!(map.to_utf16(99), 3);
    assert_eq!(map.to_byte(99), 6);
}

#[test]
fn every_boundary_round_trips() {
    let source = "Grüße 😀👍🏽 日本語 🇩🇪 a\u{301} \u{10000}x";
    let map = Utf16Map::new(source);
    let mut unit = 0;
    for (byte, c) in source.char_indices() {
        assert_eq!(map.to_utf16(byte as u32), unit, "byte {byte}");
        assert_eq!(map.to_byte(unit), byte as u32, "unit {unit}");
        unit += c.len_utf16() as u32;
    }
    assert_eq!(map.to_utf16(source.len() as u32), unit);
    assert_eq!(map.to_byte(unit), source.len() as u32);
}
