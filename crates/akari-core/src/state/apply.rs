use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::badges::{Badge, Badges, Dirty};
use super::events::StoreEvent;
use super::order::{self, guild_order};
use super::permissions::compute;
use super::read_states::{Ack, ReadStates};
use super::send::{Cooldowns, DEFAULT_LENGTH_LIMIT, Slowmode, length_limit};
use super::settings::{
    GuildSettings, NotificationLevel, ServerListItem, UserSettings, server_list,
};
use super::threads::ThreadIndex;
use super::types::{Channel, CurrentUser, Guild, Member, Message, Role, User};
use super::windows::{
    CachedWindow, Cursor, LoadKind, LoadTicket, MessageWindow, WindowLimits, Windows,
};
use crate::gateway::{
    AvailableGuild, ChannelDelete, ChannelUpdate, DispatchEvent, GatewayGuild, GuildDelete,
    GuildMemberUpdate, GuildRoleDelete, GuildRoleEvent, GuildUpdate, Ready, UserUpdate,
};
use crate::model::{self, ChannelId, GuildId, MessageId, Permissions, Timestamp, UserId};

#[derive(Default, Clone)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct Entities {
    pub(crate) current_user: Option<Arc<CurrentUser>>,
    // DM and group DM recipients only.
    pub(crate) users: HashMap<UserId, Arc<User>>,
    pub(crate) guilds: HashMap<GuildId, Arc<Guild>>,
    pub(crate) unavailable: HashSet<GuildId>,
    // Only the current user's member in each guild.
    pub(crate) members: HashMap<GuildId, Arc<Member>>,
    pub(crate) channels: HashMap<ChannelId, Arc<Channel>>,
    pub(crate) settings: UserSettings,
    pub(crate) read_states: ReadStates,
}

impl Entities {
    pub(crate) fn from_ready(ready: Ready) -> Self {
        let current = CurrentUser::from_wire(ready.user);
        let me = current.user.id;
        let mut entities = Self {
            current_user: Some(Arc::new(current)),
            settings: UserSettings::from_ready(
                ready.user_guild_settings,
                ready.user_settings_proto,
                ready.notification_settings,
            ),
            read_states: ReadStates::from_ready(ready.read_state, ready.relationships),
            ..Self::default()
        };
        let mut recipients = HashSet::new();
        for channel in ready.private_channels {
            let (channel, users) = split_recipients(channel);
            recipients.extend(channel.recipient_ids.iter().copied());
            entities.insert_users(users);
            entities.insert_channel(Channel::from_wire(channel, None));
        }
        entities.insert_users(
            ready
                .users
                .into_iter()
                .filter(|user| recipients.contains(&user.id)),
        );
        let mut merged_members = ready.merged_members.into_iter();
        for guild in ready.guilds {
            let members = merged_members.next().unwrap_or_default();
            match guild {
                GatewayGuild::Available(guild) => entities.insert_guild(*guild, members, Some(me)),
                GatewayGuild::Unavailable(guild) => {
                    entities.unavailable.insert(guild.id);
                }
            }
        }
        entities
    }

    pub(crate) fn from_guild(guild: AvailableGuild, me: Option<UserId>) -> Self {
        let mut next = Self::default();
        next.insert_guild(guild, Vec::new(), me);
        next
    }

    fn me(&self) -> Option<UserId> {
        self.current_user.as_ref().map(|current| current.user.id)
    }

    fn server_list(&self) -> Vec<ServerListItem> {
        let by_join = guild_order(
            self.guilds
                .values()
                .map(|guild| {
                    let joined = self
                        .members
                        .get(&guild.id)
                        .and_then(|member| member.joined_at);
                    (guild.clone(), joined)
                })
                .collect(),
        );
        server_list(
            self.settings.folders.as_deref(),
            by_join.iter().map(|guild| guild.id).collect(),
        )
    }

    fn insert_users(&mut self, users: impl IntoIterator<Item = model::User>) {
        for user in users {
            self.users.insert(user.id, Arc::new(User::from_wire(user)));
        }
    }

    fn insert_channel(&mut self, channel: Channel) {
        self.channels.insert(channel.id, Arc::new(channel));
    }

    fn insert_guild(
        &mut self,
        mut guild: AvailableGuild,
        mut members: Vec<model::GuildMember>,
        me: Option<UserId>,
    ) {
        let channels = std::mem::take(&mut guild.channels);
        let threads = std::mem::take(&mut guild.threads);
        members.append(&mut guild.members);
        let guild = Guild::from_wire(guild);
        let id = guild.id;
        for channel in channels {
            self.insert_channel(Channel::from_wire(channel, Some(id)));
        }
        // User accounts are only sent the threads they joined.
        for thread in threads {
            let mut thread = Channel::from_wire(thread, Some(id));
            if let Some(info) = thread.thread.as_deref_mut() {
                info.joined = true;
            }
            self.insert_channel(thread);
        }
        let own = members
            .into_iter()
            .find(|member| me.is_some() && member_user(member) == me);
        if let Some(member) = own {
            self.members
                .insert(id, Arc::new(Member::from_wire(id, member)));
        }
        self.unavailable.remove(&id);
        self.guilds.insert(id, Arc::new(guild));
    }

    pub(crate) fn channel_muted_at(&self, channel: ChannelId, now: Timestamp) -> bool {
        let Some(stored) = self.channels.get(&channel) else {
            return false;
        };
        self.settings
            .guild(stored.guild_id)
            .and_then(|settings| settings.channels.get(&channel))
            .is_some_and(|own| own.muted_now(now))
    }

