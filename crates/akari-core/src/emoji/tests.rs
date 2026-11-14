use super::*;

fn emoji(text: &str) -> UnicodeEmoji {
    UnicodeEmoji::get(text).unwrap()
}

fn listed(newest: EmojiVersion) -> Vec<UnicodeEmoji> {
    unicode_groups(newest)
        .into_iter()
        .flat_map(|group| group.emojis)
        .collect()
}

// Unicode 18.0's emoji-test.txt without skin-toned variants and components: a new crate version
// that changes the data shows here first.
#[test]
fn the_catalog_is_unicode_18s() {
    assert_eq!(EmojiVersion::NEWEST, EmojiVersion::new(18, 0));
    let all = listed(EmojiVersion::NEWEST);
    assert_eq!(all.len(), 1923);
    assert!(
        all.iter()
            .any(|emoji| emoji.version() == EmojiVersion::NEWEST)
    );
}

#[test]
fn groups_come_in_cldr_order_each_in_unicodes_order() {
    let groups = unicode_groups(EmojiVersion::NEWEST);

    let order: Vec<EmojiGroup> = groups.iter().map(|group| group.group).collect();
    assert_eq!(order, EmojiGroup::ALL);
    let first: Vec<&str> = groups[0].emojis[..5].iter().map(|e| e.as_str()).collect();
    assert_eq!(first, ["😀", "😃", "😄", "😁", "😆"]);
    let flags: Vec<&str> = groups[8].emojis[..2].iter().map(|e| e.as_str()).collect();
    assert_eq!(flags, ["🏁", "🚩"]);
    for group in &groups {
        assert!(
            group
                .emojis
                .iter()
                .all(|emoji| emoji.group() == group.group)
        );
    }
}

#[test]
fn skin_toned_variants_arent_listed() {
    let all = listed(EmojiVersion::NEWEST);

    assert!(all.iter().any(|emoji| emoji.as_str() == "👍"));
    assert!(!all.iter().any(|emoji| emoji.as_str() == "👍🏽"));
    assert!(emoji("👍").has_skin_tones());
    assert!(!emoji("😀").has_skin_tones());
}

// A host leaves out what its font can't draw: emoji newer than the version it gives.
#[test]
fn emoji_newer_than_the_hosts_version_are_left_out() {
    let cutoff = EmojiVersion::new(15, 0);
    let all = listed(cutoff);

    assert!(all.iter().all(|emoji| emoji.version() <= cutoff));
    assert!(all.iter().any(|emoji| emoji.as_str() == "🫠"), "14.0 stays");
    assert!(all.iter().any(|emoji| emoji.as_str() == "🫨"), "15.0 stays");
    assert!(!all.iter().any(|emoji| emoji.as_str() == "🫩"), "16.0 goes");
    assert!(all.len() < listed(EmojiVersion::NEWEST).len());
}

#[test]
fn a_group_the_cutoff_empties_is_left_out() {
    let groups = unicode_groups(EmojiVersion::new(0, 0));

    assert!(groups.is_empty());
}

// Discord's users' names: gemoji's words, as akari-markdown's `emoji_shortcode` gives them.
#[test]
fn names_are_the_short_names_discords_users_type() {
    let cases = [
        ("👍", "thumbsup"),
        ("😆", "laughing"),
        ("💯", "100"),
        ("💖", "sparkling_heart"),
        ("❤\u{fe0f}", "heart"),
        ("🇩🇪", "flag_de"),
        ("🇺🇸", "flag_us"),
        (
            "🏴\u{e0067}\u{e0062}\u{e0065}\u{e006e}\u{e0067}\u{e007f}",
            "england",
        ),
        ("#\u{fe0f}\u{20e3}", "hash"),
        ("©\u{fe0f}", "copyright"),
        ("\u{1faeb}", "cracking_face"),
        ("🦖", "t_rex"),
        ("🧑\u{200d}🧑\u{200d}🧒", "family_adult_adult_child"),
        ("👍🏽", "thumbsup_tone3"),
        ("🧑🏻\u{200d}🤝\u{200d}🧑🏼", "people_holding_hands_tone1_tone2"),
    ];
    for (text, name) in cases {
        assert_eq!(emoji(text).name(), name, "{text}");
    }
}

#[test]
fn other_short_codes_leave_the_name_out() {
    let codes = |text: &str| emoji(text).shortcodes().collect::<Vec<_>>();

    assert_eq!(codes("👍"), ["+1"]);
    assert_eq!(codes("😆"), ["satisfied"]);
    assert_eq!(codes("🇩🇪"), ["de"]);
    assert!(codes("😀").is_empty());
    assert!(codes("\u{1faeb}").is_empty());
}

#[test]
fn an_emoji_has_its_cldr_name_group_and_version() {
    let up = emoji("👍");

    assert_eq!(up.cldr_name(), "thumbs up");
    assert_eq!(up.group(), EmojiGroup::PeopleAndBody);
    assert_eq!(up.version(), EmojiVersion::new(0, 6));
    assert_eq!(emoji("🇩🇪").cldr_name(), "flag: Germany");
    assert_eq!(emoji("🫠").version(), EmojiVersion::new(14, 0));
}

#[test]
fn an_unqualified_emoji_is_found_as_the_qualified_one() {
    assert_eq!(emoji("❤").as_str(), "❤\u{fe0f}");
    assert_eq!(emoji("👍🏽").as_str(), "👍🏽");
    assert!(UnicodeEmoji::get("a").is_none());
    assert!(UnicodeEmoji::get("").is_none());
    assert!(UnicodeEmoji::get("\u{1f1e6}").is_none(), "a lone letter");
}

#[test]
fn versions_order_by_major_then_minor() {
    assert!(EmojiVersion::new(13, 1) > EmojiVersion::new(13, 0));
    assert!(EmojiVersion::new(13, 0) > EmojiVersion::new(5, 0));
    assert!(EmojiVersion::new(0, 7) > EmojiVersion::new(0, 6));
}
