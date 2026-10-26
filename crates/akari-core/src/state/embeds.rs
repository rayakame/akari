//! How a message's embeds show (D145), as the official client decides (canary 649848: module
//! 659674 for the record and the merge, 161573 for the card, 91688 for the filters, 521981 for
//! the link text, 294520 for the cover).

use crate::media::EmbedSlot;

use super::media::{self, Covered, MediaCover, Scan};
use super::settings::DisplaySettings;
use super::types::{Embed, EmbedMedia, Message};

/// An embed's type, as far as how it shows depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum EmbedKind {
    Rich,
    Image,
    Video,
    Gifv,
    Article,
    Link,
    Other,
}

impl EmbedKind {
    fn of(kind: Option<&str>) -> Self {
        match kind {
            Some("rich") | None => Self::Rich,
            Some("image") => Self::Image,
            Some("video") => Self::Video,
            Some("gifv") => Self::Gifv,
            Some("article") => Self::Article,
            Some("link") => Self::Link,
            Some(_) => Self::Other,
        }
    }
}

/// One media object of a message's embeds: the embed's place and its slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EmbedMediaRef {
    pub embed: u8,
    pub slot: EmbedSlot,
}

/// A video in an embed: its poster, and whether Discord's media proxy has a copy that plays
/// (a GIFV, an mp4 link) or it plays only on its own site (YouTube).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShownVideo {
    pub poster: EmbedMediaRef,
    pub proxied: bool,
}

/// One embed as it shows, after Discord's merge, promotion and filters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShownEmbed {
    /// Its place in [`Message::embeds`]; merged embeds show as the first of them.
    pub index: usize,
    pub kind: EmbedKind,
    /// The media alone, without a card.
    pub inline: bool,
    pub image: Option<EmbedMediaRef>,
    /// Two to four images of embeds of one link, instead of `image`.
    pub gallery: Vec<EmbedMediaRef>,
    pub video: Option<ShownVideo>,
    /// The small image on the card's right.
    pub thumbnail: Option<EmbedMediaRef>,
    /// Fields show only on rich embeds.
    pub fields: bool,
    /// Images, videos and GIFVs show no description.
    pub description: bool,
    /// TikTok, or a portrait YouTube video: its media fits 400 × 450, not 400 × 300.
    pub tall: bool,
    pub cover: Option<MediaCover>,
}

