use std::time::SystemTime;

use akari_core::media;
use akari_core::state::{self, EmbedMediaRef};
use akari_markdown::EmbedText;

use crate::formatted::FormattedText;
use crate::images::{EmbedSlot, ImageKey};
use crate::names::Names;
use crate::records::{MediaCover, system_time};

/// One embed as it shows, after Discord's merge, promotion and filters (D145).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct Embed {
    /// Its place among the message's embeds, which its images' keys carry.
    pub index: u8,
    pub kind: EmbedKind,
    /// The media alone, without a card.
    pub inline: bool,
    /// The bar's color; `None` (no color, or white) draws the border's.
    pub color: Option<u32>,
    /// The title's link.
    pub url: Option<String>,
    pub provider: Option<EmbedLink>,
    pub author: Option<EmbedAuthor>,
    pub title: Option<RichText>,
    /// Images, videos and GIFVs show none.
    pub description: Option<RichText>,
    /// Only a rich embed's.
    pub fields: Vec<EmbedFieldText>,
    /// The image, or two to four of a gallery.
    pub media: Vec<EmbedImage>,
    pub video: Option<EmbedVideo>,
    /// The small image on the card's right.
    pub thumbnail: Option<EmbedImage>,
    pub footer: Option<EmbedFooter>,
    /// TikTok, or a portrait YouTube video: its media fits 400 × 450, not 400 × 300.
    pub tall: bool,
    pub cover: Option<MediaCover>,
}

/// An embed's type, as far as how it shows depends on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, uniffi::Enum)]
pub enum EmbedKind {
    Rich,
    Image,
    Video,
    Gifv,
    Article,
    Link,
    Other,
}