    // A thread inherits from its channel, which inherits from its category.
    pub(crate) fn notification_level(&self, channel: ChannelId) -> NotificationLevel {
        let guild = self
            .channels
            .get(&channel)
            .and_then(|stored| stored.guild_id);
        let settings = self.settings.guild(guild);
        let mut next = Some(channel);
        for _ in 0..3 {
            let Some(id) = next else {
                break;
            };
            if let Some(level) = settings
                .and_then(|settings| settings.channels.get(&id))
                .and_then(|own| own.level)
            {
                return level;
            }
            next = self.channels.get(&id).and_then(|stored| stored.parent_id);
        }
        settings
            .and_then(|settings| settings.level)
            .or_else(|| {
                guild
                    .and_then(|id| self.guilds.get(&id))
                    .map(|guild| guild.default_message_notifications)
            })
            .unwrap_or(NotificationLevel::AllMessages)
    }

    pub(crate) fn permissions(&self, channel: ChannelId, now_millis: i64) -> Option<Permissions> {
        let channel = self.channels.get(&channel)?;
        let guild_id = channel.guild_id?;
        let guild = self.guilds.get(&guild_id)?;
        let member = self.members.get(&guild_id)?;
        let me = self.me()?;
        let parent = if channel.is_thread() {
            Some(&**self.channels.get(&channel.parent_id?)?)
        } else {
            None
        };
        Some(compute(guild, me, member, channel, parent, now_millis))
    }

    fn guild_channels(&self, guild: GuildId) -> HashMap<ChannelId, Arc<Channel>> {
        self.channels
            .iter()
            .filter(|(_, channel)| channel.guild_id == Some(guild))
            .map(|(id, channel)| (*id, channel.clone()))
            .collect()
    }
}

fn member_user(member: &model::GuildMember) -> Option<UserId> {
    member.user.as_ref().map(|user| user.id).or(member.user_id)
}

fn split_recipients(mut channel: model::Channel) -> (model::Channel, Vec<model::User>) {
    let users = std::mem::take(&mut channel.recipients);
    if channel.recipient_ids.is_empty() {
        channel.recipient_ids = users.iter().map(|user| user.id).collect();
    }
    (channel, users)
}

enum Change<'a, T> {
    Added(&'a Arc<T>),
    Updated {
        before: &'a Arc<T>,
        after: &'a Arc<T>,
    },
    Removed(&'a Arc<T>),
}

// Sorted by key, so a diff's events come in a stable order.
fn changes<'a, K: Copy + Ord + Hash, T: PartialEq>(
    old: &'a HashMap<K, Arc<T>>,
    new: &'a HashMap<K, Arc<T>>,
) -> Vec<Change<'a, T>> {
    let keys: BTreeSet<K> = old.keys().chain(new.keys()).copied().collect();
    keys.into_iter()
        .filter_map(|key| match (old.get(&key), new.get(&key)) {
            (None, Some(added)) => Some(Change::Added(added)),
            (Some(before), Some(after)) if before != after => {
                Some(Change::Updated { before, after })
            }
            (Some(removed), None) => Some(Change::Removed(removed)),
            _ => None,
        })
        .collect()
}

fn push_channel_changes(
    old: &HashMap<ChannelId, Arc<Channel>>,
    new: &HashMap<ChannelId, Arc<Channel>>,
    skip: impl Fn(&Channel) -> bool,
    events: &mut Vec<StoreEvent>,
) {
    for change in changes(old, new) {
        match change {
            Change::Added(channel) if !skip(channel) => {
                events.push(StoreEvent::ChannelAdded(channel.clone()));
            }
            // A guild channel's newest message alone changes without an event, as it does live.
            Change::Updated { before, after }
                if !skip(after)
                    && !(after.guild_id.is_some() && before.same_apart_from_activity(after)) =>
            {
                events.push(StoreEvent::ChannelUpdated(after.clone()));
            }
            Change::Removed(channel) if !skip(channel) => events.push(StoreEvent::ChannelRemoved {
                channel_id: channel.id,
                guild_id: channel.guild_id,
            }),
            _ => {}
        }
    }
}

/// What the store derives from its entities, built before a READY or a restore takes the lock.
pub(crate) struct Derived {
    badges: Badges,
    threads: ThreadIndex,
}

impl Derived {
    pub(crate) fn build(entities: &Entities, now_millis: i64) -> Self {
        Self {
            badges: Badges::build(entities, now_millis),
            threads: ThreadIndex::build(entities),
        }
    }
}

pub(crate) struct ReadyDiff {
    events: Vec<StoreEvent>,
    removed_channels: Vec<ChannelId>,
}

#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct State {
    entities: Entities,
    badges: Badges,
    threads: ThreadIndex,
    // What the current write changed that can move a badge.
    dirty: Dirty,
    ready: bool,
    restored: bool,
    windows: Windows,
    cooldowns: Cooldowns,
    // The channel the user is reading, with the newest row its list showed after the last write.
    reading: Option<(ChannelId, Option<MessageId>)>,
    // Its local ack from the latest write, for the account to send.
    due: Option<Ack>,
    // Channels whose cursor came from an unread update and may be behind Discord's.
    stale_cursors: HashSet<ChannelId>,
}

