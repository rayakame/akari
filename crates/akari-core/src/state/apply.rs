use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;

use super::events::StoreEvent;
use super::permissions::compute;
use super::types::{Channel, CurrentUser, Guild, Member, Message, Role, User};
use super::windows::{LoadKind, LoadTicket, MessageWindow, WindowLimits, Windows};
use crate::gateway::{
    AvailableGuild, ChannelDelete, ChannelUpdate, DispatchEvent, GatewayGuild, GuildDelete,
    GuildMemberUpdate, GuildRoleDelete, GuildRoleEvent, GuildUpdate, Ready, UserUpdate,
};
use crate::model::{self, ChannelId, GuildId, MessageId, Permissions, UserId};

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct Entities {
    current_user: Option<Arc<CurrentUser>>,
    // DM and group DM recipients only.
    users: HashMap<UserId, Arc<User>>,
    guilds: HashMap<GuildId, Arc<Guild>>,
    unavailable: HashSet<GuildId>,
    // Only the current user's member in each guild.
    members: HashMap<GuildId, Arc<Member>>,
    channels: HashMap<ChannelId, Arc<Channel>>,
}

impl Entities {
    pub(crate) fn from_ready(ready: Ready) -> Self {
        let current = CurrentUser::from_wire(ready.user);
        let me = current.user.id;
        let mut entities = Self {
            current_user: Some(Arc::new(current)),
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
        for channel in channels.into_iter().chain(threads) {
            self.insert_channel(Channel::from_wire(channel, Some(id)));
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

pub(crate) struct ReadyDiff {
    events: Vec<StoreEvent>,
    removed_channels: Vec<ChannelId>,
}

#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct State {
    entities: Entities,
    ready: bool,
    windows: Windows,
}

impl State {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_limits(super::windows::DEFAULT_LIMITS)
    }

    pub(crate) fn with_limits(limits: WindowLimits) -> Self {
        Self {
            entities: Entities::default(),
            ready: false,
            windows: Windows::new(limits),
        }
    }

    pub(crate) fn replace(&mut self, next: Entities, events: &mut Vec<StoreEvent>) {
        let diff = self.ready_diff(&next);
        self.swap(next, diff, events);
    }

    // `None` before the first READY, which has nothing to diff against.
    pub(crate) fn ready_diff(&self, next: &Entities) -> Option<ReadyDiff> {
        if !self.ready {
            return None;
        }
        let mut events = Vec::new();
        self.diff(next, &mut events);
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

    // Returns the old entities, so the caller can drop them after releasing the lock.
    pub(crate) fn swap(
        &mut self,
        next: Entities,
        diff: Option<ReadyDiff>,
        events: &mut Vec<StoreEvent>,
    ) -> Entities {
        if let Some(diff) = diff {
            events.extend(diff.events);
            for channel in diff.removed_channels {
                self.windows.drop_channel(channel);
            }
            self.windows.mark_stale(events);
        }
        self.ready = true;
        events.push(StoreEvent::Ready);
        std::mem::replace(&mut self.entities, next)
    }

    pub(crate) fn me(&self) -> Option<UserId> {
        self.entities.me()
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
    }

    pub(crate) fn apply(&mut self, event: DispatchEvent, events: &mut Vec<StoreEvent>) {
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
                GatewayGuild::Unavailable(guild) => self.guild_down(guild.id, events),
            },
            E::GuildUpdate(update) => self.guild_update(*update, events),
            E::GuildDelete(delete) => self.guild_delete(delete, events),
            E::GuildRoleCreate(event) | E::GuildRoleUpdate(event) => self.put_role(*event, events),
            E::GuildRoleDelete(delete) => self.delete_role(delete, events),
            E::GuildMemberUpdate(update) => self.member_update(*update, events),
            E::ChannelCreate(channel) | E::ThreadCreate(channel) => {
                self.put_channel(*channel, events);
            }
            E::ChannelUpdate(update) | E::ThreadUpdate(update) => {
                self.channel_update(*update, events);
            }
            E::ChannelDelete(delete) | E::ThreadDelete(delete) => {
                self.channel_delete(delete, events);
            }
            E::UserUpdate(update) => self.user_update(*update, events),
            E::MessageCreate(message) => {
                let (channel, id) = (message.channel_id, message.id);
                self.windows.live(*message, &self.entities.users, events);
                self.note_message(channel, id, events);
            }
            E::MessageUpdate(update) => self.windows.update(*update, &self.entities.users, events),
            E::MessageDelete(delete) => self.windows.delete(delete.channel_id, delete.id, events),
            E::MessageDeleteBulk(delete) => {
                for id in delete.ids {
                    self.windows.delete(delete.channel_id, id, events);
                }
            }
            E::Resumed | E::Other(_) => {}
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
        if let Some(last) = self
            .entities
            .channels
            .get(&channel.id)
            .and_then(|known| known.last_message_id)
        {
            channel.note_message(last);
        }
        let channel = Arc::new(channel);
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
    fn note_message(&mut self, channel: ChannelId, id: MessageId, events: &mut Vec<StoreEvent>) {
        let Some(stored) = self.entities.channels.get_mut(&channel) else {
            return;
        };
        if Arc::make_mut(stored).note_message(id.cast()) && stored.guild_id.is_none() {
            events.push(StoreEvent::ChannelUpdated(stored.clone()));
        }
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
        events.push(StoreEvent::ChannelRemoved {
            channel_id: removed.id,
            guild_id: removed.guild_id,
        });
        let threads: BTreeSet<ChannelId> = self
            .entities
            .channels
            .values()
            .filter(|channel| channel.is_thread() && channel.parent_id == Some(removed.id))
            .map(|channel| channel.id)
            .collect();
        for thread in threads {
            self.entities.channels.remove(&thread);
            self.windows.drop_channel(thread);
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
        let Some((&id, after)) = next.guilds.iter().next() else {
            return;
        };
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
                for channel in old.keys().filter(|id| !next.channels.contains_key(id)) {
                    self.entities.channels.remove(channel);
                    self.windows.drop_channel(*channel);
                }
            }
            None => events.push(StoreEvent::GuildAdded(after)),
        }
        self.entities.unavailable.remove(&id);
        self.entities.guilds.extend(next.guilds);
        self.entities.members.extend(next.members);
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
        self.entities.guilds.remove(&id);
        self.entities.members.remove(&id);
        let windows = &mut self.windows;
        self.entities.channels.retain(|channel_id, channel| {
            let keep = channel.guild_id != Some(id);
            if !keep {
                windows.drop_channel(*channel_id);
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
        self.windows
            .finish_load(ticket, page, &self.entities.users, reached_end, events);
    }

    pub(crate) fn abort_load(&mut self, ticket: LoadTicket, events: &mut Vec<StoreEvent>) {
        self.windows.abort_load(ticket, events);
    }

    pub(crate) fn stale_channels(&self) -> Vec<ChannelId> {
        self.windows.stale_channels()
    }

    pub(crate) fn viewed_channels(&self) -> Vec<Arc<Channel>> {
        self.windows
            .channels()
            .filter_map(|channel| self.entities.channels.get(&channel).cloned())
            .collect()
    }

    pub(crate) fn queue(
        &mut self,
        channel: ChannelId,
        message: Arc<Message>,
        events: &mut Vec<StoreEvent>,
    ) {
        self.windows.queue(channel, message, events);
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
        events: &mut Vec<StoreEvent>,
    ) -> Option<Arc<Message>> {
        self.windows.retry(channel, pending, events)
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

    pub(crate) fn permissions(&self, channel: ChannelId, now_millis: i64) -> Option<Permissions> {
        let channel = self.entities.channels.get(&channel)?;
        let guild_id = channel.guild_id?;
        let guild = self.entities.guilds.get(&guild_id)?;
        let member = self.entities.members.get(&guild_id)?;
        let me = self.entities.me()?;
        let parent = if channel.is_thread() {
            Some(&**self.entities.channels.get(&channel.parent_id?)?)
        } else {
            None
        };
        Some(compute(guild, me, member, channel, parent, now_millis))
    }
}

#[cfg(test)]
pub(crate) mod tests;
