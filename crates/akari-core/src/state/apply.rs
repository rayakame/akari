use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::hash::Hash;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::badges::{Badge, Badges, Dirty};
use super::clock::Clock;
use super::events::StoreEvent;
use super::order::{self, guild_order};
use super::permissions::compute;
use super::read_states::{Ack, ReadStates};
use super::send::{Cooldowns, DEFAULT_LENGTH_LIMIT, Slowmode, length_limit};
use super::settings::{
    GuildSettings, NotificationLevel, ServerListItem, UserSettings, server_list,
};
use super::threads::ThreadIndex;
use super::types::{
    Channel, CurrentUser, Guild, Member, Message, Role, StarterMessage, ThreadInfo,
    ThreadMembership, User,
};
use super::windows::{
    CachedWindow, Cursor, LoadKind, LoadTicket, MessageWindow, StarterFetch, WindowLimits, Windows,
};
use crate::gateway::{
    AvailableGuild, ChannelDelete, ChannelUpdate, DispatchEvent, GatewayGuild, GuildDelete,
    GuildMemberUpdate, GuildRoleDelete, GuildRoleEvent, GuildUpdate, Ready, ThreadListSync,
    ThreadMemberUpdate, ThreadMembersUpdate, UserUpdate,
};
use crate::load_report::PageOutcome;
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
    // Threads READY or GUILD_CREATE listed without the user's member, joined with a default one.
    pub(crate) memberless: HashSet<ChannelId>,
    // Threads the store kept across READY or GUILD_CREATE without their listing them.
    pub(crate) kept_threads: HashSet<ChannelId>,
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
            if let Some(info) = thread.thread.as_deref_mut()
                && info.member.is_none()
            {
                info.member = Some(ThreadMembership::default());
                self.memberless.insert(thread.id);
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

    // A thread's own mute is on its member; neither a muted channel nor a muted category mutes it.
    pub(crate) fn channel_muted_at(&self, channel: ChannelId, now: Timestamp) -> bool {
        let Some(stored) = self.channels.get(&channel) else {
            return false;
        };
        stored
            .thread
            .as_deref()
            .and_then(|info| info.member)
            .is_some_and(|member| member.muted_now(now))
            || self
                .settings
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
    // Threads added after the build, whose badges the swap computes.
    late: Vec<ChannelId>,
}

impl Derived {
    pub(crate) fn build(entities: &Entities, now_millis: i64) -> Self {
        Self {
            badges: Badges::build(entities, now_millis),
            threads: ThreadIndex::build(entities),
            late: Vec::new(),
        }
    }

    pub(crate) fn add_thread(&mut self, thread: &Channel) {
        self.threads.add(thread);
        self.late.push(thread.id);
    }
}

// What a READY's swap replays of the local acks taken while it was prepared.
#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
struct Journal {
    acks: Vec<(ChannelId, MessageId)>,
    // Undone since: nothing to replay, but their badges moved after the diff.
    undone: BTreeSet<ChannelId>,
}

// What the store made of a thread Get Channel brought.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Learned {
    Kept,
    NotAThread,
    // Its guild isn't in the store: down, or not restored yet.
    Unplaced,
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
    // The channels the user is reading: a channel and the thread beside it.
    reading: Vec<reading::Reading>,
    // Their local acks from the latest write, for the account to send.
    due: Vec<Ack>,
    // Channels whose cursor came from an unread update and may be behind Discord's.
    stale_cursors: HashSet<ChannelId>,
    // Open while a READY is prepared.
    journal: Option<Journal>,
    // Replayed acks that kept READY's mention count: their echo brings Discord's.
    recounts: HashMap<ChannelId, MessageId>,
    // The memberships of joined threads in guilds that are down, for their GUILD_CREATE.
    outage_members: HashMap<ChannelId, (GuildId, ThreadMembership)>,
    // Joins for threads the store doesn't have yet, for the THREAD_CREATE that brings them.
    pending_joins: HashMap<ChannelId, ThreadMembership>,
    // Threads Get Channel refused or didn't find, for the session.
    unavailable_threads: HashSet<ChannelId>,
    // Threads Get Channel brought this session.
    fetched_threads: HashSet<ChannelId>,
    // Each thread's newest message, for its box; never cached.
    latest: HashMap<ChannelId, Arc<Message>>,
    // Threads messages brought before their guild was known.
    unplaced_threads: HashMap<ChannelId, model::Channel>,
    // Unjoined threads kept across the last READY that neither the gateway nor a request has
    // brought since: archived meanwhile, for all the store knows.
    unconfirmed: HashSet<ChannelId>,
    // Each channel's pages of archived threads in this connection.
    older: HashMap<ChannelId, browser::OlderPages>,
    // READYs so far: an answer to a request asked before the last one says nothing about this
    // connection.
    generation: u64,
    // The channel whose Threads browser is open.
    browsing: Option<ChannelId>,
    clock: Arc<Clock>,
}

