use super::*;
use crate::media::EmbedSlot;
use crate::model::Timestamp;
use crate::state::media::{Covered, Scan};
use crate::state::settings::DisplaySettings;
use crate::state::types::{EmbedAuthor, EmbedMedia};

fn media(width: u32, height: u32) -> EmbedMedia {
    EmbedMedia {
        url: "https://example.com/a.png".into(),
        proxy_url: Some(
            "https://images-ext-1.discordapp.net/external/x/https/example.com/a.png".into(),
        ),
        width: Some(width),
        height: Some(height),
        flags: 0,
        placeholder: None,
    }
}

fn embed(kind: &str) -> Embed {
    Embed {
        kind: Some(kind.into()),
        title: None,
        description: None,
        url: Some("https://example.com/".into()),
        timestamp: None,
        color: None,
        author: None,
        provider: None,
        footer: None,
        image: None,
        thumbnail: None,
        video: None,
        fields: Box::new([]),
        flags: 0,
        content_scan_version: Some(4),
    }
}

fn message(embeds: Vec<Embed>) -> Message {
    let mut message = crate::cache::tests::message(1, 1, "");
    message.embeds = embeds.into();
    message
}

fn shown(embeds: Vec<Embed>) -> Vec<ShownEmbed> {
    shown_embeds(&message(embeds), &DisplaySettings::default(), |_| None)
}

fn at(embed: u8, slot: EmbedSlot) -> EmbedMediaRef {
    EmbedMediaRef { embed, slot }
}

#[test]
fn a_rich_embed_is_a_card_with_its_fields_and_both_images() {
    let mut rich = embed("rich");
    rich.title = Some("Release".into());
    rich.image = Some(media(1200, 630));
    rich.thumbnail = Some(media(80, 80));

    let [card] = &shown(vec![rich])[..] else {
        panic!("one embed");
    };

    assert_eq!(card.kind, EmbedKind::Rich);
    assert!(!card.inline);
    assert!(card.fields && card.description);
    assert_eq!(card.image, Some(at(0, EmbedSlot::Image)));
    assert_eq!(
        card.thumbnail,
        Some(at(0, EmbedSlot::Thumbnail)),
        "a rich card shows both"
    );
}

// Module 659674: ARTICLE and IMAGE take their thumbnail as the image; others keep it on the
// right; an image overrides; media without a size is dropped.
#[test]
fn articles_and_images_promote_their_thumbnail() {
    let mut article = embed("article");
    article.title = Some("News".into());
    article.thumbnail = Some(media(1200, 630));
    let mut link = embed("link");
    link.url = Some("https://example.org/".into());
    link.title = Some("Page".into());
    link.thumbnail = Some(media(400, 400));
    let mut sizeless = embed("link");
    sizeless.url = Some("https://example.net/".into());
    sizeless.title = Some("No size".into());
    sizeless.thumbnail = Some(media(0, 400));

    let shown = shown(vec![article, link, sizeless]);

    assert_eq!(shown[0].image, Some(at(0, EmbedSlot::Thumbnail)));
    assert_eq!(shown[0].thumbnail, None);
    assert!(!shown[0].description || shown[0].kind == EmbedKind::Article);
    assert_eq!(shown[1].image, None);
    assert_eq!(shown[1].thumbnail, Some(at(1, EmbedSlot::Thumbnail)));
    assert_eq!(shown[2].thumbnail, None, "no size, no media");
    assert!(!shown[1].fields, "fields only for rich embeds");
}

// Module 659674 `NV`: media alone (no card) for a GIFV, or a non-rich embed without author
// and title.
#[test]
fn bare_media_shows_without_a_card() {
    let mut image = embed("image");
    image.thumbnail = Some(media(500, 500));
    let mut titled = embed("image");
    titled.url = Some("https://example.org/".into());
    titled.title = Some("A title".into());
    titled.thumbnail = Some(media(500, 500));
    let mut authored = embed("rich");
    authored.url = Some("https://example.net/".into());
    authored.image = Some(media(500, 500));

    let shown = shown(vec![image, titled, authored]);

    assert!(shown[0].inline);
    assert_eq!(shown[0].image, Some(at(0, EmbedSlot::Thumbnail)));
    assert!(!shown[1].inline, "a title makes a card");
    assert!(!shown[2].inline, "a rich embed is a card");
}

