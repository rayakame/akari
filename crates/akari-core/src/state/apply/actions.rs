use std::sync::Arc;

use super::State;
use super::creating::{EPHEMERAL, STARTS_THREADS as USER_MESSAGE};
use crate::model::{ChannelId, ChannelType, MessageId, MessageReferenceType, Permissions};
use crate::state::permissions::{PRIVATE, silenced};
use crate::state::types::{Channel, Delivery, Message, MessageActions};

// The type sets of Discord's client (module 448761, canary 671549).
const REPLYABLE: [u16; 21] = [
    0, 7, 19, 20, 23, 24, 25, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 45, 46, 69,
];
const UNDELETABLE: [u16; 11] = [1, 2, 3, 4, 5, 21, 35, 56, 57, 64, 68];
const AUTO_MODERATION_ACTION: u16 = 24;
const MEDIA_MENTION: u16 = 68;
const IS_VOICE_MESSAGE: u64 = 1 << 13;

impl State {
    pub(crate) fn message_actions(
        &self,
        channel: ChannelId,
        message: MessageId,
        now_millis: i64,
    ) -> MessageActions {
        self.acted_on(channel, message)
            .map_or_else(MessageActions::default, |found| {
                self.message_actions_of(&found, now_millis)
            })
    }

    pub(crate) fn message_actions_of(&self, message: &Message, now_millis: i64) -> MessageActions {
        let none = MessageActions::default();
        let Some(channel) = self.entities.channels.get(&message.channel_id) else {
            return none;
        };
        match message.delivery {
            Delivery::Sent if message.flags & EPHEMERAL == 0 => {}
            Delivery::Failed => {
                return MessageActions {
                    delete: true,
                    ..none
                };
            }
            Delivery::Sent | Delivery::Pending => return none,
        }
        let private = matches!(channel.kind, ChannelType::Dm | ChannelType::GroupDm);
        let permissions = if private {
            PRIVATE
        } else {
            self.permissions(channel.id, now_millis)
                .unwrap_or(Permissions::NONE)
        };
        if !permissions.contains(Permissions::VIEW_CHANNEL) {
            return none;
        }
        let can = |needed| permissions.contains(needed);

        let system_dm = self.is_system_dm(channel);
        let media_post = self.is_media_post(channel);
        let voice = matches!(
            channel.kind,
            ChannelType::GuildVoice | ChannelType::GuildStageVoice
        );
        let thread = channel.thread.as_deref().filter(|_| channel.is_thread());
        // An archived thread counts where writing would reopen it; a locked one never does.
        let writable = thread
            .is_none_or(|info| !info.archived || (!info.locked && can(Permissions::SEND_MESSAGES)));

        let kind = u16::from(message.kind);
        let user_message = USER_MESSAGE.contains(&kind);
        let own = self.me() == Some(message.author.id);
        let moderator = can(Permissions::MANAGE_MESSAGES);
        let sticker_alone = !message.stickers.is_empty() && message.content.is_empty();
        let forward = message
            .reference
            .as_deref()
            .is_some_and(|reference| reference.kind == MessageReferenceType::Forward);

        let reply = !system_dm
            && can(Permissions::SEND_MESSAGES | Permissions::READ_MESSAGE_HISTORY)
            && REPLYABLE.contains(&kind);
        let pinnable = !system_dm
            && user_message
            && can(Permissions::PIN_MESSAGES | Permissions::READ_MESSAGE_HISTORY)
            && writable
            && !voice
            && !media_post;
        let reactions = !system_dm && writable && !media_post;

        MessageActions {
            reply,
            reply_pings: reply && !own,
            mention_toggle: reply && !own && !private,
            // Discord opens a disabled editor where the user may not send.
            edit: !system_dm
                && own
                && user_message
                && kind != MEDIA_MENTION
                && !sticker_alone
                && message.flags & IS_VOICE_MESSAGE == 0
                && !forward
                && writable
                && can(Permissions::SEND_MESSAGES),
            delete: (own || moderator)
                && writable
                && !UNDELETABLE.contains(&kind)
                && (kind != AUTO_MODERATION_ACTION || moderator),
            pin: pinnable && !message.pinned,
            unpin: pinnable && message.pinned,
            react: reactions && can(Permissions::ADD_REACTIONS),
            toggle_reaction: reactions && !self.is_silenced(channel, now_millis),
            remove_reactions: moderator && writable,
            mark_unread: writable,
            copy_link: true,
            create_thread: self.starts_thread(message, now_millis),
            view_thread: self.views_thread_of(message, now_millis),
        }
    }

    fn acted_on(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.message(channel, id)
            .or_else(|| self.windows.shown(channel, id))
            .or_else(|| {
                self.messages(channel)?
                    .pending
                    .into_iter()
                    .find(|message| message.id == id)
            })
    }

    fn is_system_dm(&self, channel: &Channel) -> bool {
        channel.kind == ChannelType::Dm
            && channel
                .recipients
                .iter()
                .any(|id| self.entities.users.get(id).is_some_and(|user| user.system))
    }

    fn is_media_post(&self, channel: &Channel) -> bool {
        channel.is_thread()
            && channel
                .parent_id
                .and_then(|parent| self.entities.channels.get(&parent))
                .is_some_and(|parent| parent.kind == ChannelType::GuildMedia)
    }

    fn is_silenced(&self, channel: &Channel, now_millis: i64) -> bool {
        let Some(guild) = channel.guild_id else {
            return false;
        };
        match (
            self.entities.guilds.get(&guild),
            self.entities.members.get(&guild),
            self.me(),
        ) {
            (Some(guild), Some(member), Some(me)) => silenced(guild, me, member, now_millis),
            _ => false,
        }
    }

    // The thread has the message's ID; a post's first message has its own post's.
    fn views_thread_of(&self, message: &Message, now_millis: i64) -> bool {
        let thread: ChannelId = message.id.cast();
        thread != message.channel_id
            && self
                .entities
                .channels
                .get(&thread)
                .is_some_and(|channel| channel.is_thread())
            && self
                .permissions(thread, now_millis)
                .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
    }
}