/// A text and how it shows: markdown on a rich embed, the plain string on others.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct RichText {
    pub text: String,
    pub formatted: FormattedText,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedLink {
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedAuthor {
    pub name: String,
    pub url: Option<String>,
    pub icon: Option<ImageKey>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedFieldText {
    pub name: RichText,
    pub value: RichText,
    /// Up to three inline fields share a row; one that isn't has its own.
    pub inline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedImage {
    pub key: ImageKey,
    /// Pixels, both above 0.
    pub width: u32,
    pub height: u32,
    /// A thumbhash for `placeholder_image` while the image loads.
    pub placeholder: Option<String>,
    /// A GIF, or an animated WebP or AVIF: it plays by the motion policy.
    pub animated: bool,
}

/// A video: its poster, and whether Discord's media proxy has a copy that plays (a GIFV, an mp4
/// link) or it plays only on its own site (YouTube).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedVideo {
    pub poster: EmbedImage,
    /// The video's own size, which its box is fitted from.
    pub width: u32,
    pub height: u32,
    pub proxied: bool,
    /// `Account::original_url` gives the copy that plays.
    pub key: ImageKey,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct EmbedFooter {
    pub text: Option<String>,
    pub icon: Option<ImageKey>,
    /// The embed's time, which the footer shows after its text.
    pub timestamp: Option<SystemTime>,
}

const IS_ANIMATED: u64 = 1 << 5;

// Discord gates lists and headings in embeds by this date (module 384231's snowflake
// 1088216706570268682, 2023-03-22).
const LISTS_SINCE_MILLIS: i64 = 1_679_521_480_935;

/// The records of a message's shown embeds; their spoilers numbered after the content's.
pub(crate) fn embeds(
    message: &state::Message,
    shown: &[state::ShownEmbed],
    names: &Names<'_>,
    first_spoiler: u16,
) -> Vec<Embed> {
    let lists = message
        .edited_timestamp
        .unwrap_or(message.timestamp)
        .unix_millis()
        >= LISTS_SINCE_MILLIS;
    let mut spoilers = first_spoiler;
    shown
        .iter()
        .filter_map(|shown| {
            let embed = message.embeds.get(shown.index)?;
            let index = u8::try_from(shown.index).ok()?;
            Some(record(
                message,
                embed,
                shown,
                index,
                lists,
                names,
                &mut spoilers,
            ))
        })
        .collect()
}

fn record(
    message: &state::Message,
    embed: &state::Embed,
    shown: &state::ShownEmbed,
    index: u8,
    lists: bool,
    names: &Names<'_>,
    spoilers: &mut u16,
) -> Embed {
    let kind = EmbedKind::from(shown.kind);
    let rich = kind == EmbedKind::Rich;
    let mut text = |source: &str, rules: EmbedText| {
        let document = if rich {
            akari_markdown::parse_embed(source, rules)
        } else {
            akari_markdown::plain(source)
        };
        let formatted = FormattedText::numbered(&document, source, names, Some(message), *spoilers);
        *spoilers = spoilers.saturating_add(document.spoiler_count());
        RichText {
            text: source.to_owned(),
            formatted,
        }
    };
    let key = |slot| ImageKey::EmbedMedia {
        channel_id: message.channel_id,
        message_id: message.id,
        embed: index,
        slot,
    };
    let title = embed.title.as_deref().map(|title| {
        text(
            title,
            EmbedText::Title {
                links: embed.url.is_none(),
            },
        )
    });
    let description = embed
        .description
        .as_deref()
        .filter(|_| shown.description)
        .map(|description| text(description, EmbedText::Description { lists }));
    let fields = if shown.fields {
        embed
            .fields
            .iter()
            .map(|field| EmbedFieldText {
                name: text(&field.name, EmbedText::Title { links: true }),
                value: text(&field.value, EmbedText::FieldValue { lists }),
                inline: field.inline,
            })
            .collect()
    } else {
        Vec::new()
    };
    let image = |at: EmbedMediaRef| image(message, at);
    let media = if shown.gallery.is_empty() {
        shown.image.and_then(image).into_iter().collect()
    } else {
        shown.gallery.iter().copied().filter_map(image).collect()
    };
    let video = shown.video.and_then(|video| {
        let source = embed.video.as_ref()?;
        Some(EmbedVideo {
            poster: image(video.poster)?,
            width: source.width.unwrap_or(0),
            height: source.height.unwrap_or(0),
            proxied: video.proxied,
            key: key(EmbedSlot::Video),
        })
    });
    let icon = |url: Option<&str>, slot| url.filter(|url| !url.is_empty()).map(|_| key(slot));
    Embed {
        index,
        kind,
        inline: shown.inline,
        color: embed.color.filter(|color| *color != 0xFF_FF_FF),
        url: embed.url.as_deref().map(str::to_owned),
        provider: embed.provider.as_ref().and_then(|provider| {
            Some(EmbedLink {
                name: provider.name.as_deref()?.to_owned(),
                url: provider.url.as_deref().map(str::to_owned),
            })
        }),
        author: embed.author.as_ref().map(|author| EmbedAuthor {
            name: author.name.to_string(),
            url: author.url.as_deref().map(str::to_owned),
            icon: icon(author.proxy_icon_url.as_deref(), EmbedSlot::AuthorIcon),
        }),
        title,
        description,
        fields,
        media,
        video,
        thumbnail: shown.thumbnail.and_then(image),
        footer: footer(
            embed,
            icon(
                embed
                    .footer
                    .as_ref()
                    .and_then(|footer| footer.proxy_icon_url.as_deref()),
                EmbedSlot::FooterIcon,
            ),
        ),
        tall: shown.tall,
        cover: shown.cover.map(MediaCover::from),
    }
}

fn footer(embed: &state::Embed, icon: Option<ImageKey>) -> Option<EmbedFooter> {
    let text = embed
        .footer
        .as_ref()
        .map(|footer| footer.text.to_string())
        .filter(|text| !text.is_empty());
    let timestamp = embed.timestamp.map(system_time);
    (text.is_some() || timestamp.is_some()).then_some(EmbedFooter {
        text,
        icon,
        timestamp,
    })
}

fn image(message: &state::Message, at: EmbedMediaRef) -> Option<EmbedImage> {
    let embed = message.embeds.get(usize::from(at.embed))?;
    let (media, slot) = match at.slot {
        media::EmbedSlot::Image => (embed.image.as_ref()?, EmbedSlot::Image),
        media::EmbedSlot::Thumbnail => (embed.thumbnail.as_ref()?, EmbedSlot::Thumbnail),
        media::EmbedSlot::Video => (embed.video.as_ref()?, EmbedSlot::Video),
        _ => return None,
    };
    Some(EmbedImage {
        key: ImageKey::EmbedMedia {
            channel_id: message.channel_id,
            message_id: message.id,
            embed: at.embed,
            slot,
        },
        width: media.width?,
        height: media.height?,
        placeholder: media.placeholder.as_deref().map(str::to_owned),
        animated: media.flags & IS_ANIMATED != 0,
    })
}

impl From<state::EmbedKind> for EmbedKind {
    fn from(kind: state::EmbedKind) -> Self {
        match kind {
            state::EmbedKind::Rich => Self::Rich,
            state::EmbedKind::Image => Self::Image,
            state::EmbedKind::Video => Self::Video,
            state::EmbedKind::Gifv => Self::Gifv,
            state::EmbedKind::Article => Self::Article,
            state::EmbedKind::Link => Self::Link,
            _ => Self::Other,
        }
    }
}