// Module 659674 `L`: later embeds of the same URL give their image to the first.
#[test]
fn embeds_of_one_link_merge_into_a_gallery() {
    let images: Vec<Embed> = (0..3)
        .map(|_| {
            let mut tweet = embed("rich");
            tweet.title = Some("Post".into());
            tweet.image = Some(media(1000, 1000));
            tweet
        })
        .collect();
    let mut other = embed("rich");
    other.url = Some("https://example.org/".into());
    other.image = Some(media(10, 10));

    let mut all = images;
    all.push(other);
    let shown = shown(all);

    assert_eq!(shown.len(), 2);
    assert_eq!(shown[0].index, 0);
    assert_eq!(
        shown[0].gallery,
        [
            at(0, EmbedSlot::Image),
            at(1, EmbedSlot::Image),
            at(2, EmbedSlot::Image)
        ]
    );
    assert_eq!(shown[0].image, None, "the gallery shows instead");
    assert_eq!(shown[1].index, 3);
}

// GIFV: the proxy's video over the thumbnail; without one, the video's own still.
#[test]
fn a_gifv_plays_over_its_poster() {
    let mut gifv = embed("gifv");
    gifv.thumbnail = Some(media(498, 280));
    gifv.video = Some(media(498, 280));
    let mut bare = embed("gifv");
    bare.url = Some("https://example.org/".into());
    bare.video = Some(media(498, 280));

    let shown = shown(vec![gifv, bare]);

    assert!(shown[0].inline);
    assert_eq!(
        shown[0].video,
        Some(ShownVideo {
            poster: at(0, EmbedSlot::Thumbnail),
            proxied: true
        })
    );
    assert_eq!(shown[0].thumbnail, None, "the poster isn't the 80 × 80 too");
    assert_eq!(
        shown[1].video,
        Some(ShownVideo {
            poster: at(1, EmbedSlot::Video),
            proxied: true
        })
    );
}

// YouTube: no proxy for the video, the thumbnail as the poster; a card.
#[test]
fn a_video_without_a_proxy_shows_its_thumbnail() {
    let mut youtube = embed("video");
    youtube.title = Some("A talk".into());
    youtube.thumbnail = Some(media(1280, 720));
    let mut video = media(1280, 720);
    video.url = "https://www.youtube.com/embed/abc".into();
    video.proxy_url = None;
    youtube.video = Some(video);
    let mut insecure = embed("video");
    insecure.url = Some("https://example.org/".into());
    insecure.title = Some("Plain http".into());
    insecure.thumbnail = Some(media(640, 360));
    let mut plain = media(640, 360);
    plain.url = "http://example.org/v.mp4".into();
    plain.proxy_url = None;
    insecure.video = Some(plain);

    let shown = shown(vec![youtube, insecure]);

    assert_eq!(
        shown[0].video,
        Some(ShownVideo {
            poster: at(0, EmbedSlot::Thumbnail),
            proxied: false
        })
    );
    assert_eq!(shown[0].thumbnail, None);
    assert!(!shown[0].description, "a video embed shows no description");
    assert_eq!(
        shown[1].video, None,
        "Discord keeps only proxied or https videos"
    );
    assert_eq!(
        shown[1].thumbnail,
        Some(at(1, EmbedSlot::Thumbnail)),
        "then its thumbnail"
    );
}

// No thumbnail and no proxy: nothing to show for the video (module 659674).
#[test]
fn a_video_without_a_poster_or_a_proxy_shows_nothing() {
    let mut bare = embed("video");
    bare.title = Some("Clip".into());
    let mut video = media(640, 360);
    video.url = "https://example.com/v".into();
    video.proxy_url = None;
    bare.video = Some(video);

    let shown = shown(vec![bare]);

    assert_eq!(shown[0].video, None);
    assert_eq!(shown[0].thumbnail, None);
}

// A link whose first embed has no image and a later one has: one image, not a gallery.
#[test]
fn one_merged_image_is_an_image() {
    let mut first = embed("rich");
    first.title = Some("Post".into());
    let mut second = embed("rich");
    second.image = Some(media(800, 600));

    let shown = shown(vec![first, second]);

    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].image, Some(at(1, EmbedSlot::Image)));
    assert!(shown[0].gallery.is_empty());
}

#[test]
fn system_and_gift_embeds_are_never_drawn() {
    let kinds = [
        "auto_moderation_message",
        "auto_moderation_notification",
        "age_verification_system_notification",
        "application_news",
        "post_preview",
        "safety_policy_notice",
        "safety_system_notification",
        "voice_channel",
        "poll_result",
        "gift",
    ];
    let embeds = kinds
        .iter()
        .enumerate()
        .map(|(index, kind)| {
            let mut embed = embed(kind);
            embed.url = Some(format!("https://example.com/{index}").into());
            embed.title = Some("T".into());
            embed
        })
        .collect();

    assert!(shown(embeds).is_empty());
    let mut notice = message(vec![embed("rich")]);
    notice.kind = crate::model::MessageType::from(46);
    assert!(
        shown_embeds(&notice, &DisplaySettings::default(), |_| None).is_empty(),
        "a poll result line"
    );
}