impl State {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_limits(super::windows::DEFAULT_LIMITS, Arc::new(Clock::new()))
    }

    pub(crate) fn with_limits(limits: WindowLimits, clock: Arc<Clock>) -> Self {
        Self {
            entities: Entities::default(),
            badges: Badges::default(),
            threads: ThreadIndex::default(),
            dirty: Dirty::default(),
            ready: false,
            restored: false,
            windows: Windows::new(limits),
            cooldowns: Cooldowns::default(),
            reading: Vec::new(),
            due: Vec::new(),
            stale_cursors: HashSet::new(),
            journal: None,
            recounts: HashMap::new(),
            outage_members: HashMap::new(),
            pending_joins: HashMap::new(),
            unavailable_threads: HashSet::new(),
            fetched_threads: HashSet::new(),
            latest: HashMap::new(),
            unplaced_threads: HashMap::new(),
            unconfirmed: HashSet::new(),
            older: HashMap::new(),
            generation: 0,
            browsing: None,
            clock,
        }
    }

    // The store's clock, which the deadline task reads too.
    pub(crate) fn now_millis(&self) -> i64 {
        self.clock.now_millis()
    }

    pub(crate) fn replace(&mut self, mut next: Entities, events: &mut Vec<StoreEvent>) {
        self.keep_folders(&mut next);
        self.keep_memberships(&mut next);
        self.keep_known_threads(&mut next);
        let derived = Derived::build(&next, self.now_millis());
        let diff = self.ready_diff(&next, &derived);
        self.swap(next, derived, diff, events);
    }

    // A READY whose settings proto didn't decode keeps the folders the store had.
    pub(crate) fn keep_folders(&self, next: &mut Entities) {
        if std::mem::take(&mut next.settings.unreadable_folders) {
            next.settings.folders = self.entities.settings.folders.clone();
        }
    }

    // A payload without the user's member says nothing about its mute or level, and one
    // without the join time nothing about when the user joined.
    pub(crate) fn keep_memberships(&self, next: &mut Entities) {
        let memberless = std::mem::take(&mut next.memberless);
        for (id, channel) in &mut next.channels {
            let Some(member) = channel.thread.as_deref().and_then(|info| info.member) else {
                continue;
            };
            let whole = memberless.contains(id);
            if !whole && member.joined_at.is_some() {
                continue;
            }
            let Some(known) = self.stored_member(*id) else {
                continue;
            };
            if let Some(info) = Arc::make_mut(channel).thread.as_deref_mut() {
                info.member = Some(if whole {
                    known
                } else {
                    with_join_time(member, Some(known))
                });
            }
        }
    }

    // The membership the store has for a thread: its own, one kept through an outage, or a
    // join waiting for its thread.
    fn stored_member(&self, thread: ChannelId) -> Option<ThreadMembership> {
        self.entities
            .channels
            .get(&thread)
            .and_then(|known| known.thread.as_deref()?.member)
            .or_else(|| self.outage_members.get(&thread).map(|(_, member)| *member))
            .or_else(|| self.pending_joins.get(&thread).copied())
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

    pub(crate) fn begin_journal(&mut self) {
        self.journal = Some(Journal::default());
    }

    pub(crate) fn end_journal(&mut self) {
        self.journal = None;
    }

    #[cfg(test)]
    pub(crate) fn journaling(&self) -> bool {
        self.journal.is_some()
    }

    // Returns the old state, so the caller can drop it after releasing the lock.
    pub(crate) fn swap(
        &mut self,
        mut next: Entities,
        mut derived: Derived,
        mut diff: Option<ReadyDiff>,
        events: &mut Vec<StoreEvent>,
    ) -> (Entities, Derived) {
        self.recounts = match self.journal.take() {
            Some(journal) => self.replay(&journal, &mut next, &mut derived, diff.as_mut()),
            None => HashMap::new(),
        };
        // Known since the READY's own passes.
        for thread in self.keep_known_threads(&mut next) {
            derived.add_thread(&thread);
        }
        // Their badges weren't built with the READY's.
        let mut late = std::mem::take(&mut derived.late);
        late.extend(
            self.fetched_threads
                .iter()
                .filter(|id| next.channels.contains_key(id)),
        );
        if let Some(diff) = diff {
            events.extend(diff.events);
            for channel in diff.removed_channels {
                self.windows.drop_channel(channel);
                self.latest.remove(&channel);
            }
            // Pages loaded since a restore predate this session too: what happened in between
            // never comes as a dispatch.
            self.windows.mark_stale(events);
        }
        self.ready = true;
        events.push(StoreEvent::Ready);
        self.dirty = Dirty::default();
        for thread in late {
            self.dirty.channel(thread);
        }
        self.keep_outage_members(&next);
        self.pending_joins.clear();
        let old = Derived {
            badges: std::mem::replace(&mut self.badges, derived.badges),
            threads: std::mem::replace(&mut self.threads, derived.threads),
            late: Vec::new(),
        };
        let old = (std::mem::replace(&mut self.entities, next), old);
        // READY's cursors are fresh.
        self.stale_cursors.clear();
        self.follow_ready();
        self.follow_ready_threads();
        self.place_waiting_threads(events);
        // A guild READY doesn't list won't come in this session.
        self.unplaced_threads.clear();
        self.refresh_badges(self.now_millis(), events);
        old
    }

    pub(crate) fn me(&self) -> Option<UserId> {
        self.entities.me()
    }

    pub(crate) fn had_ready(&self) -> bool {
        self.ready
    }

    // Only before the first READY: a READY that came first is fresher than any cache, and one
    // being prepared would be swapped in undiffed.
    pub(crate) fn restore(
        &mut self,
        cached: Entities,
        derived: Derived,
        events: &mut Vec<StoreEvent>,
    ) -> bool {
        if self.ready || self.restored || self.journal.is_some() {
            return false;
        }
        self.entities = cached;
        self.badges = derived.badges;
        self.threads = derived.threads;
        self.restored = true;
        events.push(StoreEvent::Restored);
        self.place_waiting_threads(events);
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
        self.settle(self.now_millis(), events);
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
                self.put_threads_of(std::slice::from_ref(&message), events);
                // A message in a thread kept across READY shows it's still there.
                self.unconfirmed.remove(&channel);
                let boxed = self.count_created(&message) | self.note_live_latest(&message);
                if boxed {
                    self.reread_boxes(channel, events);
                }
                self.read_message(&message);
                self.dirty.channel(channel);
                let forced = self.forced_through(&message);
                self.windows.live(*message, &self.entities.users, events);
                let moved = self.note_message(channel, id, events);
                if moved && !forced {
                    self.stale_cursors.remove(&channel);
                }
                if moved && !boxed {
                    self.note_thread_activity(channel, events);
                }
                if Some(author) == self.entities.me() {
                    self.note_own_message(channel, nonce);
                }
            }
            E::MessageUpdate(update) => {
                if let Some(thread) = update.thread.as_deref() {
                    self.put_thread(thread.clone(), events);
                }
                if self.update_latest(&update) {
                    self.reread_boxes(update.channel_id, events);
                }
                self.windows.update(*update, &self.entities.users, events);
            }
            E::MessageDelete(delete) => {
                if self.count_deleted(delete.channel_id, &[delete.id])
                    | self.delete_latest(delete.channel_id, delete.id)
                {
                    self.reread_boxes(delete.channel_id, events);
                }
                self.windows.delete(delete.channel_id, delete.id, events);
            }
            E::MessageDeleteBulk(delete) => {
                let mut changed = self.count_deleted(delete.channel_id, &delete.ids);
                for id in &delete.ids {
                    changed |= self.delete_latest(delete.channel_id, *id);
                }
                if changed {
                    self.reread_boxes(delete.channel_id, events);
                }
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
            E::ThreadMemberUpdate(update) => self.thread_member_update(*update, events),
            E::ThreadMembersUpdate(update) => self.thread_members_update(*update, events),
            E::ThreadListSync(sync) => self.thread_list_sync(*sync, events),
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
        self.unconfirmed.remove(&channel.id);
        if let Some(known) = self.entities.channels.get(&channel.id) {
            if let Some(last) = known.last_message_id {
                channel.note_message(last);
            }
            if let (Some(info), Some(before)) = (channel.thread.as_deref_mut(), &known.thread) {
                // An absent member isn't a leave: only a members update or READY says so.
                if info.member.is_none() {
                    info.member = before.member;
                }
                if info.member_ids_preview.is_empty() {
                    info.member_ids_preview = before.member_ids_preview.clone();
                }
            }
        }
        let newest_before = self
            .entities
            .channels
            .get(&channel.id)
            .and_then(|known| known.last_message_id);
        let stored = self.stored_member(channel.id);
        let waiting = self.pending_joins.remove(&channel.id);
        if let Some(info) = channel.thread.as_deref_mut() {
            if info.member.is_none() && waiting.is_some() {
                tracing::debug!(
                    channel_id = channel.id.get(),
                    "a kept join reached its thread"
                );
                info.member = waiting;
            }
            info.member = info.member.map(|member| with_join_time(member, stored));
        }
        let channel = Arc::new(channel);
        self.mark_channel(&channel);
        self.threads.add(&channel);
        let id = channel.id;
        match self.entities.channels.get(&id) {
            Some(known) if *known == channel => {}
            Some(_) => {
                self.entities.channels.insert(id, channel.clone());
                events.push(StoreEvent::ChannelUpdated(channel));
            }
            None => {
                self.entities.channels.insert(id, channel.clone());
                events.push(StoreEvent::ChannelAdded(channel));
            }
        }
        if self.drop_stale_latest(id, newest_before) {
            self.reread_boxes(id, events);
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
        if let Some(interval) = self.cooldown_interval(channel, self.now_millis()) {
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
        self.unconfirmed.remove(&update.id);
        let next = Arc::new(known.patch(update));
        if next != known {
            let id = next.id;
            self.mark_channel(&next);
            self.threads.remove(&known);
            self.threads.add(&next);
            self.entities.channels.insert(id, next.clone());
            events.push(StoreEvent::ChannelUpdated(next));
            if self.drop_stale_latest(id, known.last_message_id) {
                self.reread_boxes(id, events);
            }
        }
    }

    fn thread_member_update(&mut self, update: ThreadMemberUpdate, events: &mut Vec<StoreEvent>) {
        let member = update.member;
        let Some(thread) = member.id else {
            tracing::debug!("skipping a thread member update without its thread");
            return;
        };
        if member
            .user_id
            .is_some_and(|user| Some(user) != self.entities.me())
        {
            return;
        }
        self.set_membership(thread, Some(ThreadMembership::from_wire(member)), events);
    }

    // Only the current user's membership is kept; others joining or leaving change nothing.
    fn thread_members_update(&mut self, update: ThreadMembersUpdate, events: &mut Vec<StoreEvent>) {
        let Some(me) = self.entities.me() else {
            return;
        };
        let member = if let Some(added) = update
            .added_members
            .into_iter()
            .find(|member| member.user_id == Some(me))
        {
            Some(ThreadMembership::from_wire(added))
        } else if update.removed_member_ids.contains(&me) {
            None
        } else {
            return;
        };
        self.set_membership(update.id, member, events);
    }

    // As the official client reads it: it adds and updates threads, keeps the ones it doesn't
    // list, and its members are the user's own memberships.
    fn thread_list_sync(&mut self, sync: ThreadListSync, events: &mut Vec<StoreEvent>) {
        for thread in sync.threads {
            self.put_thread(thread, events);
        }
        let me = self.entities.me();
        for member in sync.members {
            let Some(thread) = member.id else {
                continue;
            };
            if member.user_id.is_none_or(|user| Some(user) == me) {
                self.set_membership(thread, Some(ThreadMembership::from_wire(member)), events);
            }
        }
        for message in &sync.most_recent_messages {
            if self.note_latest(message) {
                self.reread_boxes(message.channel_id, events);
            }
        }
    }

    pub(crate) fn set_own_membership(
        &mut self,
        thread: ChannelId,
        joined_at: Option<Timestamp>,
        events: &mut Vec<StoreEvent>,
    ) {
        let joined = self
            .entities
            .channels
            .get(&thread)
            .and_then(|channel| channel.thread.as_deref())
            .map(ThreadInfo::joined);
        match (joined, joined_at) {
            (Some(false), Some(joined_at)) => {
                let member = ThreadMembership {
                    joined_at: Some(joined_at),
                    ..ThreadMembership::default()
                };
                self.set_membership(thread, Some(member), events);
            }
            (Some(true), None) => self.set_membership(thread, None, events),
            _ => return,
        }
        self.settle(self.now_millis(), events);
    }

    fn set_membership(
        &mut self,
        thread: ChannelId,
        member: Option<ThreadMembership>,
        events: &mut Vec<StoreEvent>,
    ) {
        let stored = self.stored_member(thread);
        let member = member.map(|member| with_join_time(member, stored));
        let Some(known) = self
            .entities
            .channels
            .get(&thread)
            .filter(|known| known.thread.is_some())
            .cloned()
        else {
            // Discord sends the thread with THREAD_CREATE, which can come after the join.
            if !self.entities.channels.contains_key(&thread) {
                tracing::debug!(
                    channel_id = thread.get(),
                    "keeping a membership change until its thread arrives"
                );
                match member {
                    Some(member) => self.pending_joins.insert(thread, member),
                    None => self.pending_joins.remove(&thread),
                };
            }
            return;
        };
        if known
            .thread
            .as_deref()
            .is_some_and(|info| info.member == member)
        {
            return;
        }
        let mut next = (*known).clone();
        if let Some(info) = next.thread.as_deref_mut() {
            info.member = member;
        }
        let next = Arc::new(next);
        self.dirty.channel(thread);
        self.entities.channels.insert(thread, next.clone());
        events.push(StoreEvent::ChannelUpdated(next));
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
        self.latest.remove(&removed.id);
        self.unconfirmed.remove(&removed.id);
        self.older.remove(&removed.id);
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
            self.latest.remove(&thread);
            self.unconfirmed.remove(&thread);
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
        self.settle(self.now_millis(), events);
    }

    fn merge_guild(&mut self, mut next: Entities, events: &mut Vec<StoreEvent>) {
        self.keep_memberships(&mut next);
        // Like READY, GUILD_CREATE lists only the joined threads.
        self.keep_known_threads(&mut next);
        let Some((&id, after)) = next.guilds.iter().next() else {
            return;
        };
        self.outage_members.retain(|_, (guild, _)| *guild != id);
        for channel in next.channels.keys() {
            self.pending_joins.remove(channel);
        }
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
        self.follow_guild_threads(id, &mut next, events);
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
                    self.latest.remove(id);
                    self.unconfirmed.remove(id);
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
        self.place_waiting_threads(events);
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
            self.outage_members
                .retain(|_, (guild, _)| *guild != delete.id);
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
            self.stash_members(id);
            self.remove_guild_data(id);
            events.push(StoreEvent::GuildUnavailable { guild_id: id });
        }
    }

    fn stash_members(&mut self, guild: GuildId) {
        for channel in self.entities.channels.values() {
            if channel.guild_id == Some(guild)
                && let Some(member) = channel.thread.as_deref().and_then(|info| info.member)
            {
                self.outage_members.insert(channel.id, (guild, member));
            }
        }
    }

    // Before the swap: a guild READY lists as down keeps its threads' memberships until it's back.
    fn keep_outage_members(&mut self, next: &Entities) {
        self.outage_members
            .retain(|_, (guild, _)| next.unavailable.contains(guild));
        let down: Vec<GuildId> = next
            .unavailable
            .iter()
            .filter(|guild| self.entities.guilds.contains_key(guild))
            .copied()
            .collect();
        for guild in down {
            self.stash_members(guild);
        }
    }

    fn remove_guild_data(&mut self, id: GuildId) {
        self.dirty.guild(Some(id));
        self.entities.guilds.remove(&id);
        self.entities.members.remove(&id);
        let windows = &mut self.windows;
        let threads = &mut self.threads;
        let latest = &mut self.latest;
        let older = &mut self.older;
        let unconfirmed = &mut self.unconfirmed;
        self.entities.channels.retain(|channel_id, channel| {
            let keep = channel.guild_id != Some(id);
            if !keep {
                windows.drop_channel(*channel_id);
                latest.remove(channel_id);
                older.remove(channel_id);
                unconfirmed.remove(channel_id);
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
        placing: u64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<LoadTicket> {
        let newest = self
            .entities
            .channels
            .get(&channel)
            .and_then(|stored| stored.last_message_id)
            .map(|newest| newest.cast());
        self.windows
            .begin_load(channel, kind, placing, newest, events)
    }

    pub(crate) fn finish_load(
        &mut self,
        ticket: LoadTicket,
        page: Vec<model::Message>,
        reached_end: bool,
        events: &mut Vec<StoreEvent>,
    ) -> PageOutcome {
        let present =
            ticket.cursor == Cursor::Latest || (ticket.kind == LoadKind::Newer && reached_end);
        let newest = page
            .iter()
            .map(|message| message.id)
            .max()
            .filter(|_| present);
        self.put_threads_of(&page, events);
        let outcome =
            self.windows
                .finish_load(ticket, page, &self.entities.users, reached_end, events);
        if outcome == PageOutcome::Applied
            && ticket.cursor == Cursor::Latest
            && self.note_newest_window(ticket.channel)
        {
            self.reread_boxes(ticket.channel, events);
        }
        if let Some(newest) = newest {
            self.reach_present(ticket.channel, newest, events);
        }
        self.settle(self.now_millis(), events);
        outcome
    }

    pub(crate) fn supersede_around(&mut self, channel: ChannelId, placing: u64) {
        self.windows.supersede_around(channel, placing);
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
        let mut entities = self.entities.clone();
        // An unjoined thread is kept only for its own cached window or a cached message's box; the
        // rest come again with the next subscription's sync and pages.
        let shown: HashSet<ChannelId> = windows
            .iter()
            .flat_map(|(_, window)| &window.messages)
            .filter(|message| message.has_thread())
            .map(|message| message.id.cast())
            .chain(windows.iter().map(|(channel, _)| *channel))
            .collect();
        entities.channels.retain(|id, channel| {
            channel
                .thread
                .as_deref()
                .is_none_or(|info| info.joined() || shown.contains(id))
        });
        (entities, windows)
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
                self.refresh_badges(self.now_millis(), events);
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
            .chain(self.badges.next_thread_mute_end(after))
            .min()
    }

    pub(crate) fn listed(&self, channel: ChannelId) -> bool {
        self.badges.listed(channel)
    }

    // The last joined first, as Discord's sidebar; the newest first among equal or unknown times.
    pub(crate) fn listed_threads(&self, parent: ChannelId) -> Vec<Arc<Channel>> {
        let mut threads: Vec<Arc<Channel>> = self
            .threads
            .of(parent)
            .filter(|thread| self.listed(*thread))
            .filter_map(|thread| self.entities.channels.get(&thread).cloned())
            .collect();
        threads.sort_by_key(|thread| {
            let joined = thread
                .thread
                .as_deref()
                .and_then(|info| info.member)
                .and_then(|member| member.joined_at);
            Reverse((joined, thread.id))
        });
        threads
    }

    pub(crate) fn next_window_end(&self, after: Timestamp) -> Option<Timestamp> {
        self.badges.next_window_end(after)
    }

    pub(crate) fn windows_ending(&self, after: Timestamp, until: Timestamp) -> Vec<ChannelId> {
        self.badges.windows_ending(after, until)
    }

    // Whether a write since the last call brought a deadline before the ones ahead.
    pub(crate) fn take_sooner_deadline(&mut self) -> bool {
        self.badges.take_sooner()
    }

    // Their badges are computed again at `now_millis`, which moves them out of the list.
    pub(crate) fn end_windows(
        &mut self,
        threads: &[ChannelId],
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) {
        for thread in threads {
            self.dirty.channel(*thread);
        }
        self.refresh_badges(now_millis, events);
    }

    pub(crate) fn mutes_ending(&self, after: Timestamp, until: Timestamp) -> Vec<Option<GuildId>> {
        let ended: BTreeSet<Option<GuildId>> = self
            .entities
            .settings
            .mute_ends()
            .filter(|(_, end)| *end > after && *end <= until)
            .map(|(guild, _)| guild)
            .chain(self.badges.thread_mutes_ending(after, until))
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

    pub(crate) fn starter_message(&self, thread: ChannelId) -> Option<StarterMessage> {
        self.windows.starter_message(thread)
    }

    pub(crate) fn wants_starter(&self, thread: ChannelId) -> bool {
        self.windows.wanted_starter(thread).is_some()
    }

    pub(crate) fn starter_to_fetch(&mut self, thread: ChannelId) -> Option<(ChannelId, MessageId)> {
        self.windows.starter_to_fetch(thread)
    }

    pub(crate) fn settle_starter(
        &mut self,
        thread: ChannelId,
        fetch: StarterFetch,
        events: &mut Vec<StoreEvent>,
    ) {
        if let StarterFetch::Found(original) = &fetch {
            self.put_threads_of(std::slice::from_ref(original), events);
        }
        self.windows
            .settle_starter(thread, fetch, &self.entities.users, events);
    }

    pub(crate) fn fail_starter(&mut self, thread: ChannelId, events: &mut Vec<StoreEvent>) {
        self.windows.fail_starter(thread, events);
    }

    // Get Channel's answer, as a THREAD_CREATE would bring it.
    pub(crate) fn learn_thread(
        &mut self,
        thread: model::Channel,
        events: &mut Vec<StoreEvent>,
    ) -> Learned {
        let id = thread.id;
        if !matches!(
            thread.kind,
            model::ChannelType::NewsThread
                | model::ChannelType::PublicThread
                | model::ChannelType::PrivateThread
        ) {
            return Learned::NotAThread;
        }
        // The gateway's copy is newer than an answer to a request sent before it.
        if self.entities.channels.contains_key(&id) && !self.unconfirmed.contains(&id) {
            return Learned::Kept;
        }
        self.put_channel(thread, events);
        if !self.entities.channels.contains_key(&id) {
            return Learned::Unplaced;
        }
        self.fetched_threads.insert(id);
        self.settle(self.now_millis(), events);
        Learned::Kept
    }

    // READY lists only the joined, active threads, so it says nothing about the unjoined ones the
    // store has (from messages, syncs, pages or Get Channel) nor about one fetched this session;
    // they stay while their guild and parent do, an unjoined archived one while something shows
    // it. Returns the threads it put into `next`.
    pub(crate) fn keep_known_threads(&self, next: &mut Entities) -> Vec<Arc<Channel>> {
        let mut kept = Vec::new();
        for thread in self.entities.channels.values() {
            let Some(info) = thread.thread.as_deref() else {
                continue;
            };
            if info.joined() && !self.fetched_threads.contains(&thread.id) {
                continue;
            }
            // Pages and searches bring many: only what still shows one keeps it.
            if !info.joined()
                && info.archived
                && !self.fetched_threads.contains(&thread.id)
                && !self.shows(thread)
            {
                continue;
            }
            let placed = thread
                .guild_id
                .is_some_and(|guild| next.guilds.contains_key(&guild))
                && thread
                    .parent_id
                    .is_some_and(|parent| next.channels.contains_key(&parent));
            if placed && !next.channels.contains_key(&thread.id) {
                next.channels.insert(thread.id, thread.clone());
                next.kept_threads.insert(thread.id);
                kept.push(thread.clone());
            }
        }
        kept
    }

    pub(crate) fn thread_unavailable(&self, thread: ChannelId) -> bool {
        self.unavailable_threads.contains(&thread)
    }

    pub(crate) fn mark_thread_unavailable(
        &mut self,
        parent: ChannelId,
        thread: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.unavailable_threads.insert(thread) {
            return;
        }
        let guild_id = self
            .entities
            .channels
            .get(&parent)
            .and_then(|channel| channel.guild_id);
        if let Some(guild_id) = guild_id {
            events.push(StoreEvent::ThreadUnavailable {
                guild_id,
                thread_id: thread,
            });
        }
        // Discord says it's gone or hidden; the store may have it from the cache.
        if self.entities.channels.contains_key(&thread) {
            let delete = ChannelDelete {
                id: thread,
                guild_id,
                parent_id: Some(parent),
            };
            self.channel_delete(delete, events);
        }
    }
}

pub(crate) fn wall_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        })
}

mod boxes;
mod browser;
mod reading;

pub(crate) use browser::{OLDER_PAGE, OlderPage};

#[cfg(test)]
pub(crate) mod tests;

// A member without its join time keeps the stored one, which orders the list; only a leave
// clears it.
fn with_join_time(member: ThreadMembership, stored: Option<ThreadMembership>) -> ThreadMembership {
    ThreadMembership {
        joined_at: member
            .joined_at
            .or(stored.and_then(|stored| stored.joined_at)),
        ..member
    }
}
