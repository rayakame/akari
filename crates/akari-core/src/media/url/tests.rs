use super::*;
use crate::model::Snowflake;

fn hosts() -> Hosts {
    Hosts::new(
        Url::parse("https://cdn.discordapp.com/").unwrap(),
        Url::parse("https://media.discordapp.net/").unwrap(),
    )
}

const HASH: &str = "0123456789abcdef0123456789abcdef";
const ANIMATED: &str = "a_0123456789abcdef0123456789abcdef";

fn avatar(hash: &str) -> ImageKey {
    ImageKey::Avatar {
        user: Snowflake::new(80_351_110_224_678_912),
        hash: ImageHash::parse(hash),
    }
}

fn url_of(planned: Result<Planned, ImageError>) -> String {
    planned.unwrap().url.to_string()
}

const SIGNATURE: &str =
    "ex=67a1b2c3&is=67a06143&hm=0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0&";

fn attachment(filename: &str, width: u32, height: u32) -> Attachment {
    Attachment {
        id: Snowflake::new(1_300_000_000_000_000_003),
        filename: filename.into(),
        title: None,
        description: None,
        content_type: None,
        size: 1000,
        url: format!(
            "https://cdn.discordapp.com/attachments/1300000000000000001/1300000000000000003/{filename}?{SIGNATURE}"
        )
        .into(),
        proxy_url: format!(
            "https://media.discordapp.net/attachments/1300000000000000001/1300000000000000003/{filename}?{SIGNATURE}"
        )
        .into(),
        width: Some(width),
        height: Some(height),
        flags: 0,
        placeholder: None,
        content_scan_version: None,
    }
}

fn query(url: &str) -> String {
    Url::parse(url)
        .unwrap()
        .query_pairs()
        .filter(|(name, _)| !["ex", "is", "hm"].contains(&name.as_ref()))
        .map(|(name, value)| format!("{name}={value}"))
        .collect::<Vec<_>>()
        .join("&")
}

#[test]
fn the_ladder_rounds_up_to_what_the_cdn_serves() {
    for (pixels, size) in [
        (0, 16),
        (1, 16),
        (16, 16),
        (17, 20),
        (32, 32),
        (33, 40),
        (44, 44),
        (45, 48),
        (64, 64),
        (80, 80),
        (81, 96),
        (96, 96),
        (120, 128),
        (161, 240),
        (320, 320),
        (321, 480),
        (4096, 4096),
        (4097, 4096),
        (u32::MAX, 4096),
    ] {
        assert_eq!(ladder_size(pixels), size, "{pixels} px");
    }
}

// D130: the box's points × the backing scale, rounded up, then up the ladder; never below the box.
#[test]
fn each_place_asks_at_least_its_pixels_at_each_scale() {
    for (place, points, [one, two, three]) in [
        ("message avatar", 40, [40, 80, 128]),
        ("DM row", 32, [32, 64, 96]),
        ("user panel", 32, [32, 64, 96]),
        ("emoji", 22, [22, 44, 80]),
        ("reply", 16, [16, 32, 48]),
        ("jumbo emoji", 48, [48, 96, 160]),
        ("server rail", 40, [40, 80, 128]),
        ("thread box", 16, [16, 32, 48]),
        ("facepile", 24, [24, 48, 80]),
        ("sticker", 160, [160, 320, 480]),
    ] {
        for (scale, asked) in [(1, one), (2, two), (3, three)] {
            let pixels = points * scale;
            let size = ladder_size(pixels);
            assert_eq!(size, asked, "{place} at {scale}x");
            assert!(size >= pixels, "{place} at {scale}x");
        }
    }
}

#[test]
fn avatars_are_webp_at_the_ladder_size() {
    let planned = hosts().identity(&avatar(HASH), 80, None).unwrap();

    assert_eq!(
        planned.url.as_str(),
        "https://cdn.discordapp.com/avatars/80351110224678912/0123456789abcdef0123456789abcdef.webp?size=80"
    );
    assert_eq!(planned.form, Form::Still);
    assert_eq!(planned.needed, 80);
    assert!(!planned.full);
    assert!(planned.signed.is_none());
}

