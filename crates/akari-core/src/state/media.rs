//! How a message's attachments show (D141–D144): which go in the media mosaic and which are file
//! rows, what covers them, and the age gate before a channel's messages. The official client's
//! rules (canary 649848: modules 394839, 282108, 632119, 444802, 294520, 323073), with the
//! departures the plan lists.

use std::collections::HashSet;

use crate::model::{ChannelType, GuildId, MessageId, Permissions};

use super::settings::{DisplaySettings, ExplicitContentFilter, Redaction, SpoilerSetting};
use super::types::{Attachment, Message};

/// Where an attachment shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AttachmentDisplay {
    /// In the message's mosaic of images and videos.
    Media,
    /// A file row with its name and size.
    File,
}

/// Why media is covered; only the most important reason counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MediaCover {
    /// Flagged by Discord's scan in a place the user's settings cover: the user can show it
    /// and hide it again.
    Sensitive,
    /// Waiting for Discord's scan, at most about 3 s.
    Pending,
    /// A spoiler: a click shows it for the session.
    Spoiler,
}

/// One of a message's attachments as it shows: in Discord's order, the media first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShownAttachment {
    /// Its place in [`Message::attachments`].
    pub index: usize,
    pub display: AttachmentDisplay,
    /// Only media is covered.
    pub cover: Option<MediaCover>,
}

/// Whether a channel's messages wait behind the age gate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AgeGate {
    Open,
    /// Age-restricted: the user can continue, which opens every channel of the server.
    /// `server`: the whole server is, not just the channel.
    Ask {
        server: bool,
    },
    /// Discord knows the user is under 18.
    TooYoung {
        server: bool,
    },
}

const CONTAINS_EXPLICIT: u64 = 1 << 4;
const CONTAINS_GORE: u64 = 1 << 6;
// Scans before this found no gore (the client's `validContentScanVersion`).
const GORE_SCAN: u32 = 4;

/// The kinds of a conversation whose settings decide what's covered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Place {
    Guild,
    FriendDm,
    NonFriendDm,
}

/// The harms a place covers.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Covered {
    pub(crate) explicit: bool,
    pub(crate) gore: bool,
}

impl Covered {
    pub(crate) fn any(self) -> bool {
        self.explicit || self.gore
    }
}

/// What a place covers, with the user's settings resolved as the official client resolves an
/// unset one: by the legacy filter and whether Discord knows the user is under 18. Self-harm
/// covers nothing: the client ignores its setting.
pub(crate) fn covered(
    display: &DisplaySettings,
    place: Place,
    nsfw_allowed: Option<bool>,
) -> Covered {
    let minor = nsfw_allowed == Some(false);
    let pick = |settings: &super::settings::ExplicitContentSettings| match place {
        Place::Guild => settings.guilds,
        Place::FriendDm => settings.friend_dm,
        Place::NonFriendDm => settings.non_friend_dm,
    };
    let explicit = match pick(&display.explicit_content) {
        Redaction::Unset => match (place, minor, display.explicit_content_filter) {
            (_, true, _) => true,
            (Place::Guild, false, _) => false,
            (Place::FriendDm, false, filter) => filter == ExplicitContentFilter::All,
            (Place::NonFriendDm, false, filter) => filter != ExplicitContentFilter::Disabled,
        },
        setting => covers(setting),
    };
    let gore = match pick(&display.gore) {
        Redaction::Unset => minor || place == Place::NonFriendDm,
        setting => covers(setting),
    };
    Covered { explicit, gore }
}

// BLUR and BLOCK look the same on the client; the server enforces BLOCK when sending.
fn covers(setting: Redaction) -> bool {
    matches!(setting, Redaction::Blur | Redaction::Block)
}