// Drawn as lines of their own, or not at all (modules 581619 and 91688).
const NEVER: [&str; 10] = [
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
const CONTAINS_EXPLICIT: u64 = 1 << 4;
const CONTAINS_GORE: u64 = 1 << 6;
// Only a user's message shows embeds (module 143413's USER_MESSAGE).
const USER_MESSAGES: [u16; 4] = [0, 19, 20, 23];

/// Whether the message shows embeds at all: a user's message, with `render_embeds` on.
pub(crate) fn shows_embeds(message: &Message, display: &DisplaySettings) -> bool {
    display.render_embeds && USER_MESSAGES.contains(&u16::from(message.kind))
}

/// The embeds a message shows, in order; `cover` gives each one's cover.
pub(crate) fn shown_embeds(
    message: &Message,
    display: &DisplaySettings,
    cover: impl Fn(&Embed) -> Option<MediaCover>,
) -> Vec<ShownEmbed> {
    if !shows_embeds(message, display) {
        return Vec::new();
    }
    let mut shown: Vec<ShownEmbed> = Vec::new();
    let mut urls: Vec<(&str, usize)> = Vec::new();
    for (index, embed) in message.embeds.iter().enumerate() {
        let Ok(place) = u8::try_from(index) else {
            break;
        };
        if embed
            .kind
            .as_deref()
            .is_some_and(|kind| NEVER.contains(&kind))
        {
            continue;
        }
        let record = record(embed, place);
        if let Some(url) = embed.url.as_deref() {
            if let Some(&(_, first)) = urls.iter().find(|(known, _)| *known == url) {
                if let Some(image) = record.image {
                    let into = &mut shown[first];
                    if into.gallery.is_empty() {
                        into.gallery.extend(into.image);
                    }
                    into.gallery.push(image);
                }
                continue;
            }
            urls.push((url, shown.len()));
        }
        shown.push(ShownEmbed {
            index,
            cover: cover(embed),
            ..record
        });
    }
    for embed in &mut shown {
        finish(embed, &message.embeds[embed.index], display);
    }
    shown
}

// The client's record (module 659674 `R`): sized media only, the thumbnail promoted for
// articles and images, a video kept with a poster and a source that plays.
fn record(embed: &Embed, place: u8) -> ShownEmbed {
    let kind = EmbedKind::of(embed.kind.as_deref());
    let at = |slot| EmbedMediaRef { embed: place, slot };
    let mut image = None;
    let mut thumbnail = None;
    if embed.thumbnail.as_ref().is_some_and(sized) {
        if matches!(kind, EmbedKind::Article | EmbedKind::Image) {
            image = Some(at(EmbedSlot::Thumbnail));
        } else {
            thumbnail = Some(at(EmbedSlot::Thumbnail));
        }
    }
    if embed.image.as_ref().is_some_and(sized) {
        image = Some(at(EmbedSlot::Image));
    }
    let mut video = None;
    if let Some(source) = embed.video.as_ref().filter(|video| sized(video)) {
        if thumbnail.is_none() && source.proxy_url.is_some() {
            thumbnail = Some(at(EmbedSlot::Video));
        }
        let plays = source.proxy_url.is_some() || source.url.starts_with("https:");
        if let Some(poster) = thumbnail.filter(|_| plays) {
            video = Some(ShownVideo {
                poster,
                proxied: source.proxy_url.is_some(),
            });
        }
    }
    ShownEmbed {
        index: usize::from(place),
        kind,
        inline: false,
        image,
        gallery: Vec::new(),
        video,
        thumbnail,
        fields: kind == EmbedKind::Rich,
        description: !matches!(kind, EmbedKind::Image | EmbedKind::Video | EmbedKind::Gifv),
        tall: tall(embed),
        cover: None,
    }
}

// Module 659674 `ds` and `y`: the provider's name, or YouTube's host.
fn tall(embed: &Embed) -> bool {
    let provider = embed
        .provider
        .as_ref()
        .and_then(|provider| provider.name.as_deref());
    if provider == Some("TikTok") {
        return true;
    }
    let Some(video) = embed.video.as_ref() else {
        return false;
    };
    let portrait = video.height.unwrap_or(0) > video.width.unwrap_or(0);
    let youtube = provider == Some("YouTube")
        || reqwest::Url::parse(&video.url)
            .is_ok_and(|url| url.host_str() == Some("www.youtube.com"));
    portrait && youtube
}

fn sized(media: &EmbedMedia) -> bool {
    media.width.is_some_and(|width| width > 0) && media.height.is_some_and(|height| height > 0)
}

// After the merge: the inline form, the thumbnail's place, the media setting.
fn finish(shown: &mut ShownEmbed, embed: &Embed, display: &DisplaySettings) {
    if shown.gallery.len() == 1 {
        shown.image = shown.gallery.pop();
    } else if !shown.gallery.is_empty() {
        shown.image = None;
    }
    if !display.inline_embed_media {
        shown.image = None;
        shown.gallery.clear();
        shown.video = None;
        shown.thumbnail = None;
        return;
    }
    let media = shown.image.is_some() || shown.video.is_some();
    shown.inline = media
        && (shown.kind == EmbedKind::Gifv
            || (shown.kind != EmbedKind::Rich && embed.author.is_none() && embed.title.is_none()));
    // A rich card shows its thumbnail beside its image; others only without media.
    if shown.video.is_some()
        || (shown.kind != EmbedKind::Rich && (media || !shown.gallery.is_empty()))
    {
        shown.thumbnail = None;
    }
}

/// The cover over an embed: its harm flags where the user's settings cover them, a scan still
/// to come (people's embeds with media), a spoilered link in the message.
pub(crate) fn embed_cover(
    embed: &Embed,
    covered: Covered,
    scan: Scan,
    bot: bool,
    spoilered: bool,
) -> Option<MediaCover> {
    let flags = embed.flags;
    if (covered.explicit && flags & CONTAINS_EXPLICIT != 0)
        || (covered.gore && flags & CONTAINS_GORE != 0)
    {
        return Some(MediaCover::Sensitive);
    }
    if !bot && waits_for_scan(embed, covered, scan) {
        return Some(MediaCover::Pending);
    }
    spoilered.then_some(MediaCover::Spoiler)
}

pub(crate) fn waits_for_scan(embed: &Embed, covered: Covered, scan: Scan) -> bool {
    let media = [&embed.image, &embed.thumbnail, &embed.video]
        .into_iter()
        .any(|media| media.as_ref().is_some_and(sized));
    media
        && covered.any()
        && !scan.settled
        && media::unscanned(embed.content_scan_version, covered, scan.newest)
}

/// Discord hides the message's text when it's one link and the message shows one inline image
/// or GIFV for it (module 521981); `single_link` is the text's say.
pub(crate) fn hides_link(
    shown: &[ShownEmbed],
    display: &DisplaySettings,
    single_link: bool,
) -> bool {
    let [embed] = shown else {
        return false;
    };
    single_link
        && display.render_embeds
        && display.inline_embed_media
        && embed.inline
        && matches!(embed.kind, EmbedKind::Image | EmbedKind::Gifv)
}

#[cfg(test)]
mod tests;