#[test]
fn only_an_animated_hash_asked_animated_animates() {
    let hosts = hosts();

    assert_eq!(
        url_of(hosts.identity(&avatar(ANIMATED), 80, Some(AnimatedFormat::WebP))),
        format!(
            "https://cdn.discordapp.com/avatars/80351110224678912/{ANIMATED}.webp?size=80&animated=true"
        )
    );
    assert_eq!(
        url_of(hosts.identity(&avatar(ANIMATED), 80, Some(AnimatedFormat::Gif))),
        format!("https://cdn.discordapp.com/avatars/80351110224678912/{ANIMATED}.gif?size=80")
    );
    assert_eq!(
        url_of(hosts.identity(&avatar(ANIMATED), 80, None)),
        format!("https://cdn.discordapp.com/avatars/80351110224678912/{ANIMATED}.webp?size=80")
    );
    let still = hosts
        .identity(&avatar(HASH), 80, Some(AnimatedFormat::WebP))
        .unwrap();
    assert_eq!(still.form, Form::Still);
    assert!(!still.url.as_str().contains("animated"));
    assert_eq!(
        hosts
            .identity(&avatar(ANIMATED), 80, Some(AnimatedFormat::Gif))
            .unwrap()
            .form,
        Form::Gif
    );
}

#[test]
fn every_identity_image_has_its_path() {
    let hosts = hosts();
    let guild = Snowflake::new(41_771_983_423_143_937);
    let user = Snowflake::new(80_351_110_224_678_912);
    let hash = ImageHash::parse(HASH);
    for (key, url) in [
        (
            ImageKey::MemberAvatar {
                guild,
                user,
                hash: hash.clone(),
            },
            format!(
                "https://cdn.discordapp.com/guilds/41771983423143937/users/80351110224678912/avatars/{HASH}.webp?size=80"
            ),
        ),
        (
            ImageKey::GuildIcon {
                guild,
                hash: hash.clone(),
            },
            format!(
                "https://cdn.discordapp.com/icons/41771983423143937/{HASH}.webp?size=80&quality=lossless"
            ),
        ),
        (
            ImageKey::RoleIcon {
                role: Snowflake::new(41_771_983_423_143_938),
                hash: hash.clone(),
            },
            format!(
                "https://cdn.discordapp.com/role-icons/41771983423143938/{HASH}.webp?size=80&quality=lossless"
            ),
        ),
        (
            ImageKey::GroupIcon {
                channel: Snowflake::new(41_771_983_423_143_939),
                hash: hash.clone(),
            },
            format!(
                "https://cdn.discordapp.com/channel-icons/41771983423143939/{HASH}.png?size=80"
            ),
        ),
        (
            ImageKey::Emoji {
                id: Snowflake::new(392_938_283_556_143_104),
                animated: false,
            },
            "https://cdn.discordapp.com/emojis/392938283556143104.webp?size=80".to_owned(),
        ),
    ] {
        assert_eq!(url_of(hosts.identity(&key, 80, None)), url, "{key:?}");
    }
}

#[test]
fn animated_server_icons_keep_lossless_before_animated() {
    let key = ImageKey::GuildIcon {
        guild: Snowflake::new(41_771_983_423_143_937),
        hash: ImageHash::parse(ANIMATED),
    };

    assert_eq!(
        url_of(hosts().identity(&key, 80, Some(AnimatedFormat::WebP))),
        format!(
            "https://cdn.discordapp.com/icons/41771983423143937/{ANIMATED}.webp?size=80&quality=lossless&animated=true"
        )
    );
}

#[test]
fn group_icons_never_animate() {
    let key = ImageKey::GroupIcon {
        channel: Snowflake::new(41_771_983_423_143_939),
        hash: ImageHash::parse(ANIMATED),
    };

    let planned = hosts()
        .identity(&key, 64, Some(AnimatedFormat::WebP))
        .unwrap();

    assert_eq!(planned.form, Form::Still);
    assert!(planned.url.as_str().ends_with(".png?size=64"));
}

#[test]
fn animated_emoji_animate_when_asked() {
    let key = ImageKey::Emoji {
        id: Snowflake::new(392_938_283_556_143_104),
        animated: true,
    };
    let hosts = hosts();

    assert_eq!(
        url_of(hosts.identity(&key, 44, Some(AnimatedFormat::WebP))),
        "https://cdn.discordapp.com/emojis/392938283556143104.webp?size=44&animated=true"
    );
    assert_eq!(
        url_of(hosts.identity(&key, 44, Some(AnimatedFormat::Gif))),
        "https://cdn.discordapp.com/emojis/392938283556143104.gif?size=44"
    );
    assert_eq!(
        url_of(hosts.identity(&key, 44, None)),
        "https://cdn.discordapp.com/emojis/392938283556143104.webp?size=44"
    );
}

