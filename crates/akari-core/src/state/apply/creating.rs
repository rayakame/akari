use std::time::{Duration, Instant, SystemTime};

use super::State;
use crate::model::{self, ChannelId, ChannelType, MessageId, Permissions};
use crate::state::events::StoreEvent;
use crate::state::read_states::Ack;
use crate::state::send::Slowmode;
use crate::state::types::{Delivery, Message, PostCreation, ThreadCreation, ThreadMembership};

// The kinds Discord's client lets a thread start from: a user's message, reply or command and a
// few more that render as one (module 406704).
pub(super) const STARTS_THREADS: [u16; 9] = [0, 19, 20, 23, 26, 41, 45, 47, 68];
pub(super) const EPHEMERAL: u64 = 1 << 6;
// Discord's client waits the interval, or the wait a 20016 names, and 100 ms more before it
// lets the user create again.
const COOLDOWN_MARGIN: Duration = Duration::from_millis(100);

impl State {
    pub(crate) fn thread_creation(&self, parent: ChannelId, now_millis: i64) -> ThreadCreation {
        let Some(channel) = self.entities.channels.get(&parent) else {
            return ThreadCreation::default();
        };
        let text = channel.kind == ChannelType::GuildText;
        if !text && channel.kind != ChannelType::GuildNews {
            return ThreadCreation::default();
        }
        // The first message is required: without sending in threads, Akari offers no thread.
        let Some(permissions) = self
            .permissions(parent, now_millis)
            .filter(|permissions| permissions.contains(Permissions::SEND_MESSAGES_IN_THREADS))
        else {
            return ThreadCreation::default();
        };
        ThreadCreation {
            public: permissions
                .contains(Permissions::CREATE_PUBLIC_THREADS | Permissions::READ_MESSAGE_HISTORY),
            private: text && permissions.contains(Permissions::CREATE_PRIVATE_THREADS),
        }
    }

    // Module 406704: SEND_MESSAGES on the forum, which a timeout takes away; module 691060: a
    // moderated tag only with MANAGE_THREADS.
    pub(crate) fn post_creation(&self, forum: ChannelId, now_millis: i64) -> PostCreation {
        let Some(channel) = self
            .entities
            .channels
            .get(&forum)
            .filter(|channel| channel.is_forum())
        else {
            return PostCreation::default();
        };
        let Some(permissions) = self
            .permissions(forum, now_millis)
            .filter(|permissions| permissions.contains(Permissions::SEND_MESSAGES))
        else {
            return PostCreation::default();
        };
        let moderator = permissions.contains(Permissions::MANAGE_THREADS);
        let tags = channel.forum.as_deref().map_or_else(Vec::new, |settings| {
            settings
                .tags
                .iter()
                .filter(|tag| moderator || !tag.moderated)
                .map(|tag| tag.id)
                .collect()
        });
        PostCreation {
            allowed: true,
            tags,
            requires_tag: channel.requires_tag(),
        }
    }

    pub(crate) fn can_start_thread(
        &self,
        channel: ChannelId,
        message: MessageId,
        now_millis: i64,
    ) -> bool {
        self.message(channel, message)
            .is_some_and(|found| self.starts_thread(&found, now_millis))
    }

    // Only the public permission counts: a thread from a message is never private.
    pub(super) fn starts_thread(&self, message: &Message, now_millis: i64) -> bool {
        message.delivery == Delivery::Sent
            && STARTS_THREADS.contains(&u16::from(message.kind))
            && !message.has_thread()
            && message.flags & EPHEMERAL == 0
            && !self.entities.channels.contains_key(&message.id.cast())
            && self.thread_creation(message.channel_id, now_millis).public
    }

    // A message the window let go of since the form opened (a jump, a trim) is Discord's to judge.
    pub(crate) fn may_start_thread(
        &self,
        channel: ChannelId,
        message: MessageId,
        now_millis: i64,
    ) -> bool {
        if self.message(channel, message).is_some() {
            return self.can_start_thread(channel, message, now_millis);
        }
        self.thread_creation(channel, now_millis).public
            && !self.entities.channels.contains_key(&message.cast())
    }

    pub(crate) fn thread_slowmode(
        &self,
        parent: ChannelId,
        now_millis: i64,
        now: Instant,
        wall: SystemTime,
    ) -> Option<Slowmode> {
        let (interval, exempt) = self.slowmode_parts(parent, now_millis)?;
        Some(Slowmode {
            interval,
            exempt,
            until: if exempt {
                None
            } else {
                self.thread_cooldowns.until(parent, now, wall)
            },
        })
    }

    pub(crate) fn start_thread_cooldown(&mut self, parent: ChannelId, now_millis: i64) {
        if let Some((interval, false)) = self.slowmode_parts(parent, now_millis) {
            self.thread_cooldowns
                .hold(parent, Instant::now(), interval + COOLDOWN_MARGIN);
        }
    }

    pub(crate) fn hold_thread_cooldown(
        &mut self,
        parent: ChannelId,
        wait: Duration,
        now_millis: i64,
    ) {
        if let Some((_, false)) = self.slowmode_parts(parent, now_millis) {
            self.thread_cooldowns
                .hold(parent, Instant::now(), wait + COOLDOWN_MARGIN);
        }
    }

    // Discord's answer to a create, as its THREAD_CREATE would bring it; a THREAD_CREATE that came
    // first is newer.
    pub(crate) fn keep_created_thread(
        &mut self,
        thread: model::Channel,
        events: &mut Vec<StoreEvent>,
    ) -> Option<ChannelId> {
        let id = thread.id;
        if !matches!(
            thread.kind,
            ChannelType::NewsThread | ChannelType::PublicThread | ChannelType::PrivateThread
        ) {
            return None;
        }
        let unjoined = self.entities.channels.get(&id).map(|known| {
            known
                .thread
                .as_deref()
                .is_some_and(|info| info.member.is_none())
        });
        match unjoined {
            None => self.put_channel(thread, events),
            // MESSAGE_UPDATE's `thread` and a new thread's THREAD_CREATE come without the member.
            Some(true) => {
                if let Some(member) = thread.member {
                    self.set_membership(id, Some(ThreadMembership::from_wire(member)), events);
                }
            }
            Some(false) => {}
        }
        let kept = self.entities.channels.contains_key(&id).then_some(id);
        if kept.is_some() {
            self.settle(self.now_millis(), events);
        }
        kept
    }

    // A created post's answer: the user's own new post, as its THREAD_CREATE with
    // `newly_created` would bring it, and its first message when Discord sent it along.
    pub(crate) fn keep_created_post(
        &mut self,
        post: model::Channel,
        first: Option<model::Message>,
        events: &mut Vec<StoreEvent>,
    ) -> (Option<ChannelId>, Vec<Ack>) {
        let (parent, owner) = (post.parent_id, post.owner_id);
        let known = self.entities.channels.contains_key(&post.id);
        let Some(id) = self.keep_created_thread(post, events) else {
            return (None, Vec::new());
        };
        if !known {
            self.note_post(parent, id);
            self.read_own_post(parent, id, owner);
            self.forum_post_created(id, parent, owner, true, events);
        }
        if let Some(first) = first.filter(|first| first.channel_id == id) {
            self.note_first_message(&first, events);
        }
        self.settle(self.now_millis(), events);
        (Some(id), self.take_reading_acks())
    }
}
