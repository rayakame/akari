use std::process::ExitCode;
use std::str::FromStr;
use std::sync::Arc;

use akari_core::media::{AnimatedFormat, EmbedSlot, ImageKey, ImageRequest, cdn_size, proxy_size};
use akari_core::model::{ChannelId, MessageId, Snowflake, StickerFormatType};
use akari_core::state::{ImageHash, Store};
use akari_core::{Account, DiscordClient, MessageLoad};
use clap::{Args, Subcommand, ValueEnum};

use crate::keychain::Accounts;
use crate::report;
use crate::session::{self, closed_message, stored_token};

/// Which image, by Discord's IDs.
#[derive(Subcommand)]
pub enum Kind {
    /// A user's avatar.
    Avatar { user_id: u64, hash: String },
    /// A member's avatar for one guild.
    Member {
        guild_id: u64,
        user_id: u64,
        hash: String,
    },
    /// One of Discord's six default avatars.
    Default { index: u8 },
    /// A server's icon.
    Icon { guild_id: u64, hash: String },
    /// A group DM's icon.
    Group { channel_id: u64, hash: String },
    /// A role's icon.
    Role { role_id: u64, hash: String },
    /// A custom emoji; `--moving` for an animated one.
    Emoji {
        emoji_id: u64,
        #[arg(long)]
        moving: bool,
    },
    /// A sticker.
    Sticker {
        sticker_id: u64,
        #[arg(value_enum)]
        format: StickerFormat,
    },
    /// A message's attachment (a video's poster).
    Attachment {
        channel_id: u64,
        message_id: u64,
        attachment_id: u64,
    },
    /// An image of a message's embed.
    Embed {
        channel_id: u64,
        message_id: u64,
        /// The embed's place in the message, from 0.
        index: u8,
        #[arg(value_enum)]
        slot: Slot,
    },
}

#[derive(Clone, Copy, ValueEnum)]
pub enum StickerFormat {
    Png,
    Apng,
    Lottie,
    Gif,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Slot {
    Image,
    Thumbnail,
    Author,
    Footer,
}

#[derive(Clone, Copy, ValueEnum)]
pub enum Animation {
    Webp,
    Gif,
}

/// A box's size in points: `40` or `400x300`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Points {
    width: u32,
    height: u32,
}

impl FromStr for Points {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let side = |text: &str| {
            text.parse::<u32>()
                .ok()
                .filter(|side| *side > 0)
                .ok_or_else(|| format!("{text:?} isn't a positive number of points"))
        };
        match text.split_once('x') {
            Some((width, height)) => Ok(Self {
                width: side(width)?,
                height: side(height)?,
            }),
            None => {
                let side = side(text)?;
                Ok(Self {
                    width: side,
                    height: side,
                })
            }
        }
    }
}

#[derive(Args)]
pub struct ImageArgs {
    #[command(subcommand)]
    kind: Kind,
    /// The box's size in points, `40` or `400x300`.
    #[arg(long, global = true, default_value = "40")]
    points: Points,
    /// The screen's backing scale: 2 on Retina, 1 on a standard display.
    #[arg(long, global = true, default_value_t = 2, value_parser = clap::value_parser!(u32).range(1..=4))]
    scale: u32,
    /// Ask for the animation, in this form.
    #[arg(long, global = true, value_enum)]
    animated: Option<Animation>,
    /// Don't connect: only images already on disk answer.
    #[arg(long, global = true)]
    offline: bool,
}

fn key(kind: &Kind) -> ImageKey {
    match kind {
        Kind::Avatar { user_id, hash } => ImageKey::Avatar {
            user: Snowflake::new(*user_id),
            hash: ImageHash::parse(hash),
        },
        Kind::Member {
            guild_id,
            user_id,
            hash,
        } => ImageKey::MemberAvatar {
            guild: Snowflake::new(*guild_id),
            user: Snowflake::new(*user_id),
            hash: ImageHash::parse(hash),
        },
        Kind::Default { index } => ImageKey::DefaultAvatar { index: *index },
        Kind::Icon { guild_id, hash } => ImageKey::GuildIcon {
            guild: Snowflake::new(*guild_id),
            hash: ImageHash::parse(hash),
        },
        Kind::Group { channel_id, hash } => ImageKey::GroupIcon {
            channel: Snowflake::new(*channel_id),
            hash: ImageHash::parse(hash),
        },
        Kind::Role { role_id, hash } => ImageKey::RoleIcon {
            role: Snowflake::new(*role_id),
            hash: ImageHash::parse(hash),
        },
        Kind::Emoji { emoji_id, moving } => ImageKey::Emoji {
            id: Snowflake::new(*emoji_id),
            animated: *moving,
        },
        Kind::Sticker { sticker_id, format } => ImageKey::Sticker {
            id: Snowflake::new(*sticker_id),
            format: match format {
                StickerFormat::Png => StickerFormatType::Png,
                StickerFormat::Apng => StickerFormatType::Apng,
                StickerFormat::Lottie => StickerFormatType::Lottie,
                StickerFormat::Gif => StickerFormatType::Gif,
            },
        },
        Kind::Attachment {
            channel_id,
            message_id,
            attachment_id,
        } => ImageKey::Attachment {
            channel: Snowflake::new(*channel_id),
            message: Snowflake::new(*message_id),
            id: Snowflake::new(*attachment_id),
        },
        Kind::Embed {
            channel_id,
            message_id,
            index,
            slot,
        } => ImageKey::EmbedMedia {
            channel: Snowflake::new(*channel_id),
            message: Snowflake::new(*message_id),
            embed: *index,
            slot: match slot {
                Slot::Image => EmbedSlot::Image,
                Slot::Thumbnail => EmbedSlot::Thumbnail,
                Slot::Author => EmbedSlot::AuthorIcon,
                Slot::Footer => EmbedSlot::FooterIcon,
            },
        },
    }
}