#[test]
fn the_settings_hide_embeds_or_their_media() {
    let mut rich = embed("rich");
    rich.title = Some("T".into());
    rich.image = Some(media(500, 500));
    rich.thumbnail = Some(media(80, 80));
    let mut image = embed("image");
    image.url = Some("https://example.org/".into());
    image.thumbnail = Some(media(500, 500));
    let message = message(vec![rich, image]);
    let with = |edit: fn(&mut DisplaySettings)| {
        let mut display = DisplaySettings::default();
        edit(&mut display);
        shown_embeds(&message, &display, |_| None)
    };

    assert!(with(|display| display.render_embeds = false).is_empty());
    let without_media = with(|display| display.inline_embed_media = false);
    assert_eq!(without_media.len(), 2);
    assert!(without_media.iter().all(|shown| {
        !shown.inline
            && shown.image.is_none()
            && shown.thumbnail.is_none()
            && shown.gallery.is_empty()
    }));
}

// Module 294520: a harm flag wins over a pending scan (non-bot authors with media only), which
// wins over a spoiler.
#[test]
fn an_embeds_cover_follows_its_flags_scan_and_the_spoilers() {
    let covered = Covered {
        explicit: true,
        gore: false,
    };
    let waiting = Scan {
        settled: false,
        newest: Some(4),
    };
    let mut flagged = embed("image");
    flagged.thumbnail = Some(media(500, 500));
    flagged.flags = 16;
    let mut unscanned = embed("image");
    unscanned.thumbnail = Some(media(500, 500));
    unscanned.content_scan_version = None;
    let mut textual = embed("link");
    textual.title = Some("No media".into());
    textual.content_scan_version = None;

    assert_eq!(
        embed_cover(&flagged, covered, waiting, false, true),
        Some(MediaCover::Sensitive)
    );
    assert_eq!(
        embed_cover(&unscanned, covered, waiting, false, true),
        Some(MediaCover::Pending)
    );
    assert_eq!(
        embed_cover(&unscanned, covered, waiting, true, false),
        None,
        "a bot's"
    );
    assert_eq!(
        embed_cover(&textual, covered, waiting, false, false),
        None,
        "nothing to scan"
    );
    assert_eq!(
        embed_cover(&unscanned, Covered::default(), waiting, false, true),
        Some(MediaCover::Spoiler)
    );
    let settled = Scan {
        settled: true,
        newest: Some(4),
    };
    assert_eq!(
        embed_cover(&unscanned, covered, settled, false, false),
        None
    );
}

// Module 521981: one inline image or GIFV embed and content that is one link.
#[test]
fn the_link_hides_under_its_one_inline_image() {
    let mut image = embed("image");
    image.thumbnail = Some(media(500, 500));
    let mut rich = embed("rich");
    rich.title = Some("T".into());
    rich.image = Some(media(500, 500));
    let display = DisplaySettings::default();

    assert!(hides_link(&shown(vec![image.clone()]), &display, true));
    assert!(
        !hides_link(&shown(vec![image.clone()]), &display, false),
        "more than a link"
    );
    assert!(!hides_link(&shown(vec![rich]), &display, true), "a card");
    let mut second = image.clone();
    second.url = Some("https://example.org/".into());
    assert!(
        !hides_link(&shown(vec![image.clone(), second]), &display, true),
        "two embeds"
    );
    let off = DisplaySettings {
        inline_embed_media: false,
        ..DisplaySettings::default()
    };
    assert!(!hides_link(&shown(vec![image]), &off, true));
}

#[test]
fn an_author_makes_an_image_a_card() {
    let mut image = embed("image");
    image.thumbnail = Some(media(500, 500));
    image.author = Some(EmbedAuthor {
        name: "Someone".into(),
        url: None,
        proxy_icon_url: None,
    });

    assert!(!shown(vec![image])[0].inline);
}

// Module 659674 `ds`: TikTok, and a portrait video from YouTube, fit their media in 400 × 450.
#[test]
fn tiktok_and_portrait_youtube_are_tall() {
    let provider = |name: &str| {
        Some(crate::state::types::EmbedProvider {
            name: Some(name.into()),
            url: None,
        })
    };
    let video = |width, height, provider| {
        let mut video = embed("video");
        video.provider = provider;
        video.thumbnail = Some(media(width, height));
        video.video = Some(media(width, height));
        video
    };
    let mut from_host = video(1080, 1920, None);
    if let Some(source) = from_host.video.as_mut() {
        source.url = "https://www.youtube.com/embed/x".into();
    }

    let tall = |embed| shown(vec![embed])[0].tall;
    assert!(tall(video(1080, 1920, provider("TikTok"))));
    assert!(
        tall(video(1920, 1080, provider("TikTok"))),
        "TikTok whatever its shape"
    );
    assert!(tall(video(1080, 1920, provider("YouTube"))));
    assert!(tall(from_host));
    assert!(
        !tall(video(1920, 1080, provider("YouTube"))),
        "a landscape one"
    );
    assert!(!tall(video(1080, 1920, provider("Vimeo"))));
}