#[test]
fn default_avatars_are_the_cdns_pngs_without_a_query() {
    let hosts = hosts();
    for index in 0..=5 {
        let planned = hosts
            .identity(&ImageKey::DefaultAvatar { index }, 512, None)
            .unwrap();
        assert_eq!(
            planned.url.as_str(),
            format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
        );
        assert!(
            planned.full,
            "the CDN ignores the size: the file is the image"
        );
        assert_eq!(planned.form, Form::Still);
    }
    for index in [6, 7, 255] {
        assert_eq!(
            hosts
                .identity(&ImageKey::DefaultAvatar { index }, 80, None)
                .err(),
            Some(ImageError::UnknownKey),
            "{index}"
        );
    }
}

#[test]
fn stickers_take_their_formats_forms_on_the_media_host() {
    let hosts = hosts();
    let sticker = |format| ImageKey::Sticker {
        id: Snowflake::new(749_054_660_769_218_631),
        format,
    };
    let base = "https://media.discordapp.net/stickers/749054660769218631";
    for (format, animated, url, form) in [
        (
            StickerFormatType::Png,
            Some(AnimatedFormat::WebP),
            format!("{base}.webp?size=320&quality=lossless"),
            Form::Still,
        ),
        (
            StickerFormatType::Apng,
            Some(AnimatedFormat::WebP),
            format!("{base}.png?size=320"),
            Form::Apng,
        ),
        (
            StickerFormatType::Apng,
            None,
            format!("{base}.png?size=320&passthrough=false"),
            Form::Still,
        ),
        (
            StickerFormatType::Gif,
            Some(AnimatedFormat::WebP),
            format!("{base}.gif?size=320"),
            Form::Gif,
        ),
        (
            StickerFormatType::Gif,
            None,
            format!("{base}.webp?size=320"),
            Form::Still,
        ),
    ] {
        let planned = hosts.identity(&sticker(format), 320, animated).unwrap();
        assert_eq!(planned.url.as_str(), url, "{format:?} {animated:?}");
        assert_eq!(planned.form, form, "{format:?} {animated:?}");
    }
    for format in [StickerFormatType::Lottie, StickerFormatType::Unknown(9)] {
        assert_eq!(
            hosts.identity(&sticker(format), 320, None).err(),
            Some(ImageError::Unsupported)
        );
    }
}

#[test]
fn identity_disk_keys_name_the_image_not_its_size() {
    let hosts = hosts();

    let small = hosts.identity(&avatar(HASH), 40, None).unwrap();
    let large = hosts.identity(&avatar(HASH), 160, None).unwrap();
    let animated = hosts
        .identity(&avatar(ANIMATED), 80, Some(AnimatedFormat::WebP))
        .unwrap();

    assert_eq!(small.disk, large.disk);
    assert_eq!(small.disk, format!("avatar/80351110224678912/{HASH}"));
    assert_eq!(
        animated.disk,
        format!("avatar/80351110224678912/{ANIMATED}")
    );
    assert_ne!(
        hosts
            .identity(&ImageKey::DefaultAvatar { index: 1 }, 80, None)
            .unwrap()
            .disk,
        hosts
            .identity(&ImageKey::DefaultAvatar { index: 2 }, 80, None)
            .unwrap()
            .disk
    );
}

// The box covered, the longest side up the ladder from 128, the other side by the aspect.
#[test]
fn proxy_sizes_cover_the_box_and_snap_up() {
    for (source, wanted, size) in [
        // 400 × 300 points at 2×: the 1920 × 1080 source asked 1024 wide.
        ((1920, 1080), (800, 450), Some((1024, 576))),
        ((1920, 1080), (550, 309), Some((600, 338))),
        // A square cover cell of a landscape source needs its height.
        ((1920, 1080), (546, 546), Some((1024, 576))),
        ((1080, 1920), (394, 700), Some((576, 1024))),
        // Small boxes still ask at least 128 on the longest side.
        ((1920, 1080), (40, 20), Some((128, 72))),
        // Never above the source: the source as it is.
        ((200, 100), (400, 200), None),
        ((1920, 1080), (1900, 1070), None),
        ((640, 480), (640, 480), None),
        // The snapped size reaching the source's is the source.
        ((600, 300), (560, 280), None),
        ((600, 300), (500, 250), Some((512, 256))),
        // The proxy serves at most 4096 on the longest side.
        ((8000, 6000), (8000, 6000), Some((4096, 3072))),
        ((8000, 6000), (5000, 3750), Some((4096, 3072))),
    ] {
        assert_eq!(
            proxy_size(source, wanted),
            size,
            "{source:?} for {wanted:?}"
        );
        if let Some((width, height)) = size {
            assert!(
                width >= wanted.0.min(4096) && height >= wanted.1.min(3072),
                "{source:?} for {wanted:?}"
            );
        }
    }
    assert_eq!(proxy_size((0, 100), (40, 40)), None);
}

