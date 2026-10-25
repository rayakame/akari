use akari_core::model::{ChannelId, ChannelType, EmojiId, GuildId, MessageId, RoleId, UserId};
use akari_core::state;
use akari_markdown::utf16::Utf16Map;
use akari_markdown::{
    BlockKind, Container, Document, HeadingLevel, InlineKind, Link, LinkKind, Navigation, Span,
    Style,
};

use crate::images::ImageKey;

/// A Unicode emoji's name as Discord's hover card and popout show it, without colons
/// (`grinning`, `thumbsup_tone3`, `flag_de`); `None` for anything that isn't one emoji.
#[uniffi::export]
pub fn emoji_shortcode(emoji: String) -> Option<String> {
    akari_markdown::emoji_shortcode(&emoji)
}
use crate::names::Names;

/// A message's content as Discord formats it. Offsets are UTF-16 code units into `content`.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct FormattedText {
    pub blocks: Vec<TextBlock>,
    pub links: Vec<TextLink>,
    /// 1–30 emoji and whitespace, nothing else: shown large.
    pub jumbo: bool,
}

/// A paragraph, heading, subtext line or code block, inside its quotes and list items.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextBlock {
    pub kind: TextBlockKind,
    /// Outermost first.
    pub containers: Vec<TextContainer>,
    /// Set when the whole block is inside one spoiler.
    pub spoiler: Option<u16>,
    pub spans: Vec<TextSpan>,
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TextBlockKind {
    Paragraph,
    Heading {
        level: u8,
    },
    Subtext,
    /// Its code is in `Code` spans.
    Code {
        language: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum TextContainer {
    Quote,
    /// `marker`: this block shows the item's bullet or number.
    ListItem {
        ordered: bool,
        number: u32,
        marker: bool,
    },
}

/// A run of text or a token, with the styles around it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextSpan {
    pub kind: SpanKind,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    /// Spoilers are numbered in order; all parts of one share it.
    pub spoiler: Option<u16>,
    /// An index into `FormattedText::links`.
    pub link: Option<u16>,
    /// What the span stands for in `content`, with its own delimiters.
    pub start: u32,
    pub end: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum SpanKind {
    /// Shown as `content[start..end]`.
    Text {
        start: u32,
        end: u32,
    },
    Break,
    /// Shown as `content[start..end]`, in a monospace font.
    Code {
        start: u32,
        end: u32,
    },
    User {
        id: UserId,
        name: MentionName,
    },
    /// `color` is 0 for a role without one.
    Role {
        id: RoleId,
        name: MentionName,
        color: u32,
    },
    Channel {
        id: ChannelId,
        name: MentionName,
        kind: Option<ChannelType>,
    },
    Everyone,
    Here,
    /// `name` is "name", "name sub" or "name group sub".
    Command {
        name: String,
        id: u64,
    },
    Emoji {
        name: String,
        id: u64,
        animated: bool,
        /// The image to show in the text, animated when `animated` says so.
        key: ImageKey,
    },
    Timestamp {
        seconds: i64,
        style: TimestampStyle,
    },
    Navigation {
        target: GuildNavigation,
    },
}

/// A mention's name, as far as the store knows it.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum MentionName {
    Known {
        name: String,
    },
    /// A channel the user can't view.
    NoAccess,
    /// The store has the list it would be in, and it isn't there.
    Missing,
    /// The store doesn't have that list yet, e.g. before READY.
    NotLoaded,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct TextLink {
    /// The WHATWG serialization of the target.
    pub url: String,
    /// `[text](url "title")`, which shows text instead of the URL.
    pub masked: bool,
    pub title: Option<String>,
    /// Written in angle brackets, which stops Discord from embedding it.
    pub suppress_embed: bool,
    /// Set for Discord channel and message URLs.
    pub channel: Option<ChannelLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ChannelLink {
    /// `None` for direct messages.
    pub guild_id: Option<GuildId>,
    pub channel_id: ChannelId,
    pub message_id: Option<MessageId>,
    pub name: MentionName,
    pub kind: Option<ChannelType>,
}

/// `<t:…:style>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum TimestampStyle {
    /// `t`
    ShortTime,
    /// `T`
    MediumTime,
    /// `d`
    ShortDate,
    /// `D`
    LongDate,
    /// `f`, also when the style is left out
    LongDateShortTime,
    /// `F`
    FullDateShortTime,
    /// `s`
    ShortDateShortTime,
    /// `S`
    ShortDateMediumTime,
    /// `R`
    Relative,
}

/// `<id:…>`: a place in the server's navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum GuildNavigation {
    Customize,
    Browse,
    Guide,
    LinkedRoles { role_id: Option<RoleId> },
}

impl FormattedText {
    pub(crate) fn new(
        document: &Document,
        content: &str,
        names: &Names<'_>,
        message: Option<&state::Message>,
    ) -> Self {
        let offsets = Utf16Map::new(content);
        let at = |span: Span| (offsets.to_utf16(span.start), offsets.to_utf16(span.end));
        let text = |span: Span| content.get(span.range()).unwrap_or_default().to_owned();
        let blocks = document
            .blocks()
            .iter()
            .map(|block| {
                let (start, end) = at(block.source);
                TextBlock {
                    kind: match block.kind {
                        BlockKind::Heading(level) => TextBlockKind::Heading {
                            level: match level {
                                HeadingLevel::One => 1,
                                HeadingLevel::Two => 2,
                                HeadingLevel::Three => 3,
                            },
                        },
                        BlockKind::Subtext => TextBlockKind::Subtext,
                        BlockKind::CodeBlock { language } => TextBlockKind::Code {
                            language: language.map(text),
                        },
                        _ => TextBlockKind::Paragraph,
                    },
                    containers: block
                        .containers
                        .iter()
                        .map(|container| match *container {
                            Container::ListItem {
                                ordered,
                                number,
                                marker,
                            } => TextContainer::ListItem {
                                ordered,
                                number,
                                marker,
                            },
                            _ => TextContainer::Quote,
                        })
                        .collect(),
                    spoiler: block.spoiler.map(|id| id.0),
                    spans: block
                        .inlines
                        .iter()
                        .map(|inline| {
                            let (start, end) = at(inline.source);
                            TextSpan {
                                kind: span_kind(inline.kind, &at, &text, names, message),
                                bold: inline.style.contains(Style::BOLD),
                                italic: inline.style.contains(Style::ITALIC),
                                underline: inline.style.contains(Style::UNDERLINE),
                                strike: inline.style.contains(Style::STRIKE),
                                spoiler: inline.spoiler.map(|id| id.0),
                                link: inline.link.map(|id| id.0),
                                start,
                                end,
                            }
                        })
                        .collect(),
                    start,
                    end,
                }
            })
            .collect();
        Self {
            blocks,
            links: document
                .links()
                .iter()
                .map(|link| text_link(link, names))
                .collect(),
            jumbo: document.jumbo(),
        }
    }
}

fn span_kind(
    kind: InlineKind,
    at: &impl Fn(Span) -> (u32, u32),
    text: &impl Fn(Span) -> String,
    names: &Names<'_>,
    message: Option<&state::Message>,
) -> SpanKind {
    match kind {
        InlineKind::Text(span) => {
            let (start, end) = at(span);
            SpanKind::Text { start, end }
        }
        InlineKind::Code(span) => {
            let (start, end) = at(span);
            SpanKind::Code { start, end }
        }
        InlineKind::UserMention(id) => {
            let id = UserId::new(id);
            SpanKind::User {
                id,
                name: message.map_or(MentionName::NotLoaded, |message| names.user(id, message)),
            }
        }
        InlineKind::RoleMention(id) => {
            let id = RoleId::new(id);
            let (name, color) = names.role(id);
            SpanKind::Role { id, name, color }
        }
        InlineKind::ChannelMention(id) => {
            let id = ChannelId::new(id);
            let (name, kind) = names.channel(id);
            SpanKind::Channel { id, name, kind }
        }
        InlineKind::Everyone => SpanKind::Everyone,
        InlineKind::Here => SpanKind::Here,
        InlineKind::Command { name, id } => SpanKind::Command {
            name: text(name),
            id,
        },
        InlineKind::CustomEmoji { name, id, animated } => SpanKind::Emoji {
            name: text(name),
            id,
            animated,
            key: ImageKey::Emoji {
                emoji_id: EmojiId::new(id),
                animated,
            },
        },
        InlineKind::Timestamp { seconds, style } => SpanKind::Timestamp {
            seconds,
            style: timestamp_style(style),
        },
        InlineKind::Navigation(target) => SpanKind::Navigation {
            target: match target {
                Navigation::Customize => GuildNavigation::Customize,
                Navigation::Browse => GuildNavigation::Browse,
                Navigation::LinkedRoles(role) => GuildNavigation::LinkedRoles {
                    role_id: role.map(RoleId::new),
                },
                _ => GuildNavigation::Guide,
            },
        },
        _ => SpanKind::Break,
    }
}

fn timestamp_style(style: akari_markdown::TimestampStyle) -> TimestampStyle {
    use akari_markdown::TimestampStyle as S;
    match style {
        S::ShortTime => TimestampStyle::ShortTime,
        S::MediumTime => TimestampStyle::MediumTime,
        S::ShortDate => TimestampStyle::ShortDate,
        S::LongDate => TimestampStyle::LongDate,
        S::LongDateShortTime => TimestampStyle::LongDateShortTime,
        S::FullDateShortTime => TimestampStyle::FullDateShortTime,
        S::ShortDateShortTime => TimestampStyle::ShortDateShortTime,
        S::ShortDateMediumTime => TimestampStyle::ShortDateMediumTime,
        S::Relative => TimestampStyle::Relative,
    }
}

fn text_link(link: &Link, names: &Names<'_>) -> TextLink {
    let (masked, title) = match &link.kind {
        LinkKind::Masked { title } => (true, title.as_deref().map(str::to_owned)),
        _ => (false, None),
    };
    TextLink {
        url: link.url.to_string(),
        masked,
        title,
        suppress_embed: link.suppress_embed,
        channel: link.discord_channel().map(|target| {
            let channel_id = ChannelId::new(target.channel_id);
            let (name, kind) = names.channel(channel_id);
            ChannelLink {
                guild_id: target.guild_id.map(GuildId::new),
                channel_id,
                message_id: target.message_id.map(MessageId::new),
                name,
                kind,
            }
        }),
    }
}

/// The formatted text of `content` without a store: every name is `NotLoaded`. For tests and
/// previews.
#[uniffi::export]
pub fn formatted_text(content: String) -> FormattedText {
    FormattedText::new(
        &akari_markdown::parse(&content),
        &content,
        &Names::none(),
        None,
    )
}