// Review: what the host needs to show a card's text and bar, as the client decides it (91688,
// 161573): markdown on rich embeds only, lists by the message's time (its edit's first), title
// links when the embed has no URL (an empty one counts as none), white as no color, a footer with
// text or a time.
#[test]
fn the_shown_embed_says_how_its_text_and_bar_show() {
    let mut rich = embed("rich");
    rich.url = Some("".into());
    rich.color = Some(0xFF_FF_FF);
    rich.footer = Some(crate::state::types::EmbedFooter {
        text: "".into(),
        proxy_icon_url: None,
    });
    let mut article = embed("article");
    article.color = Some(0x12_34_56);
    article.timestamp = Some(Timestamp::from_unix_millis(1_700_000_000_000));
    let mut message = message(vec![rich, article]);
    message.timestamp = Timestamp::from_unix_millis(1_600_000_000_000);
    let shown = shown_embeds(&message, &DisplaySettings::default(), |_| None);

    assert!(shown[0].markdown && !shown[1].markdown);
    assert!(!shown[0].lists, "2020");
    assert!(shown[0].title_links, "no URL");
    assert!(!shown[1].title_links);
    assert_eq!((shown[0].color, shown[1].color), (None, Some(0x12_34_56)));
    assert!(!shown[0].footer, "empty text, no time");
    assert!(shown[1].footer, "a time");

    message.edited_timestamp = Some(Timestamp::from_unix_millis(1_700_000_000_000));
    let edited = shown_embeds(&message, &DisplaySettings::default(), |_| None);
    assert!(edited[0].lists, "the edit's time");
}

// Review: a merged gallery of bare images shows alone and hides its link (659674 tests the
// record's first image); at most four images are kept.
#[test]
fn a_bare_gallery_shows_alone_with_at_most_four_images() {
    let image = |n: u32| {
        let mut image = embed("image");
        image.thumbnail = Some(media(600 + n, 400));
        image
    };
    let shown = shown((0..6).map(image).collect());

    let [gallery] = &shown[..] else {
        panic!("merged: {shown:?}");
    };
    assert!(gallery.inline);
    assert_eq!(gallery.gallery.len(), 4);
    assert!(hides_link(&shown, &DisplaySettings::default(), true));
}

// A GIFV alone hides its link too (521981's h5: IMAGE and GIFV).
#[test]
fn a_gifv_hides_its_link() {
    let mut gifv = embed("gifv");
    gifv.thumbnail = Some(media(498, 280));
    gifv.video = Some(media(498, 280));
    let shown = shown(vec![gifv]);

    assert!(shown[0].inline);
    assert!(hides_link(&shown, &DisplaySettings::default(), true));
}

// Module 659674 `p6, V, G8, Xk`: articles for Discord's own shop and game pages aren't drawn.
#[test]
fn discords_shop_and_game_articles_are_left_out() {
    let article = |url: &str| {
        let mut article = embed("article");
        article.url = Some(url.into());
        article.title = Some("Shop".into());
        article
    };
    for url in [
        "https://discord.com/channels/123/shop",
        "https://discord.com/channels/123/shop/456",
        "https://discord.com/shop",
        "https://canary.discord.com/shop#itemSkuId=1",
        "https://discord.com/games/789",
    ] {
        assert!(shown(vec![article(url)]).is_empty(), "{url}");
    }
    for url in [
        "https://example.com/shop",
        "https://discord.com/channels/123/456",
    ] {
        assert_eq!(shown(vec![article(url)]).len(), 1, "{url}");
    }
}

// Module 619517 `isAnimated`: a GIF, or a WebP or AVIF Discord flags animated.
#[test]
fn embed_media_animates_as_discords_client_says() {
    let at = |url: &str, flags: u64| EmbedMedia {
        url: url.into(),
        proxy_url: None,
        flags,
        ..media(10, 10)
    };
    assert!(at("https://media.tenor.com/a.gif", 0).is_animated());
    assert!(at("https://example.com/a.GIF?x=1", 0).is_animated());
    assert!(!at("https://example.com/a.webp", 0).is_animated());
    assert!(at("https://example.com/a.webp", 32).is_animated());
    assert!(at("https://example.com/a.avif", 32).is_animated());
    assert!(!at("https://example.com/a.png", 32).is_animated());
}