fn message_of(kind: &Kind) -> Option<(ChannelId, MessageId)> {
    match kind {
        Kind::Attachment {
            channel_id,
            message_id,
            ..
        }
        | Kind::Embed {
            channel_id,
            message_id,
            ..
        } => Some((Snowflake::new(*channel_id), Snowflake::new(*message_id))),
        _ => None,
    }
}

// The source's pixels, when the store knows them: what the media proxy's size follows.
fn source_size(store: &Store, key: &ImageKey) -> Option<(u32, u32)> {
    match key {
        ImageKey::Attachment {
            channel,
            message,
            id,
        } => {
            let message = store.message(*channel, *message)?;
            let attachment = message.attachments.iter().find(|a| a.id == *id)?;
            Some((attachment.width?, attachment.height?))
        }
        ImageKey::EmbedMedia {
            channel,
            message,
            embed,
            slot,
        } => {
            let message = store.message(*channel, *message)?;
            let embed = message.embeds.get(usize::from(*embed))?;
            let media = match slot {
                EmbedSlot::Image => embed.image.as_ref(),
                EmbedSlot::Thumbnail => embed.thumbnail.as_ref(),
                _ => None,
            }?;
            Some((media.width?, media.height?))
        }
        _ => None,
    }
}

/// What the core asks for this box, from the same rules it uses.
fn asked(store: &Store, key: &ImageKey, pixels: (u32, u32)) -> String {
    match key {
        ImageKey::DefaultAvatar { .. } => "the CDN's 256 px PNG (no size)".to_owned(),
        ImageKey::Attachment { .. } | ImageKey::EmbedMedia { .. } => {
            match source_size(store, key) {
                Some(source) => match proxy_size(source, pixels) {
                    Some((width, height)) => {
                        format!("{width} x {height} px from the media proxy")
                    }
                    None => format!(
                        "the source's own {} x {} px from the media proxy",
                        source.0, source.1
                    ),
                },
                None => "as Discord gives it".to_owned(),
            }
        }
        _ => format!("size={} from the CDN", cdn_size(pixels.0.max(pixels.1))),
    }
}

pub async fn run<S: Accounts>(client: &DiscordClient, store: &Arc<S>, args: ImageArgs) -> ExitCode {
    let key = key(&args.kind);
    let pixels = (
        args.points.width * args.scale,
        args.points.height * args.scale,
    );
    let request = ImageRequest {
        key: key.clone(),
        width: pixels.0,
        height: pixels.1,
        animated: args.animated.map(|animation| match animation {
            Animation::Webp => AnimatedFormat::WebP,
            Animation::Gif => AnimatedFormat::Gif,
        }),
    };
    let opened = if args.offline {
        let (id, token) = match stored_token(client, store).await {
            Ok(stored) => stored,
            Err(code) => return code,
        };
        match client.account(token, id) {
            Ok(account) => Opened::Offline(account),
            Err(err) => {
                eprintln!("{}", closed_message(Some(&err)));
                return ExitCode::FAILURE;
            }
        }
    } else {
        match session::open(client, store, false).await {
            Ok(session) => Opened::Online(session),
            Err(code) => return code,
        }
    };
    let account = opened.account();
    if let (Some((channel, message)), false) = (message_of(&args.kind), args.offline) {
        account.view_channel(channel);
        if let Err(err) = account
            .load_messages(
                channel,
                MessageLoad::Around {
                    id: message,
                    limit: 5,
                },
            )
            .await
        {
            eprintln!("Couldn't load the message: {}", report(&err));
        }
    }
    println!(
        "box     {} x {} px ({} x {} pt at {}x)",
        pixels.0, pixels.1, args.points.width, args.points.height, args.scale
    );
    println!("asked   {}", asked(account.store(), &key, pixels));
    let code = match account.image(request).await {
        Ok(file) => {
            println!("file    {}", file.path.display());
            let smaller = file.width < pixels.0 && file.height < pixels.1;
            println!(
                "pixels  {} x {}{}{}",
                file.width,
                file.height,
                if file.animated { ", animated" } else { "" },
                if smaller { " (source smaller)" } else { "" }
            );
            println!(
                "from    {}",
                if file.from_disk { "disk" } else { "network" }
            );
            ExitCode::SUCCESS
        }
        Err(err) => {
            println!("failed  {err}");
            ExitCode::FAILURE
        }
    };
    match opened {
        Opened::Online(session) => session.close(None).await,
        Opened::Offline(account) => account.close(),
    }
    code
}

enum Opened {
    Online(session::Session),
    Offline(Account),
}

impl Opened {
    fn account(&self) -> &Account {
        match self {
            Self::Online(session) => &session.account,
            Self::Offline(account) => account,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_are_one_side_or_two() {
        assert_eq!(
            "40".parse::<Points>(),
            Ok(Points {
                width: 40,
                height: 40
            })
        );
        assert_eq!(
            "400x300".parse::<Points>(),
            Ok(Points {
                width: 400,
                height: 300
            })
        );
        assert!("0".parse::<Points>().is_err());
        assert!("40x".parse::<Points>().is_err());
        assert!("big".parse::<Points>().is_err());
    }
}
