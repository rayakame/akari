use crate::emoji::{MAX_EMOJI_LEN, could_start, emoji_len};

fn len(s: &str) -> Option<usize> {
    emoji_len(s, 0, &mut 0)
}

#[test]
fn the_emoji_list_covers_unicode_16() {
    assert_eq!(len("\u{1FAE9}"), Some(4), "face with bags under eyes");
}

#[test]
fn every_emoji_starts_with_a_candidate() {
    for emoji in emojis::iter() {
        let tones = emoji.skin_tones().into_iter().flatten();
        for e in std::iter::once(emoji).chain(tones) {
            let first = e.as_str().chars().next().unwrap();
            assert!(could_start(first), "{:?} starts with {first:?}", e.as_str());
        }
    }
}

#[test]
fn the_longest_emoji_in_the_grapheme_wins() {
    assert_eq!(
        len("👨\u{200d}👩\u{200d}👧x"),
        Some("👨\u{200d}👩\u{200d}👧".len())
    );
    assert_eq!(len("😀\u{301}"), Some(4));
    assert_eq!(len("🇩🇪🇩🇪"), Some(8));
    assert_eq!(len("a😀"), None);
    assert_eq!(len("1"), None);
}

#[test]
fn no_emoji_is_longer_than_the_search() {
    let longest = emojis::iter()
        .flat_map(|e| std::iter::once(e).chain(e.skin_tones().into_iter().flatten()))
        .map(|e| e.as_str().len())
        .max()
        .unwrap();
    assert!(longest < MAX_EMOJI_LEN, "{longest} bytes");
}

// A Unicode emoji's name as Discord shows it in its hover card and popout: gemoji's word, a skin
// tone as `_tone{n}`, a flag as `flag_xx`; ©, ®, ™ and text aren't emoji.
#[test]
fn unicode_emoji_have_discords_names() {
    for (emoji, name) in [
        ("😀", "grinning"),
        ("🍕", "pizza"),
        ("👍", "thumbsup"),
        ("👍🏽", "thumbsup_tone3"),
        ("👋🏿", "wave_tone5"),
        ("🇩🇪", "flag_de"),
        ("🇯🇵", "flag_jp"),
        ("❤️", "heart"),
        ("❤", "heart"),
        ("👨‍💻", "man_technologist"),
    ] {
        assert_eq!(
            crate::emoji_shortcode(emoji).as_deref(),
            Some(name),
            "{emoji}"
        );
    }
    for text in ["a", "", "™", "©", "😀😀", "1"] {
        assert_eq!(crate::emoji_shortcode(text), None, "{text:?}");
    }
}