#[test]
fn attachments_ask_the_proxy_at_their_size_with_the_signature() {
    let planned = hosts()
        .attachment(&attachment("photo.jpg", 1920, 1080), (800, 450), None)
        .unwrap();

    let url = planned.url.as_str();
    assert!(
        url.starts_with(
            "https://media.discordapp.net/attachments/1300000000000000001/1300000000000000003/photo.jpg?ex=67a1b2c3&is=67a06143&hm="
        ),
        "{url}"
    );
    assert_eq!(query(url), "format=webp&width=1024&height=576");
    assert_eq!(planned.needed, 1024);
    assert!(!planned.full);
    assert_eq!(planned.disk, "attachment/1300000000000000003");
    assert_eq!(
        planned.signed.as_ref().map(Url::host_str),
        Some(Some("cdn.discordapp.com"))
    );
}

#[test]
fn attachments_take_discords_formats_by_extension() {
    let hosts = hosts();
    let mut animated_webp = attachment("loop.webp", 320, 240);
    animated_webp.flags = 1 << 5;
    for (attachment, animated, expected, form) in [
        (
            attachment("shot.png", 320, 240),
            None,
            "format=webp&quality=lossless",
            Form::Still,
        ),
        (
            attachment("pic.avif", 320, 240),
            None,
            "format=webp&quality=lossless",
            Form::Still,
        ),
        (
            attachment("cat.gif", 320, 240),
            Some(AnimatedFormat::WebP),
            "",
            Form::Gif,
        ),
        (
            attachment("cat.gif", 320, 240),
            Some(AnimatedFormat::Gif),
            "",
            Form::Gif,
        ),
        (
            attachment("cat.gif", 320, 240),
            None,
            "format=webp",
            Form::Still,
        ),
        (
            animated_webp.clone(),
            Some(AnimatedFormat::Gif),
            "format=webp&animated=true",
            Form::WebP,
        ),
        (animated_webp, None, "format=webp", Form::Still),
        (
            attachment("still.webp", 320, 240),
            Some(AnimatedFormat::WebP),
            "format=webp",
            Form::Still,
        ),
        (
            attachment("clip.MP4", 320, 240),
            Some(AnimatedFormat::WebP),
            "format=webp",
            Form::Still,
        ),
    ] {
        let planned = hosts.attachment(&attachment, (320, 240), animated).unwrap();
        assert_eq!(
            query(planned.url.as_str()),
            expected,
            "{} {animated:?}",
            attachment.filename
        );
        assert_eq!(planned.form, form, "{} {animated:?}", attachment.filename);
        assert!(planned.full);
    }
}

#[test]
fn attachments_without_a_size_or_a_media_kind_are_not_images() {
    let hosts = hosts();
    let mut no_size = attachment("photo.jpg", 0, 0);
    no_size.width = None;
    no_size.height = None;
    for attachment in [
        no_size,
        attachment("notes.txt", 100, 100),
        attachment("drawing.svg", 100, 100),
    ] {
        assert_eq!(
            hosts.attachment(&attachment, (80, 80), None).err(),
            Some(ImageError::NotAnImage),
            "{}",
            attachment.filename
        );
    }
}

#[test]
fn an_attachment_off_discords_hosts_is_unsupported() {
    let mut elsewhere = attachment("photo.jpg", 1920, 1080);
    elsewhere.proxy_url = "https://example.com/photo.jpg".into();

    assert_eq!(
        hosts().attachment(&elsewhere, (80, 80), None).err(),
        Some(ImageError::Unsupported)
    );
}

#[test]
fn sizing_replaces_the_proxys_own_size_params() {
    let mut sized = attachment("photo.jpg", 1920, 1080);
    sized.proxy_url = format!(
        "https://media.discordapp.net/attachments/1/3/photo.jpg?{SIGNATURE}width=100&height=50&format=png"
    )
    .into();

    let planned = hosts().attachment(&sized, (800, 450), None).unwrap();

    assert_eq!(
        query(planned.url.as_str()),
        "format=webp&width=1024&height=576"
    );
}

