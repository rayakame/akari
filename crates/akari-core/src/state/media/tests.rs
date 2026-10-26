use super::*;
use crate::model::Snowflake;
use crate::state::settings::ExplicitContentSettings;

fn display() -> DisplaySettings {
    DisplaySettings::default()
}

fn redactions(
    guilds: Redaction,
    friend_dm: Redaction,
    non_friend_dm: Redaction,
) -> ExplicitContentSettings {
    ExplicitContentSettings {
        guilds,
        friend_dm,
        non_friend_dm,
    }
}

fn file(name: &str, flags: u64, version: Option<u32>) -> Attachment {
    Attachment {
        id: Snowflake::new(1),
        filename: name.into(),
        title: None,
        description: None,
        content_type: None,
        size: 1,
        url: "https://cdn.discordapp.com/attachments/1/1/a".into(),
        proxy_url: "https://media.discordapp.net/attachments/1/1/a".into(),
        width: Some(640),
        height: Some(480),
        flags,
        placeholder: None,
        content_scan_version: version,
    }
}

const EVERYTHING: Covered = Covered {
    explicit: true,
    gore: true,
};
const EXPLICIT: Covered = Covered {
    explicit: true,
    gore: false,
};
const NOTHING: Covered = Covered {
    explicit: false,
    gore: false,
};
const WAITING: Scan = Scan {
    settled: false,
    newest: Some(4),
};

// Module 632119 for explicit, 444802 for gore: unset settings by the legacy filter (default
// non-friends) and by an explicit "under 18" only; `None` takes the adult path.
#[test]
fn unset_settings_resolve_as_the_client_resolves_them() {
    let adult = |place, filter| {
        let mut display = display();
        display.explicit_content_filter = filter;
        (
            covered(&display, place, Some(true)),
            covered(&display, place, None),
        )
    };
    use ExplicitContentFilter::{All, Disabled, NonFriends};
    for filter in [Disabled, NonFriends, All] {
        let (allowed, unknown) = adult(Place::Guild, filter);
        assert_eq!(allowed, NOTHING, "{filter:?}");
        assert_eq!(unknown, allowed);
        let (friend, _) = adult(Place::FriendDm, filter);
        assert_eq!(
            friend,
            Covered {
                explicit: filter == All,
                gore: false
            },
            "{filter:?}"
        );
        let (stranger, _) = adult(Place::NonFriendDm, filter);
        assert_eq!(
            stranger,
            Covered {
                explicit: filter != Disabled,
                gore: true
            },
            "{filter:?}"
        );
    }
    for place in [Place::Guild, Place::FriendDm, Place::NonFriendDm] {
        for filter in [Disabled, NonFriends, All] {
            let mut display = display();
            display.explicit_content_filter = filter;
            assert_eq!(
                covered(&display, place, Some(false)),
                EVERYTHING,
                "{place:?} {filter:?}"
            );
        }
    }
}

#[test]
fn set_settings_win_and_self_harm_covers_nothing() {
    let mut display = display();
    display.explicit_content = redactions(Redaction::Blur, Redaction::Block, Redaction::Show);
    display.gore = redactions(Redaction::Show, Redaction::Unset, Redaction::Show);
    display.self_harm = redactions(Redaction::Block, Redaction::Block, Redaction::Block);

    assert_eq!(covered(&display, Place::Guild, Some(true)), EXPLICIT);
    assert_eq!(covered(&display, Place::FriendDm, Some(true)), EXPLICIT);
    assert_eq!(covered(&display, Place::NonFriendDm, Some(true)), NOTHING);
    assert_eq!(
        covered(&display, Place::NonFriendDm, Some(false)),
        NOTHING,
        "set beats minor"
    );
}