impl State {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_limits(super::windows::DEFAULT_LIMITS)
    }

    pub(crate) fn with_limits(limits: WindowLimits) -> Self {
        Self {
            entities: Entities::default(),
            badges: Badges::default(),
            threads: ThreadIndex::default(),
            dirty: Dirty::default(),
            ready: false,
            restored: false,
            windows: Windows::new(limits),
            cooldowns: Cooldowns::default(),
            reading: None,
            due: None,
            stale_cursors: HashSet::new(),
        }
    }

    pub(crate) fn replace(&mut self, mut next: Entities, events: &mut Vec<StoreEvent>) {
        self.keep_folders(&mut next);
        let derived = Derived::build(&next, wall_millis());
        let diff = self.ready_diff(&next, &derived);
        self.swap(next, derived, diff, events);
    }

    // A READY whose settings proto didn't decode keeps the folders the store had.
    pub(crate) fn keep_folders(&self, next: &mut Entities) {
        if std::mem::take(&mut next.settings.unreadable_folders) {
            next.settings.folders = self.entities.settings.folders.clone();
        }
    }

    // `None` before the first READY or a restore, which leave nothing to diff against.
    pub(crate) fn ready_diff(&self, next: &Entities, derived: &Derived) -> Option<ReadyDiff> {
        if !self.ready && !self.restored {
            return None;
        }
        let mut events = Vec::new();
        self.diff(next, &mut events);
        self.badges.diff(&derived.badges, &mut events);
        let removed_channels = self
            .entities
            .channels
            .keys()
            .filter(|channel| !next.channels.contains_key(channel))
            .copied()
            .collect();
        Some(ReadyDiff {
            events,
            removed_channels,
        })
    }

    // Returns the old state, so the caller can drop it after releasing the lock.
    pub(crate) fn swap(
        &mut self,
        next: Entities,
        derived: Derived,
        diff: Option<ReadyDiff>,
        events: &mut Vec<StoreEvent>,
    ) -> (Entities, Derived) {
        if let Some(diff) = diff {
            events.extend(diff.events);
            for channel in diff.removed_channels {
                self.windows.drop_channel(channel);
            }
            // Pages loaded since a restore predate this session too: what happened in between
            // never comes as a dispatch.
            self.windows.mark_stale(events);
        }
        self.ready = true;
        events.push(StoreEvent::Ready);
        self.dirty = Dirty::default();
        let old = Derived {
            badges: std::mem::replace(&mut self.badges, derived.badges),
            threads: std::mem::replace(&mut self.threads, derived.threads),
        };
        let old = (std::mem::replace(&mut self.entities, next), old);
        // READY's cursors are fresh.
        self.stale_cursors.clear();
        self.read_again();
        self.refresh_badges(wall_millis(), events);
        old
    }

    pub(crate) fn me(&self) -> Option<UserId> {
        self.entities.me()
    }

    pub(crate) fn had_ready(&self) -> bool {
        self.ready
    }

    // Only before the first READY: a READY that came first is fresher than any cache.
    pub(crate) fn restore(
        &mut self,
        cached: Entities,
        derived: Derived,
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        if self.ready || self.restored {
            return false;
        }
        self.entities = cached;
        self.badges = derived.badges;
        self.threads = derived.threads;
        self.restored = true;
        events.push(StoreEvent::Restored);
        true
    }

    fn diff(&self, next: &Entities, events: &mut Vec<StoreEvent>) {
        #[cfg(test)]
        tests::run_diff_hook();
        let old = &self.entities;
        if let Some(current) = &next.current_user
            && old.current_user.as_ref() != Some(current)
        {
            events.push(StoreEvent::CurrentUserUpdated(current.clone()));
        }
        for change in changes(&old.users, &next.users) {
            if let Change::Updated { after: user, .. } = change {
                events.push(StoreEvent::UserUpdated(user.clone()));
            }
        }

        let ids: BTreeSet<GuildId> = [&old.guilds, &next.guilds]
            .into_iter()
            .flat_map(HashMap::keys)
            .chain(old.unavailable.iter().chain(&next.unavailable))
            .copied()
            .collect();
        let mut kept = HashSet::new();
        for id in ids {
            let was_down = old.unavailable.contains(&id);
            let is_down = next.unavailable.contains(&id);
            match (old.guilds.get(&id), next.guilds.get(&id)) {
                (Some(before), Some(after)) => {
                    kept.insert(id);
                    if before != after {
                        events.push(StoreEvent::GuildUpdated(after.clone()));
                    }
                }
                (None, Some(after)) => events.push(StoreEvent::GuildAdded(after.clone())),
                (Some(_), None) | (None, None) if is_down && !was_down => {
                    events.push(StoreEvent::GuildUnavailable { guild_id: id });
                }
                (Some(_), None) | (None, None) if !is_down => {
                    events.push(StoreEvent::GuildRemoved { guild_id: id });
                }
                _ => {}
            }
        }

        for change in changes(&old.members, &next.members) {
            if let Change::Added(member) | Change::Updated { after: member, .. } = change
                && kept.contains(&member.guild_id)
            {
                events.push(StoreEvent::CurrentMemberUpdated(member.clone()));
            }
        }
        // Channels of added, removed or unavailable guilds come with their guild's event.
        let implied = |channel: &Channel| channel.guild_id.is_some_and(|id| !kept.contains(&id));
        push_channel_changes(&old.channels, &next.channels, implied, events);

        if old.server_list() != next.server_list() {
            events.push(StoreEvent::ServerListChanged);
        }
        let settings: BTreeSet<Option<GuildId>> = [&old.settings.guilds, &next.settings.guilds]
            .into_iter()
            .flat_map(HashMap::keys)
            .copied()
            .collect();
        for guild_id in settings {
            if old.settings.guild(guild_id) != next.settings.guild(guild_id) {
                events.push(StoreEvent::GuildSettingsChanged { guild_id });
            }
        }
    }

    pub(crate) fn apply(&mut self, event: DispatchEvent, events: &mut Vec<StoreEvent>) {
        self.apply_event(event, events);
        self.settle(wall_millis(), events);
    }

    // The end of a write that can move a cursor: the channel being read stays read.
    fn settle(&mut self, now_millis: i64, events: &mut Vec<StoreEvent>) {
        self.follow_reading();
        self.refresh_badges(now_millis, events);
    }

    fn refresh_badges(&mut self, now_millis: i64, events: &mut Vec<StoreEvent>) {
        if self.dirty.is_empty() {
            return;
        }
        let dirty = std::mem::take(&mut self.dirty);
        self.badges
            .update(&self.entities, dirty, now_millis, events);
    }

    // The channel, and its threads, whose permissions follow its overwrites.
    fn mark_channel(&mut self, channel: &Channel) {
        self.dirty.channel(channel.id);
        for thread in self.threads.of(channel.id) {
            self.dirty.channel(thread);
        }
    }

    fn apply_event(&mut self, event: DispatchEvent, events: &mut Vec<StoreEvent>) {
        use DispatchEvent as E;

        match event {
            E::Ready(ready) => self.replace(Entities::from_ready(*ready), events),
            E::ReadySupplemental(supplemental) => {
                for channel in supplemental.lazy_private_channels {
                    self.put_channel(channel, events);
                }
            }
            E::GuildCreate(guild) => match *guild {
                GatewayGuild::Available(guild) => self.guild_create(*guild, events),
                GatewayGuild::Unavailable(guild) => {
                    self.tracking_server_list(events, |state, events| {
                        state.guild_down(guild.id, events);
                    });
                }
            },
            E::GuildUpdate(update) => self.guild_update(*update, events),
            E::GuildDelete(delete) => {
                self.tracking_server_list(events, |state, events| {
                    state.guild_delete(delete, events);
                });
            }
            E::GuildRoleCreate(event) | E::GuildRoleUpdate(event) => self.put_role(*event, events),
            E::GuildRoleDelete(delete) => self.delete_role(delete, events),
            E::GuildMemberUpdate(update) => self.member_update(*update, events),
            E::ChannelCreate(channel) => self.put_channel(*channel, events),
            E::ThreadCreate(thread) => {
                let (id, parent) = (thread.id, thread.parent_id);
                self.put_channel(*thread, events);
                self.note_post(parent, id);
            }
            E::ChannelUpdate(update) | E::ThreadUpdate(update) => {
                self.channel_update(*update, events);
            }
            E::ChannelDelete(delete) | E::ThreadDelete(delete) => {
                self.channel_delete(delete, events);
            }
            E::UserUpdate(update) => self.user_update(*update, events),
            E::MessageCreate(message) => {
                let (channel, id, author) = (message.channel_id, message.id, message.author.id);
                let nonce = message
                    .nonce
                    .as_ref()
                    .and_then(model::Nonce::as_u64)
                    .map(MessageId::new);
                self.read_message(&message);
                self.dirty.channel(channel);
                let forced = self.forced_through(&message);
                self.windows.live(*message, &self.entities.users, events);
                if self.note_message(channel, id, events) && !forced {
                    self.stale_cursors.remove(&channel);
                }
                if Some(author) == self.entities.me() {
                    self.note_own_message(channel, nonce);
                }
            }
            E::MessageUpdate(update) => self.windows.update(*update, &self.entities.users, events),
            E::MessageDelete(delete) => self.windows.delete(delete.channel_id, delete.id, events),
            E::MessageDeleteBulk(delete) => {
                for id in delete.ids {
                    self.windows.delete(delete.channel_id, id, events);
                }
            }
            E::UserGuildSettingsUpdate(settings) => {
                let guild_id = settings.guild_id;
                if self.entities.settings.put_guild(*settings) {
                    self.dirty.guild(guild_id);
                    events.push(StoreEvent::GuildSettingsChanged { guild_id });
                }
            }
            E::UserSettingsProtoUpdate(update) => {
                self.tracking_server_list(events, |state, _| {
                    state.entities.settings.apply_proto(*update);
                });
            }
            E::NotificationSettingsUpdate(settings) => {
                self.set_notification_flags(settings.flags, None, events);
            }
            E::MessageAck(ack) => self.apply_ack(&ack),
            E::ChannelUnreadUpdate(update) => {
                self.note_unreads(update.guild_id, &update.channel_unread_updates);
            }
            E::PassiveUpdateV2(update) => {
                self.note_unreads(update.guild_id, &update.updated_channels);
            }
            E::RelationshipAdd(relationship) => self.entities.read_states.relate(relationship),
            E::RelationshipUpdate(update) => {
                self.entities.read_states.update_relationship(update);
            }
            E::RelationshipRemove(removed) => self.entities.read_states.unrelate(removed.id),
            E::Resumed | E::Other(_) => {}
        }
    }

    fn tracking_server_list(
        &mut self,
        events: &mut Vec<StoreEvent>,
        change: impl FnOnce(&mut Self, &mut Vec<StoreEvent>),
    ) {
        let before = self.entities.server_list();
        change(self, events);
        if self.entities.server_list() != before {
            events.push(StoreEvent::ServerListChanged);
        }
    }

    fn put_user(&mut self, user: model::User, events: &mut Vec<StoreEvent>) {
        let user = User::from_wire(user);
        match self.entities.users.get(&user.id) {
            Some(known) if **known == user => {}
            Some(_) => {
                let user = Arc::new(user);
                self.entities.users.insert(user.id, user.clone());
                events.push(StoreEvent::UserUpdated(user));
            }
            None => {
                self.entities.users.insert(user.id, Arc::new(user));
            }
        }
    }

    fn put_channel(&mut self, channel: model::Channel, events: &mut Vec<StoreEvent>) {
        if let Some(guild) = channel.guild_id
            && !self.entities.guilds.contains_key(&guild)
        {
            tracing::debug!(
                channel_id = channel.id.get(),
                guild_id = guild.get(),
                "skipping a channel in an unknown guild"
            );
            return;
        }
        let (channel, users) = split_recipients(channel);
        for user in users {
            self.put_user(user, events);
        }
        let mut channel = Channel::from_wire(channel, None);
        if let Some(known) = self.entities.channels.get(&channel.id) {
            if let Some(last) = known.last_message_id {
                channel.note_message(last);
            }
            if let (Some(info), Some(before)) = (channel.thread.as_deref_mut(), &known.thread) {
                info.joined |= before.joined;
            }
        }
        let channel = Arc::new(channel);
        self.mark_channel(&channel);
        self.threads.add(&channel);
        match self.entities.channels.get(&channel.id) {
            Some(known) if *known == channel => {}
            Some(_) => {
                self.entities.channels.insert(channel.id, channel.clone());
                events.push(StoreEvent::ChannelUpdated(channel));
            }
            None => {
                self.entities.channels.insert(channel.id, channel.clone());
                events.push(StoreEvent::ChannelAdded(channel));
            }
        }
    }

    // DM lists sort by the newest message; guild channels change silently, or every message
    // in a busy server would be an event.
    fn note_message(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        let Some(stored) = self.entities.channels.get_mut(&channel) else {
            return false;
        };
        let moved = Arc::make_mut(stored).note_message(id.cast());
        if moved && stored.guild_id.is_none() {
            events.push(StoreEvent::ChannelUpdated(stored.clone()));
        }
        moved
    }

    // Discord enforces slowmode per user, so a message from another device starts it too.
    fn note_own_message(&mut self, channel: ChannelId, nonce: Option<MessageId>) {
        if let Some(interval) = self.cooldown_interval(channel, wall_millis()) {
            self.cooldowns
                .seen(channel, nonce, Instant::now(), interval);
        }
    }

    fn cooldown_interval(&self, channel: ChannelId, now_millis: i64) -> Option<Duration> {
        let slowmode = self.slowmode_parts(channel, now_millis)?;
        (!slowmode.1).then_some(slowmode.0)
    }

    fn slowmode_parts(&self, channel: ChannelId, now_millis: i64) -> Option<(Duration, bool)> {
        let stored = self.entities.channels.get(&channel)?;
        stored.guild_id?;
        if stored.rate_limit_per_user == 0 {
            return None;
        }
        let exempt = self
            .permissions(channel, now_millis)
            .is_some_and(|permissions| permissions.contains(Permissions::BYPASS_SLOWMODE));
        Some((
            Duration::from_secs(u64::from(stored.rate_limit_per_user)),
            exempt,
        ))
    }

    fn channel_update(&mut self, update: ChannelUpdate, events: &mut Vec<StoreEvent>) {
        let Some(known) = self.entities.channels.get(&update.id).cloned() else {
            tracing::debug!(
                channel_id = update.id.get(),
                "skipping an update for an unknown channel"
            );
            return;
        };
        for user in update.recipients.clone().unwrap_or_default() {
            self.put_user(user, events);
        }
        let next = Arc::new(known.patch(update));
        if next != known {
            self.mark_channel(&next);
            self.threads.remove(&known);
            self.threads.add(&next);
            self.entities.channels.insert(next.id, next.clone());
            events.push(StoreEvent::ChannelUpdated(next));
        }
    }

    fn channel_delete(&mut self, delete: ChannelDelete, events: &mut Vec<StoreEvent>) {
        let Some(removed) = self.entities.channels.remove(&delete.id) else {
            tracing::debug!(
                channel_id = delete.id.get(),
                "skipping the deletion of an unknown channel"
            );
            return;
        };
        self.windows.drop_channel(removed.id);
        self.entities.read_states.forget(removed.id);
        self.dirty.channel(removed.id);
        events.push(StoreEvent::ChannelRemoved {
            channel_id: removed.id,
            guild_id: removed.guild_id,
        });
        let threads: Vec<ChannelId> = self.threads.of(removed.id).collect();
        self.threads.remove(&removed);
        for thread in threads {
            if let Some(gone) = self.entities.channels.remove(&thread) {
                self.threads.remove(&gone);
            }
            self.windows.drop_channel(thread);
            self.entities.read_states.forget(thread);
            self.dirty.channel(thread);
            events.push(StoreEvent::ChannelRemoved {
                channel_id: thread,
                guild_id: removed.guild_id,
            });
        }
    }

    fn guild_create(&mut self, guild: AvailableGuild, events: &mut Vec<StoreEvent>) {
        let next = Entities::from_guild(guild, self.entities.me());
        self.add_guild(next, events);
    }

    pub(crate) fn add_guild(&mut self, next: Entities, events: &mut Vec<StoreEvent>) {
        self.tracking_server_list(events, |state, events| state.merge_guild(next, events));
        self.settle(wall_millis(), events);
    }

    fn merge_guild(&mut self, mut next: Entities, events: &mut Vec<StoreEvent>) {
        let Some((&id, after)) = next.guilds.iter().next() else {
            return;
        };
        for (channel, merged) in &mut next.channels {
            if let Some(last) = self
                .entities
                .channels
                .get(channel)
                .and_then(|known| known.last_message_id)
            {
                Arc::make_mut(merged).note_message(last);
            }
        }
        self.dirty.guild(Some(id));
        let after = after.clone();
        match self.entities.guilds.get(&id).cloned() {
            Some(before) => {
                if before != after {
                    events.push(StoreEvent::GuildUpdated(after));
                }
                if let Some(member) = next.members.get(&id)
                    && self.entities.members.get(&id) != Some(member)
                {
                    events.push(StoreEvent::CurrentMemberUpdated(member.clone()));
                }
                let old = self.entities.guild_channels(id);
                push_channel_changes(&old, &next.channels, |_| false, events);
                for (id, channel) in old.iter().filter(|(id, _)| !next.channels.contains_key(id)) {
                    self.entities.channels.remove(id);
                    self.threads.remove(channel);
                    self.entities.read_states.forget(*id);
                    self.windows.drop_channel(*id);
                }
            }
            None => events.push(StoreEvent::GuildAdded(after)),
        }
        self.entities.unavailable.remove(&id);
        self.entities.guilds.extend(next.guilds);
        self.entities.members.extend(next.members);
        for channel in next.channels.values() {
            self.threads.add(channel);
        }
        self.entities.channels.extend(next.channels);
    }

    fn guild_update(&mut self, update: GuildUpdate, events: &mut Vec<StoreEvent>) {
        let Some(known) = self.entities.guilds.get(&update.id).cloned() else {
            tracing::debug!(
                guild_id = update.id.get(),
                "skipping an update for an unknown guild"
            );
            return;
        };
        self.put_guild(known.patch(update), &known, events);
    }

    fn put_guild(&mut self, next: Guild, known: &Arc<Guild>, events: &mut Vec<StoreEvent>) {
        if next != **known {
            self.dirty.guild(Some(next.id));
            let next = Arc::new(next);
            self.entities.guilds.insert(next.id, next.clone());
            events.push(StoreEvent::GuildUpdated(next));
        }
    }

    fn guild_delete(&mut self, delete: GuildDelete, events: &mut Vec<StoreEvent>) {
        if delete.unavailable {
            self.guild_down(delete.id, events);
        } else if self.entities.guilds.contains_key(&delete.id)
            || self.entities.unavailable.contains(&delete.id)
        {
            let channels: Vec<ChannelId> = self
                .entities
                .channels
                .values()
                .filter(|channel| channel.guild_id == Some(delete.id))
                .map(|channel| channel.id)
                .collect();
            for channel in channels {
                self.entities.read_states.forget(channel);
            }
            self.remove_guild_data(delete.id);
            self.entities.unavailable.remove(&delete.id);
            events.push(StoreEvent::GuildRemoved {
                guild_id: delete.id,
            });
        } else {
            tracing::debug!(
                guild_id = delete.id.get(),
                "skipping the deletion of an unknown guild"
            );
        }
    }

    fn guild_down(&mut self, id: GuildId, events: &mut Vec<StoreEvent>) {
        if self.entities.unavailable.insert(id) {
            self.remove_guild_data(id);
            events.push(StoreEvent::GuildUnavailable { guild_id: id });
        }
    }

    fn remove_guild_data(&mut self, id: GuildId) {
        self.dirty.guild(Some(id));
        self.entities.guilds.remove(&id);
        self.entities.members.remove(&id);
        let windows = &mut self.windows;
        let threads = &mut self.threads;
        self.entities.channels.retain(|channel_id, channel| {
            let keep = channel.guild_id != Some(id);
            if !keep {
                windows.drop_channel(*channel_id);
                threads.remove(channel);
            }
            keep
        });
    }

    fn put_role(&mut self, event: GuildRoleEvent, events: &mut Vec<StoreEvent>) {
        let Some(known) = self.entities.guilds.get(&event.guild_id).cloned() else {
            tracing::debug!(
                guild_id = event.guild_id.get(),
                "skipping a role in an unknown guild"
            );
            return;
        };
        let role = Role::from_wire(event.role);
        let mut roles = known.roles.to_vec();
        match roles.iter_mut().find(|existing| existing.id == role.id) {
            Some(existing) => *existing = role,
            None => roles.push(role),
        }
        let mut next = (*known).clone();
        next.roles = roles.into();
        self.put_guild(next, &known, events);
    }

    fn delete_role(&mut self, delete: GuildRoleDelete, events: &mut Vec<StoreEvent>) {
        let Some(known) = self.entities.guilds.get(&delete.guild_id).cloned() else {
            return;
        };
        if !known.roles.iter().any(|role| role.id == delete.role_id) {
            tracing::debug!(
                role_id = delete.role_id.get(),
                "skipping the deletion of an unknown role"
            );
            return;
        }
        let mut next = (*known).clone();
        next.roles = known
            .roles
            .iter()
            .filter(|role| role.id != delete.role_id)
            .cloned()
            .collect();
        self.put_guild(next, &known, events);
    }

    fn member_update(&mut self, update: GuildMemberUpdate, events: &mut Vec<StoreEvent>) {
        let guild = update.guild_id;
        if self.entities.me() != Some(update.user.id) {
            return;
        }
        if !self.entities.guilds.contains_key(&guild) {
            tracing::debug!(
                guild_id = guild.get(),
                "skipping a member in an unknown guild"
            );
            return;
        }
        let known = self.entities.members.get(&guild).cloned();
        let base = known.as_deref().cloned().unwrap_or_else(|| Member {
            guild_id: guild,
            nick: None,
            avatar: None,
            roles: Box::new([]),
            joined_at: None,
            communication_disabled_until: None,
            flags: 0,
            pending: false,
        });
        let next = Arc::new(base.patch(update));
        if known.as_ref() != Some(&next) {
            self.dirty.guild(Some(guild));
            self.entities.members.insert(guild, next.clone());
            events.push(StoreEvent::CurrentMemberUpdated(next));
        }
    }

    fn user_update(&mut self, update: UserUpdate, events: &mut Vec<StoreEvent>) {
        let Some(known) = self.entities.current_user.clone() else {
            return;
        };
        if known.user.id != update.id {
            tracing::debug!(
                user_id = update.id.get(),
                "skipping an update for another user"
            );
            return;
        }
        let next = Arc::new(known.patch(update));
        if next != known {
            self.entities.current_user = Some(next.clone());
            events.push(StoreEvent::CurrentUserUpdated(next));
        }
    }

    pub(crate) fn view_channel(&mut self, channel: ChannelId, events: &mut Vec<StoreEvent>) {
        self.windows.view(channel, events);
    }

    pub(crate) fn begin_load(
        &mut self,
        channel: ChannelId,
        kind: LoadKind,
        events: &mut Vec<StoreEvent>,
    ) -> Option<LoadTicket> {
        self.windows.begin_load(channel, kind, events)
    }

    pub(crate) fn finish_load(
        &mut self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        reached_end: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        let present =
            ticket.cursor == Cursor::Latest || (ticket.kind == LoadKind::Newer && reached_end);
        let newest = page
            .iter()
            .map(|message| message.id)
            .max()
            .filter(|_| present);
        self.windows
            .finish_load(ticket, page, &self.entities.users, reached_end, events);
        if let Some(newest) = newest {
            self.reach_present(ticket.channel, newest, events);
        }
        self.settle(wall_millis(), events);
    }

    pub(crate) fn abort_load(&mut self, ticket: LoadTicket, events: &mut Vec<StoreEvent>) {
        self.windows.abort_load(ticket, events);
    }

    pub(crate) fn fill_from_cache(
        &mut self,
        ticket: LoadTicket,
        messages: Vec<Arc<Message>>,
        events: &mut Vec<StoreEvent>,
    ) {
        self.windows.fill_from_cache(ticket, messages, events);
    }

    pub(crate) fn stale_channels(&self) -> Vec<ChannelId> {
        self.windows.stale_channels()
    }

    pub(crate) fn cache_parts(
        &self,
        per_window: usize,
    ) -> (Entities, Vec<(ChannelId, CachedWindow)>) {
        // Before a session nothing confirms a window is still current.
        let windows = if self.ready {
            self.windows.cache_windows(per_window)
        } else {
            Vec::new()
        };
        (self.entities.clone(), windows)
    }

    pub(crate) fn viewed_channels(&self) -> Vec<Arc<Channel>> {
        self.windows
            .channels()
            .filter_map(|channel| self.entities.channels.get(&channel).cloned())
            .collect()
    }

    // The cooldown starts in the same write, so a UI that reads slowmode on the pending
    // message's event sees it.
    pub(crate) fn queue(
        &mut self,
        channel: ChannelId,
        message: Arc<Message>,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) {
        let send = message.id;
        self.windows.queue(channel, message, events);
        self.start_cooldown(channel, send, now_millis);
    }

    pub(crate) fn confirm(
        &mut self,
        channel: ChannelId,
        pending: MessageId,
        message: model::Message,
        events: &mut Vec<StoreEvent>,
    ) {
        self.windows
            .confirm(channel, pending, message, &self.entities.users, events);
    }

    pub(crate) fn fail(
        &mut self,
        channel: ChannelId,
        pending: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        self.windows.fail(channel, pending, events);
    }

    pub(crate) fn retry(
        &mut self,
        channel: ChannelId,
        pending: MessageId,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Arc<Message>> {
        let message = self.windows.retry(channel, pending, events)?;
        self.start_cooldown(channel, pending, now_millis);
        Some(message)
    }

    pub(crate) fn discard(
        &mut self,
        channel: ChannelId,
        pending: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        self.windows.discard(channel, pending, events);
    }

    pub(crate) fn messages(&self, channel: ChannelId) -> Option<MessageWindow> {
        self.windows.snapshot(channel)
    }

    pub(crate) fn pending(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.windows.pending(channel, id)
    }

    pub(crate) fn message(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.windows.message(channel, id)
    }

    pub(crate) fn current_user(&self) -> Option<Arc<CurrentUser>> {
        self.entities.current_user.clone()
    }

    pub(crate) fn user(&self, id: UserId) -> Option<Arc<User>> {
        self.entities.users.get(&id).cloned()
    }

    pub(crate) fn guilds(&self) -> Vec<Arc<Guild>> {
        self.entities.guilds.values().cloned().collect()
    }

    pub(crate) fn guild(&self, id: GuildId) -> Option<Arc<Guild>> {
        self.entities.guilds.get(&id).cloned()
    }

    pub(crate) fn unavailable_guilds(&self) -> Vec<GuildId> {
        let mut ids: Vec<GuildId> = self.entities.unavailable.iter().copied().collect();
        ids.sort_unstable();
        ids
    }

    pub(crate) fn current_member(&self, guild: GuildId) -> Option<Arc<Member>> {
        self.entities.members.get(&guild).cloned()
    }

    pub(crate) fn server_list(&self) -> Vec<ServerListItem> {
        self.entities.server_list()
    }

    pub(crate) fn guild_settings(&self, guild: Option<GuildId>) -> Arc<GuildSettings> {
        self.entities
            .settings
            .guild(guild)
            .cloned()
            .unwrap_or_default()
    }

    pub(crate) fn channel_muted_at(&self, channel: ChannelId, now: Timestamp) -> bool {
        self.entities.channel_muted_at(channel, now)
    }

    pub(crate) fn notification_level(&self, channel: ChannelId) -> NotificationLevel {
        self.entities.notification_level(channel)
    }

    /// A copy of the entities with `flags`, or `None` when the flags are already these.
    pub(crate) fn with_notification_flags(&self, flags: u64) -> Option<Entities> {
        (self.entities.settings.notification_flags != flags).then(|| {
            let mut next = self.entities.clone();
            next.settings.notification_flags = flags;
            next
        })
    }

    // `badges` were built for the new flags; without them they are rebuilt here.
    pub(crate) fn set_notification_flags(
        &mut self,
        flags: u64,
        badges: Option<Badges>,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.entities.settings.notification_flags == flags {
            return;
        }
        self.entities.settings.notification_flags = flags;
        match badges {
            Some(badges) => {
                self.badges.diff(&badges, events);
                self.badges = badges;
            }
            None => {
                self.dirty.all();
                self.refresh_badges(wall_millis(), events);
            }
        }
    }

    pub(crate) fn end_mutes(
        &mut self,
        guilds: &[Option<GuildId>],
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) {
        for guild_id in guilds {
            self.dirty.guild(*guild_id);
            events.push(StoreEvent::GuildSettingsChanged {
                guild_id: *guild_id,
            });
        }
        self.refresh_badges(now_millis, events);
    }

    pub(crate) fn channel_badge(&self, channel: ChannelId) -> Badge {
        self.badges.channel(channel)
    }

    pub(crate) fn guild_badge(&self, guild: GuildId) -> Badge {
        self.badges.guild(guild)
    }

    pub(crate) fn unread_private_channels(&self) -> Vec<Arc<Channel>> {
        order::private_channel_order(self.channels_where(|channel| {
            channel.guild_id.is_none()
                && !channel.message_request
                && !channel.spam
                && self.badges.channel(channel.id).mentions > 0
        }))
    }

    pub(crate) fn next_mute_end(&self, after: Timestamp) -> Option<Timestamp> {
        self.entities
            .settings
            .mute_ends()
            .map(|(_, end)| end)
            .filter(|end| *end > after)
            .min()
    }

    pub(crate) fn mutes_ending(&self, after: Timestamp, until: Timestamp) -> Vec<Option<GuildId>> {
        let ended: BTreeSet<Option<GuildId>> = self
            .entities
            .settings
            .mute_ends()
            .filter(|(_, end)| *end > after && *end <= until)
            .map(|(guild, _)| guild)
            .collect();
        ended.into_iter().collect()
    }

    #[cfg(test)]
    pub(crate) fn settings(&self) -> &UserSettings {
        &self.entities.settings
    }

    pub(crate) fn channel(&self, id: ChannelId) -> Option<Arc<Channel>> {
        self.entities.channels.get(&id).cloned()
    }

    fn channels_where(&self, keep: impl Fn(&Channel) -> bool) -> Vec<Arc<Channel>> {
        self.entities
            .channels
            .values()
            .filter(|channel| keep(channel))
            .cloned()
            .collect()
    }

    pub(crate) fn guild_channels(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.channels_where(|channel| channel.guild_id == Some(guild) && !channel.is_thread())
    }

    pub(crate) fn threads(&self, guild: GuildId) -> Vec<Arc<Channel>> {
        self.channels_where(|channel| channel.guild_id == Some(guild) && channel.is_thread())
    }

    pub(crate) fn private_channels(&self) -> Vec<Arc<Channel>> {
        self.channels_where(|channel| channel.guild_id.is_none())
    }

    pub(crate) fn slowmode(
        &self,
        channel: ChannelId,
        now_millis: i64,
        now: Instant,
        wall: SystemTime,
    ) -> Option<Slowmode> {
        let (interval, exempt) = self.slowmode_parts(channel, now_millis)?;
        Some(Slowmode {
            interval,
            exempt,
            until: if exempt {
                None
            } else {
                self.cooldowns.until(channel, now, wall)
            },
        })
    }

    pub(crate) fn length_limit(&self) -> usize {
        self.entities
            .current_user
            .as_ref()
            .map_or(DEFAULT_LENGTH_LIMIT, |me| length_limit(me.premium_type))
    }

    fn start_cooldown(&mut self, channel: ChannelId, send: MessageId, now_millis: i64) {
        if let Some(interval) = self.cooldown_interval(channel, now_millis) {
            self.cooldowns
                .start(channel, send, Instant::now(), interval);
        }
    }

    pub(crate) fn drop_cooldown(&mut self, channel: ChannelId, send: MessageId) {
        self.cooldowns.drop_started_by(channel, send);
    }

    pub(crate) fn hold_cooldown(&mut self, channel: ChannelId, wait: Duration, now_millis: i64) {
        if self.slowmode_parts(channel, now_millis).is_some() {
            self.cooldowns.hold(channel, Instant::now(), wait);
        }
    }

    pub(crate) fn permissions(&self, channel: ChannelId, now_millis: i64) -> Option<Permissions> {
        self.entities.permissions(channel, now_millis)
    }
}

pub(crate) fn wall_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

mod reading;

#[cfg(test)]
pub(crate) mod tests;