/// The cover over one attachment shown as media.
pub(crate) fn media_cover(
    attachment: &Attachment,
    covered: Covered,
    scan: Scan,
    spoilers_shown: bool,
) -> Option<MediaCover> {
    let flags = attachment.flags;
    if (covered.explicit && flags & CONTAINS_EXPLICIT != 0)
        || (covered.gore && flags & CONTAINS_GORE != 0)
    {
        return Some(MediaCover::Sensitive);
    }
    if covered.any()
        && !scan.settled
        && unscanned(attachment.content_scan_version, covered, scan.newest)
    {
        return Some(MediaCover::Pending);
    }
    (attachment.is_spoiler() && !spoilers_shown).then_some(MediaCover::Spoiler)
}

// The client's `P`: no version yet, or with gore covered one from before scans found gore.
pub(crate) fn unscanned(version: Option<u32>, covered: Covered, newest: Option<u32>) -> bool {
    match version {
        None => true,
        Some(version) => covered.gore && version < newest.unwrap_or(GORE_SCAN).min(GORE_SCAN),
    }
}

/// What the store knows about a message's scan.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Scan {
    /// Asked for long enough ago that the client gives up waiting (its −1).
    pub(crate) settled: bool,
    /// READY's `explicit_content_scan_version`.
    pub(crate) newest: Option<u32>,
}

/// The attachments a message shows, in Discord's order: images and videos in the mosaic, then
/// the files, each in the message's order; thumbnails left out.
pub(crate) fn shown_attachments(
    message: &Message,
    display: &DisplaySettings,
    cover: impl Fn(&Attachment) -> Option<MediaCover>,
) -> Vec<ShownAttachment> {
    let mut media = Vec::new();
    let mut files = Vec::new();
    for (index, attachment) in message.attachments.iter().enumerate() {
        if attachment.is_thumbnail() {
            continue;
        }
        if display.inline_attachment_media && attachment.is_visual() {
            media.push(ShownAttachment {
                index,
                display: AttachmentDisplay::Media,
                cover: cover(attachment),
            });
        } else {
            files.push(ShownAttachment {
                index,
                display: AttachmentDisplay::File,
                cover: None,
            });
        }
    }
    media.extend(files);
    media
}

/// Whether spoilers show without a click: the setting, or a moderator's with Manage Messages.
pub(crate) fn spoilers_shown(
    display: &DisplaySettings,
    permissions: impl FnOnce() -> Option<Permissions>,
) -> bool {
    match display.render_spoilers {
        SpoilerSetting::Always => true,
        SpoilerSetting::IfModerator => permissions()
            .is_some_and(|permissions| permissions.contains(Permissions::MANAGE_MESSAGES)),
        _ => false,
    }
}

/// The place a message is in, by its author for DMs (module 282108 `M`).
pub(crate) fn place(kind: ChannelType, author_is_friend: bool) -> Place {
    match kind {
        ChannelType::Dm | ChannelType::GroupDm if author_is_friend => Place::FriendDm,
        ChannelType::Dm | ChannelType::GroupDm => Place::NonFriendDm,
        _ => Place::Guild,
    }
}

/// The age gate for a guild channel that is age-restricted itself (or by its parent) or in an
/// age-restricted server.
pub(crate) fn age_gate(
    channel_restricted: bool,
    server: bool,
    nsfw_allowed: Option<bool>,
    agreed: bool,
) -> AgeGate {
    if !channel_restricted && !server {
        AgeGate::Open
    } else if nsfw_allowed == Some(false) {
        AgeGate::TooYoung { server }
    } else if agreed {
        AgeGate::Open
    } else {
        AgeGate::Ask { server }
    }
}

/// Messages whose scan was asked this session, and those whose wait ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Scans {
    pub(crate) asked: HashSet<MessageId>,
    pub(crate) settled: HashSet<MessageId>,
    // Asked, but the request failed for a while: asked again on a reload.
    pub(crate) failed: HashSet<MessageId>,
}

/// Guilds whose age-restricted channels the user agreed to see, kept per account.
pub(crate) type AgeAgreements = std::collections::BTreeSet<GuildId>;

#[cfg(test)]
mod tests;