#[test]
fn flagged_media_is_covered_where_its_harm_is() {
    let explicit = file("a.png", CONTAINS_EXPLICIT, Some(4));
    let gore = file("a.png", CONTAINS_GORE, Some(4));

    assert_eq!(
        media_cover(&explicit, EXPLICIT, WAITING, false),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(media_cover(&gore, EXPLICIT, WAITING, false), None);
    assert_eq!(
        media_cover(&gore, EVERYTHING, WAITING, false),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(media_cover(&explicit, NOTHING, WAITING, false), None);
    // Self-harm (128) covers nothing.
    assert_eq!(
        media_cover(&file("a.png", 1 << 7, Some(4)), EVERYTHING, WAITING, false),
        None
    );
}

// Module 282108 `P`: pending without a version; with gore covered, also below min(READY's, 4);
// a settled scan never; nothing pending where nothing is covered.
#[test]
fn unscanned_media_waits_for_its_scan() {
    let fresh = file("a.png", 0, None);
    let old = file("a.png", 0, Some(3));
    let ineligible = file("a.png", 0, Some(0));
    let settled = Scan {
        settled: true,
        newest: Some(4),
    };

    assert_eq!(
        media_cover(&fresh, EXPLICIT, WAITING, false),
        Some(MediaCover::Pending)
    );
    assert_eq!(media_cover(&fresh, NOTHING, WAITING, false), None);
    assert_eq!(media_cover(&fresh, EXPLICIT, settled, false), None);
    assert_eq!(media_cover(&old, EXPLICIT, WAITING, false), None);
    assert_eq!(media_cover(&ineligible, EXPLICIT, WAITING, false), None);
    assert_eq!(
        media_cover(&old, EVERYTHING, WAITING, false),
        Some(MediaCover::Pending)
    );
    let older_ready = Scan {
        settled: false,
        newest: Some(2),
    };
    assert_eq!(media_cover(&old, EVERYTHING, older_ready, false), None);
    // A newer READY version (self-harm's 5) doesn't make version-4 scans old.
    let newer_ready = Scan {
        settled: false,
        newest: Some(5),
    };
    assert_eq!(
        media_cover(&file("a.png", 0, Some(4)), EVERYTHING, newer_ready, false),
        None
    );
    let no_ready = Scan {
        settled: false,
        newest: None,
    };
    assert_eq!(
        media_cover(&old, EVERYTHING, no_ready, false),
        Some(MediaCover::Pending)
    );
}

// Module 294520: a harm beats a pending scan, which beats a spoiler. Showing spoilers never lifts
// the others (departure D-3: Discord's setting does).
#[test]
fn the_most_important_cover_wins() {
    let spoiler = 1 << 3;
    let flagged = file("a.png", spoiler | CONTAINS_EXPLICIT, Some(4));
    let unscanned = file("a.png", spoiler, None);
    let plain = file("a.png", spoiler, Some(4));

    assert_eq!(
        media_cover(&flagged, EXPLICIT, WAITING, false),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(
        media_cover(&flagged, EXPLICIT, WAITING, true),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(
        media_cover(&unscanned, EXPLICIT, WAITING, true),
        Some(MediaCover::Pending)
    );
    assert_eq!(
        media_cover(&plain, EXPLICIT, WAITING, false),
        Some(MediaCover::Spoiler)
    );
    assert_eq!(media_cover(&plain, EXPLICIT, WAITING, true), None);
    assert_eq!(
        media_cover(&plain, NOTHING, WAITING, false),
        Some(MediaCover::Spoiler)
    );
}

fn message(attachments: Vec<Attachment>) -> Message {
    let mut message = crate::cache::tests::message(1, 1, "");
    message.attachments = attachments.into();
    message
}

fn placed(shown: &[ShownAttachment]) -> Vec<(usize, AttachmentDisplay)> {
    shown
        .iter()
        .map(|shown| (shown.index, shown.display))
        .collect()
}

// Module 394839: the mosaic first, then the files, each in the message's order; IS_THUMBNAIL
// left out. Sized files that aren't media and 1×1 images are file rows (D-6).
#[test]
fn media_comes_first_and_files_follow() {
    let mut pixel = file("pixel.png", 0, Some(4));
    pixel.width = Some(1);
    pixel.height = Some(1);
    let mut sizeless = file("b.png", 0, Some(4));
    sizeless.width = None;
    let shown = message(vec![
        file("notes.zip", 0, Some(4)),
        file("a.png", 0, Some(4)),
        file("tile.png", 1 << 1, Some(4)),
        file("clip.mp4", 0, Some(4)),
        pixel,
        sizeless,
        file("song.mp3", 0, Some(4)),
    ]);

    let rows = shown_attachments(&shown, &display(), |_| None);

    use AttachmentDisplay::{File, Media};
    assert_eq!(
        placed(&rows),
        [
            (1, Media),
            (3, Media),
            (0, File),
            (4, File),
            (5, File),
            (6, File)
        ]
    );
}

#[test]
fn without_inline_media_every_attachment_is_a_file_row_without_a_cover() {
    let mut display = display();
    display.inline_attachment_media = false;
    let shown = message(vec![file("a.png", 1 << 3, None), file("b.mp4", 0, None)]);

    let rows = shown_attachments(&shown, &display, |_| Some(MediaCover::Spoiler));

    assert_eq!(
        placed(&rows),
        [(0, AttachmentDisplay::File), (1, AttachmentDisplay::File)]
    );
    assert!(rows.iter().all(|row| row.cover.is_none()));
}

#[test]
fn only_media_takes_its_cover() {
    let shown = message(vec![
        file("notes.zip", 1 << 3, None),
        file("a.png", 1 << 3, None),
    ]);

    let rows = shown_attachments(&shown, &display(), |_| Some(MediaCover::Spoiler));

    assert_eq!(rows[0].cover, Some(MediaCover::Spoiler));
    assert_eq!(rows[1].cover, None);
}

#[test]
fn spoilers_show_by_the_setting_and_a_moderators_permission() {
    let with = |setting| {
        let mut display = display();
        display.render_spoilers = setting;
        display
    };
    let moderator = Some(Permissions::VIEW_CHANNEL | Permissions::MANAGE_MESSAGES);
    let member = Some(Permissions::VIEW_CHANNEL);

    assert!(spoilers_shown(&with(SpoilerSetting::Always), None));
    assert!(!spoilers_shown(&with(SpoilerSetting::OnClick), moderator));
    assert!(spoilers_shown(
        &with(SpoilerSetting::IfModerator),
        moderator
    ));
    assert!(!spoilers_shown(&with(SpoilerSetting::IfModerator), member));
    assert!(!spoilers_shown(&with(SpoilerSetting::IfModerator), None));
}

#[test]
fn a_dm_is_a_friend_dm_by_its_author() {
    assert_eq!(place(ChannelType::Dm, true), Place::FriendDm);
    assert_eq!(place(ChannelType::GroupDm, true), Place::FriendDm);
    assert_eq!(place(ChannelType::GroupDm, false), Place::NonFriendDm);
    assert_eq!(place(ChannelType::GuildText, true), Place::Guild);
    assert_eq!(place(ChannelType::PublicThread, false), Place::Guild);
}

// Module 323073: restricted channel or server; "too young" only for an explicit false; the
// agreement opens it.
#[test]
fn the_age_gate_follows_the_restriction_the_age_and_the_agreement() {
    assert_eq!(age_gate(false, false, Some(false), false), AgeGate::Open);
    assert_eq!(
        age_gate(true, false, Some(true), false),
        AgeGate::Ask { server: false }
    );
    assert_eq!(
        age_gate(true, false, None, false),
        AgeGate::Ask { server: false }
    );
    assert_eq!(
        age_gate(false, true, None, false),
        AgeGate::Ask { server: true }
    );
    assert_eq!(
        age_gate(true, true, Some(true), false),
        AgeGate::Ask { server: true }
    );
    assert_eq!(age_gate(true, false, Some(true), true), AgeGate::Open);
    assert_eq!(
        age_gate(true, false, Some(false), true),
        AgeGate::TooYoung { server: false }
    );
    assert_eq!(
        age_gate(false, true, Some(false), false),
        AgeGate::TooYoung { server: true }
    );
}
