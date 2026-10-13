use std::ops::Range;

/// Parsed message content: blocks in order, and the links their inlines point to.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Document {
    blocks: Box<[Block]>,
    links: Box<[Link]>,
    jumbo: bool,
    spoiler_count: u16,
    link_spans: Box<[Span]>,
}

impl Document {
    pub(crate) fn new(
        blocks: Vec<Block>,
        links: Vec<(Link, Span)>,
        jumbo: bool,
        spoiler_count: u16,
    ) -> Self {
        let (links, link_spans): (Vec<Link>, Vec<Span>) = links.into_iter().unzip();
        Self {
            blocks: blocks.into(),
            links: links.into(),
            jumbo,
            spoiler_count,
            link_spans: link_spans.into(),
        }
    }

    pub(crate) fn link_spans(&self) -> &[Span] {
        &self.link_spans
    }

    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// Indexed by `Inline::link`.
    pub fn links(&self) -> &[Link] {
        &self.links
    }

    /// 1–27 emoji (Unicode or custom) and whitespace, nothing else.
    pub fn jumbo(&self) -> bool {
        self.jumbo
    }

    pub fn spoiler_count(&self) -> u16 {
        self.spoiler_count
    }
}

/// A paragraph, heading, subtext line or code block, inside its containers.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    pub kind: BlockKind,
    /// Outermost first.
    pub containers: Box<[Container]>,
    /// Set when the whole block is inside one spoiler (a code block in `||…||`).
    pub spoiler: Option<SpoilerId>,
    /// A code block's code is one or more `Code` inlines, shown one after another.
    pub inlines: Box<[Inline]>,
    pub source: Span,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Paragraph,
    Heading(HeadingLevel),
    Subtext,
    CodeBlock { language: Option<Span> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadingLevel {
    One,
    Two,
    Three,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Quote,
    /// `marker`: this block shows the item's bullet or number (an item's first block).
    ListItem {
        ordered: bool,
        number: u32,
        marker: bool,
    },
}

/// A run of text or a token, with the styles around it.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inline {
    pub kind: InlineKind,
    pub style: Style,
    pub spoiler: Option<SpoilerId>,
    pub link: Option<LinkId>,
    /// The source this inline shows, with the delimiters that belong to it alone: an escape's
    /// backslash, inline code's backticks, a token's brackets. Not the delimiters of styles.
    pub source: Span,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineKind {
    /// Shown as the source text in this span.
    Text(Span),
    Break,
    /// Shown as the source text in this span, in a monospace font.
    Code(Span),
    UserMention(u64),
    RoleMention(u64),
    ChannelMention(u64),
    Everyone,
    Here,
    /// `name` covers "name", "name sub" or "name group sub".
    Command {
        name: Span,
        id: u64,
    },
    CustomEmoji {
        name: Span,
        id: u64,
        animated: bool,
    },
    Timestamp {
        seconds: i64,
        style: TimestampStyle,
    },
    Navigation(Navigation),
}

/// `<t:…:style>`; `LongDateShortTime` (`f`) when the style is left out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimestampStyle {
    /// `t`
    ShortTime,
    /// `T`
    MediumTime,
    /// `d`
    ShortDate,
    /// `D`
    LongDate,
    /// `f`
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Navigation {
    Customize,
    Browse,
    /// Also `<id:home>`.
    Guide,
    LinkedRoles(Option<u64>),
}

/// Bold, italic, underline and strikethrough, in any combination.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Style(pub(crate) u8);

impl Style {
    pub const BOLD: Style = Style(1);
    pub const ITALIC: Style = Style(2);
    pub const UNDERLINE: Style = Style(4);
    pub const STRIKE: Style = Style(8);

    pub fn contains(self, other: Style) -> bool {
        self.0 & other.0 == other.0
    }

    pub(crate) const fn with(self, other: Style) -> Style {
        Style(self.0 | other.0)
    }
}

/// Spoilers are numbered in source order; all parts of one spoiler share its id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SpoilerId(pub u16);

/// An index into `Document::links`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LinkId(pub u16);

/// UTF-8 byte offsets into the source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl Span {
    pub(crate) fn new(range: Range<usize>) -> Self {
        Self {
            start: range.start as u32,
            end: range.end as u32,
        }
    }

    pub fn range(self) -> Range<usize> {
        self.start as usize..self.end as usize
    }

    pub fn is_empty(self) -> bool {
        self.start == self.end
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    /// The WHATWG serialization of the target.
    pub url: Box<str>,
    pub kind: LinkKind,
    /// Written in angle brackets, which stops Discord from embedding it.
    pub suppress_embed: bool,
}

#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkKind {
    /// A URL in the text, with or without angle brackets.
    Plain,
    /// `[text](url "title")`.
    Masked { title: Option<Box<str>> },
    /// `<user@example.com>`.
    Email,
    /// `<+1 555 123 4567>`.
    Phone,
}

impl Link {
    /// A link to a Discord channel or message (discord.com, ptb., canary., discordapp.com).
    pub fn discord_channel(&self) -> Option<DiscordChannelLink> {
        let url = url::Url::parse(&self.url).ok()?;
        if !matches!(url.scheme(), "https" | "http") {
            return None;
        }
        let host = url.host_str()?;
        let base = host
            .strip_prefix("ptb.")
            .or_else(|| host.strip_prefix("canary."))
            .unwrap_or(host);
        if !matches!(base, "discord.com" | "discordapp.com") {
            return None;
        }
        let mut segments: Vec<&str> = url.path_segments()?.collect();
        // A trailing slash is the same page; any other empty segment makes it another URL.
        if segments.len() > 1 && segments.last() == Some(&"") {
            segments.pop();
        }
        let (guild, channel, message) = match segments[..] {
            ["channels", guild, channel] => (guild, channel, None),
            ["channels", guild, channel, message] => (guild, channel, Some(message)),
            _ => return None,
        };
        let guild_id = match guild {
            "@me" => None,
            id => Some(id.parse().ok()?),
        };
        let channel_id = channel.parse().ok()?;
        let message_id = message.map(str::parse).transpose().ok()?;
        Some(DiscordChannelLink {
            guild_id,
            channel_id,
            message_id,
        })
    }
}

/// Where a Discord channel or message URL points.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiscordChannelLink {
    /// `None` for direct messages (`@me`).
    pub guild_id: Option<u64>,
    pub channel_id: u64,
    pub message_id: Option<u64>,
}