#[test]
fn embed_images_size_like_attachments_and_key_by_their_path() {
    let media = EmbedMedia {
        url: "https://example.com/a.png".into(),
        proxy_url: Some(
            "https://images-ext-1.discordapp.net/external/AbCdEf/https/example.com/a.png".into(),
        ),
        width: Some(1200),
        height: Some(630),
        flags: 0,
        placeholder: None,
    };
    let hosts = hosts();

    let planned = hosts.embed(&media, (800, 420), None).unwrap();
    let moved = hosts
        .embed(
            &EmbedMedia {
                width: Some(1200),
                height: Some(630),
                ..media.clone()
            },
            (160, 84),
            None,
        )
        .unwrap();

    assert_eq!(
        planned.url.as_str(),
        "https://images-ext-1.discordapp.net/external/AbCdEf/https/example.com/a.png?format=webp&quality=lossless&width=1024&height=538"
    );
    assert!(planned.signed.is_none());
    assert!(planned.disk.starts_with("embed/"));
    assert_eq!(planned.disk, moved.disk);
    let other = hosts
        .embed(
            &EmbedMedia {
                proxy_url: Some(
                    "https://images-ext-2.discordapp.net/external/XyZ/https/example.com/b.png"
                        .into(),
                ),
                ..media
            },
            (800, 420),
            None,
        )
        .unwrap();
    assert_ne!(planned.disk, other.disk);
}

#[test]
fn embed_icons_are_fetched_as_discord_gives_them() {
    let planned = hosts()
        .embed_icon("https://images-ext-2.discordapp.net/external/QwE/https/example.com/icon.png")
        .unwrap();

    assert_eq!(
        planned.url.as_str(),
        "https://images-ext-2.discordapp.net/external/QwE/https/example.com/icon.png"
    );
    assert!(planned.full);
    assert_eq!(
        hosts().embed_icon("https://example.com/icon.png").err(),
        Some(ImageError::Unsupported)
    );
}

#[test]
fn only_discords_hosts_are_allowed() {
    let hosts = hosts();
    for (url, allowed) in [
        ("https://cdn.discordapp.com/avatars/1/2.webp", true),
        ("https://media.discordapp.net/attachments/1/2/a.png", true),
        (
            "https://images-ext-1.discordapp.net/external/x/https/a/b.png",
            true,
        ),
        (
            "https://images-ext-2.discordapp.net/external/x/https/a/b.png",
            true,
        ),
        ("http://cdn.discordapp.com/avatars/1/2.webp", false),
        ("https://cdn.discordapp.com:8443/avatars/1/2.webp", false),
        ("https://cdn.discordapp.com.example.com/a.png", false),
        ("https://discord.com/api/v9/users/@me", false),
        ("https://example.com/a.png", false),
    ] {
        assert_eq!(hosts.allows(&Url::parse(url).unwrap()), allowed, "{url}");
    }
}

#[test]
fn originals_are_png_or_gif_at_the_largest_size() {
    let hosts = hosts();

    assert_eq!(
        hosts.original(&avatar(HASH)).unwrap().as_str(),
        format!("https://cdn.discordapp.com/avatars/80351110224678912/{HASH}.png?size=4096")
    );
    assert_eq!(
        hosts.original(&avatar(ANIMATED)).unwrap().as_str(),
        format!("https://cdn.discordapp.com/avatars/80351110224678912/{ANIMATED}.gif?size=4096")
    );
    assert_eq!(
        hosts
            .original(&ImageKey::Emoji {
                id: Snowflake::new(392_938_283_556_143_104),
                animated: true
            })
            .unwrap()
            .as_str(),
        "https://cdn.discordapp.com/emojis/392938283556143104.gif"
    );
    assert_eq!(
        hosts
            .original(&ImageKey::DefaultAvatar { index: 2 })
            .unwrap()
            .as_str(),
        "https://cdn.discordapp.com/embed/avatars/2.png"
    );
    assert_eq!(
        hosts
            .original(&ImageKey::Sticker {
                id: Snowflake::new(749_054_660_769_218_631),
                format: StickerFormatType::Lottie
            })
            .err(),
        Some(ImageError::Unsupported)
    );
}
