use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Bound;

use super::apply::Entities;
use super::events::StoreEvent;
use super::read_states::ReadState;
use super::settings::{NotificationLevel, Unreads};
use super::threads::ThreadIndex;
use super::types::Channel;
use crate::model::{ChannelId, ChannelType, GuildId, MessageId, Permissions, Timestamp};

const USE_NEW_NOTIFICATIONS: u64 = 1 << 4;
const WEEK_MILLIS: i64 = 7 * 24 * 60 * 60 * 1000;
const IS_GUILD_RESOURCE_CHANNEL: u64 = 1 << 7;

/// What a list shows for a channel, a guild or a folder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Badge {
    /// Shows as unread: the white dot or pill, a bright row.
    pub unread: bool,
    /// The red count, shown also when muted.
    pub mentions: u32,
    /// A forum's or media channel's count beside its name.
    pub posts: PostCount,
}

/// What a forum's row counts beside its name, as the official client (G-8); nothing while muted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum PostCount {
    #[default]
    None,
    /// "N New", in the brand color: posts newer than the forum's ack that the user neither
    /// joined nor opened, when its unread setting is All Messages.
    New(u32),
    /// Posts with activity after the forum's ack that are unread, in the muted color.
    Unread(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Thread,
    // The official client never shows their chats as unread; mentions still count.
    Voice,
    Private,
}

impl Kind {
    fn of(channel: &Channel) -> Option<Self> {
        Some(match channel.kind {
            ChannelType::GuildText
            | ChannelType::GuildNews
            | ChannelType::GuildForum
            | ChannelType::GuildMedia => Self::Text,
            ChannelType::NewsThread | ChannelType::PublicThread | ChannelType::PrivateThread => {
                Self::Thread
            }
            ChannelType::GuildVoice | ChannelType::GuildStageVoice => Self::Voice,
            ChannelType::Dm | ChannelType::GroupDm => Self::Private,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Slot {
    badge: Badge,
    read_state: Option<ReadState>,
    guild: Option<GuildId>,
    dot: bool,
    mentions: u32,
    // A thread its guild's channel list shows.
    listed: bool,
    // A joined, open thread's inactivity window end, and its own timed mute's end.
    window_end: Option<Timestamp>,
    mute_end: Option<Timestamp>,
}

fn shown(slot: Option<&Slot>) -> (Badge, Option<ReadState>) {
    slot.map_or((Badge::default(), None), |slot| {
        (slot.badge, slot.read_state)
    })
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Sum {
    dots: u32,
    mentions: u32,
    muted: bool,
}

impl Sum {
    fn badge(self) -> Badge {
        Badge {
            unread: !self.muted && self.dots > 0,
            mentions: self.mentions,
            posts: PostCount::None,
        }
    }
}

/// What a write changed that can move a badge.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Dirty {
    channels: BTreeSet<ChannelId>,
    // `None`: every DM and group DM.
    guilds: BTreeSet<Option<GuildId>>,
    all: bool,
}

impl Dirty {
    pub(crate) fn channel(&mut self, channel: ChannelId) {
        self.channels.insert(channel);
    }

    pub(crate) fn guild(&mut self, guild: Option<GuildId>) {
        self.guilds.insert(guild);
    }

    pub(crate) fn all(&mut self) {
        self.all = true;
    }

    pub(crate) fn is_empty(&self) -> bool {
        !self.all && self.channels.is_empty() && self.guilds.is_empty()
    }
}

/// Every channel's and guild's badge, kept as the store changes, so reads cost a lookup.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Badges {
    channels: HashMap<ChannelId, Slot>,
    guilds: HashMap<GuildId, Sum>,
    // The slots' ends in order, so the deadline task finds the next ones without a scan.
    window_ends: BTreeSet<(Timestamp, ChannelId)>,
    mute_ends: BTreeSet<(Timestamp, ChannelId)>,
    // An update brought an end before every other one ahead, which the task may sleep past.
    sooner: bool,
}

impl Badges {
    pub(crate) fn build(entities: &Entities, threads: &ThreadIndex, now_millis: i64) -> Self {
        #[cfg(test)]
        hooks::built();
        let now = Timestamp::from_unix_millis(now_millis);
        let mut badges = Self {
            channels: HashMap::new(),
            guilds: entities
                .guilds
                .keys()
                .map(|guild| (*guild, Sum::default()))
                .collect(),
            ..Self::default()
        };
        for guild in entities.guilds.keys() {
            badges.set_muted(*guild, guild_muted(entities, *guild, now));
        }
        for channel in entities.channels.values() {
            if let Some(slot) = compute(entities, threads, channel, now_millis) {
                badges.add(channel.id, slot);
            }
        }
        badges
    }

    pub(crate) fn channel(&self, channel: ChannelId) -> Badge {
        shown(self.channels.get(&channel)).0
    }

    pub(crate) fn guild(&self, guild: GuildId) -> Badge {
        self.guilds
            .get(&guild)
            .map_or_else(Badge::default, |sum| sum.badge())
    }

    pub(crate) fn listed(&self, channel: ChannelId) -> bool {
        self.channels.get(&channel).is_some_and(|slot| slot.listed)
    }

    pub(crate) fn next_window_end(&self, after: Timestamp) -> Option<Timestamp> {
        first_after(&self.window_ends, after)
    }

    pub(crate) fn windows_ending(&self, after: Timestamp, until: Timestamp) -> Vec<ChannelId> {
        ending(&self.window_ends, after, until).collect()
    }

    pub(crate) fn next_thread_mute_end(&self, after: Timestamp) -> Option<Timestamp> {
        first_after(&self.mute_ends, after)
    }

    // The guilds of the threads whose own mute ended within `after..=until`.
    pub(crate) fn thread_mutes_ending(
        &self,
        after: Timestamp,
        until: Timestamp,
    ) -> impl Iterator<Item = Option<GuildId>> + '_ {
        ending(&self.mute_ends, after, until)
            .map(|thread| self.channels.get(&thread).and_then(|slot| slot.guild))
    }

    pub(crate) fn take_sooner(&mut self) -> bool {
        std::mem::take(&mut self.sooner)
    }

    /// Events for every badge that differs in `next`, a later session's.
    pub(crate) fn diff(&self, next: &Self, events: &mut Vec<StoreEvent>) {
        let channels: BTreeSet<ChannelId> = self
            .channels
            .keys()
            .chain(next.channels.keys())
            .copied()
            .collect();
        for channel in channels {
            let (before, after) = (self.channels.get(&channel), next.channels.get(&channel));
            // A channel that is gone has its own event.
            if after.is_some() && shown(before) != shown(after) {
                events.push(StoreEvent::BadgeChanged {
                    channel_id: channel,
                    guild_id: after.or(before).and_then(|slot| slot.guild),
                });
            }
            push_listing(channel, before, after, events);
        }
        let guilds: BTreeSet<GuildId> = next.guilds.keys().copied().collect();
        for guild in guilds {
            if self.guild(guild) != next.guild(guild) {
                events.push(StoreEvent::GuildBadgeChanged { guild_id: guild });
            }
        }
    }

    // `diff`'s events for `channels` and their guilds, replacing the ones it gave them before
    // they changed in `next`.
    pub(crate) fn rediff(
        &self,
        next: &Self,
        channels: &BTreeSet<ChannelId>,
        events: &mut Vec<StoreEvent>,
    ) {
        let guilds: BTreeSet<GuildId> = channels
            .iter()
            .flat_map(|channel| [self.channels.get(channel), next.channels.get(channel)])
            .flatten()
            .filter_map(|slot| slot.guild)
            .collect();
        events.retain(|event| match event {
            StoreEvent::BadgeChanged { channel_id, .. }
            | StoreEvent::ThreadListingChanged {
                thread_id: channel_id,
                ..
            } => !channels.contains(channel_id),
            StoreEvent::GuildBadgeChanged { guild_id } => !guilds.contains(guild_id),
            _ => true,
        });
        for channel in channels {
            let (before, after) = (self.channels.get(channel), next.channels.get(channel));
            if after.is_some() && shown(before) != shown(after) {
                events.push(StoreEvent::BadgeChanged {
                    channel_id: *channel,
                    guild_id: after.or(before).and_then(|slot| slot.guild),
                });
            }
            push_listing(*channel, before, after, events);
        }
        for guild in guilds {
            if next.guilds.contains_key(&guild) && self.guild(guild) != next.guild(guild) {
                events.push(StoreEvent::GuildBadgeChanged { guild_id: guild });
            }
        }
    }

    pub(crate) fn update(
        &mut self,
        entities: &Entities,
        threads: &ThreadIndex,
        dirty: Dirty,
        now_millis: i64,
        events: &mut Vec<StoreEvent>,
    ) {
        if dirty.all {
            let next = Self::build(entities, threads, now_millis);
            self.diff(&next, events);
            *self = next;
            return;
        }
        let now = Timestamp::from_unix_millis(now_millis);
        let mut before = BTreeMap::new();
        let mut channels = dirty.channels;
        for guild in dirty.guilds {
            if let Some(guild) = guild {
                before.entry(guild).or_insert_with(|| self.guild(guild));
                if entities.guilds.contains_key(&guild) {
                    self.guilds.entry(guild).or_default();
                    self.set_muted(guild, guild_muted(entities, guild, now));
                } else {
                    self.guilds.remove(&guild);
                }
            }
            channels.extend(
                entities
                    .channels
                    .values()
                    .filter(|channel| channel.guild_id == guild)
                    .map(|channel| channel.id),
            );
            channels.extend(
                self.channels
                    .iter()
                    .filter(|(_, slot)| slot.guild == guild)
                    .map(|(id, _)| *id),
            );
        }
        // A post's change moves its forum's count.
        let forums: Vec<ChannelId> = channels
            .iter()
            .filter_map(|channel| {
                let parent = entities.channels.get(channel)?.parent_id?;
                entities
                    .channels
                    .get(&parent)
                    .filter(|parent| {
                        matches!(
                            parent.kind,
                            ChannelType::GuildForum | ChannelType::GuildMedia
                        )
                    })
                    .map(|parent| parent.id)
            })
            .collect();
        channels.extend(forums);
        for channel in channels {
            let old = self.channels.remove(&channel);
            let new = entities
                .channels
                .get(&channel)
                .and_then(|stored| compute(entities, threads, stored, now_millis));
            for slot in old.iter().chain(&new) {
                if let Some(guild) = slot.guild {
                    before.entry(guild).or_insert_with(|| self.guild(guild));
                }
            }
            if let Some(old) = old {
                self.subtract(channel, &old);
            }
            if let Some(new) = new {
                self.add(channel, new);
                let ends = |slot: Option<&Slot>| slot.map(|slot| (slot.window_end, slot.mute_end));
                if ends(old.as_ref()) != ends(Some(&new)) {
                    self.note_sooner(new, now);
                }
            }
            if new.is_some() && shown(old.as_ref()) != shown(new.as_ref()) {
                events.push(StoreEvent::BadgeChanged {
                    channel_id: channel,
                    guild_id: new.or(old).and_then(|slot| slot.guild),
                });
            }
            push_listing(channel, old.as_ref(), new.as_ref(), events);
        }
        for (guild, before) in before {
            if self.guilds.contains_key(&guild) && self.guild(guild) != before {
                events.push(StoreEvent::GuildBadgeChanged { guild_id: guild });
            }
        }
    }

    fn set_muted(&mut self, guild: GuildId, muted: bool) {
        if let Some(sum) = self.guilds.get_mut(&guild) {
            sum.muted = muted;
        }
    }

    fn add(&mut self, channel: ChannelId, slot: Slot) {
        if let Some(sum) = slot.guild.and_then(|guild| self.guilds.get_mut(&guild)) {
            sum.dots += u32::from(slot.dot);
            sum.mentions = sum.mentions.saturating_add(slot.mentions);
        }
        if let Some(end) = slot.window_end {
            self.window_ends.insert((end, channel));
        }
        if let Some(end) = slot.mute_end {
            self.mute_ends.insert((end, channel));
        }
        self.channels.insert(channel, slot);
    }

    fn subtract(&mut self, channel: ChannelId, slot: &Slot) {
        if let Some(sum) = slot.guild.and_then(|guild| self.guilds.get_mut(&guild)) {
            sum.dots = sum.dots.saturating_sub(u32::from(slot.dot));
            sum.mentions = sum.mentions.saturating_sub(slot.mentions);
        }
        if let Some(end) = slot.window_end {
            self.window_ends.remove(&(end, channel));
        }
        if let Some(end) = slot.mute_end {
            self.mute_ends.remove(&(end, channel));
        }
    }

    // After `slot` is added: whether one of its ends is now the first ahead.
    fn note_sooner(&mut self, slot: Slot, now: Timestamp) {
        let first = |end: Option<Timestamp>, ends: &BTreeSet<(Timestamp, ChannelId)>| {
            end.is_some_and(|end| end > now && first_after(ends, now) == Some(end))
        };
        if first(slot.window_end, &self.window_ends) || first(slot.mute_end, &self.mute_ends) {
            self.sooner = true;
        }
    }
}

fn first_after(ends: &BTreeSet<(Timestamp, ChannelId)>, after: Timestamp) -> Option<Timestamp> {
    ends.range((
        Bound::Excluded((after, ChannelId::new(u64::MAX))),
        Bound::Unbounded,
    ))
    .next()
    .map(|(end, _)| *end)
}

fn ending(
    ends: &BTreeSet<(Timestamp, ChannelId)>,
    after: Timestamp,
    until: Timestamp,
) -> impl Iterator<Item = ChannelId> + '_ {
    let last = ChannelId::new(u64::MAX);
    ends.range((
        Bound::Excluded((after, last)),
        Bound::Included((until, last)),
    ))
    .map(|(_, channel)| *channel)
}

// A thread that appears or goes has its own channel event.
fn push_listing(
    channel: ChannelId,
    before: Option<&Slot>,
    after: Option<&Slot>,
    events: &mut Vec<StoreEvent>,
) {
    if let (Some(before), Some(after)) = (before, after)
        && before.listed != after.listed
        && let Some(guild_id) = after.guild
    {
        events.push(StoreEvent::ThreadListingChanged {
            guild_id,
            thread_id: channel,
        });
    }
}

fn guild_muted(entities: &Entities, guild: GuildId, now: Timestamp) -> bool {
    entities
        .settings
        .guild(Some(guild))
        .is_some_and(|settings| settings.muted_now(now))
}

// `None` for channels that never show a badge: categories, directories and the like.
fn compute(
    entities: &Entities,
    threads: &ThreadIndex,
    channel: &Channel,
    now_millis: i64,
) -> Option<Slot> {
    let kind = Kind::of(channel)?;
    let now = Timestamp::from_unix_millis(now_millis);
    let read_state = entities.read_states.get(channel.id);
    let mentions = read_state.map_or(0, |state| state.mentions);
    let muted = entities.channel_muted_at(channel.id, now);
    let Some(guild) = channel.guild_id else {
        return Some(Slot {
            badge: Badge {
                unread: !muted && entities.unread(channel),
                mentions,
                posts: PostCount::None,
            },
            read_state,
            guild: None,
            dot: false,
            mentions: 0,
            listed: false,
            window_end: None,
            mute_end: None,
        });
    };
    let hidden = Slot {
        badge: Badge::default(),
        read_state,
        guild: Some(guild),
        dot: false,
        mentions: 0,
        listed: false,
        window_end: None,
        mute_end: None,
    };
    let visible = entities
        .permissions(channel.id, now_millis)
        .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL));
    // The official client tracks no unreads in Server Guide resource channels, in any server.
    let resource = channel.flags & IS_GUILD_RESOURCE_CHANNEL != 0;
    if !visible || resource {
        return Some(hidden);
    }
    // The official client's gates for community servers with "Channels & Roles": a channel
    // quiet for a week shows nothing, one the user didn't pick only its mentions.
    let opt_in = opt_in_enabled(entities, guild);
    let stale = opt_in
        && channel
            .last_message_id
            .is_none_or(|last| last.unix_millis() < now_millis - WEEK_MILLIS);
    let mentions = if stale { 0 } else { mentions };
    let unpicked = opt_in && kind != Kind::Thread && mentions == 0 && !opted_in(entities, channel);
    // Unread or mentioned threads stay listed past their window, as in the official client.
    let listed = kind == Kind::Thread
        && channel
            .thread
            .as_deref()
            .is_some_and(|info| info.joined() && !info.archived)
        && (channel.window_end().is_none_or(|end| now < end)
            || entities.unread(channel)
            || mentions > 0);
    let unread = kind != Kind::Voice
        && !muted
        && !stale
        && !unpicked
        && entities.unread(channel)
        && shows_unreads(entities, channel);
    let hides_muted = entities
        .settings
        .guild(Some(guild))
        .is_some_and(|settings| settings.hide_muted_channels);
    let hidden = muted && hides_muted;
    // The guild counts the threads its list shows, whatever their parent's mute.
    let counted = !hidden && (kind != Kind::Thread || listed);
    let parent_muted = kind != Kind::Thread && under_muted_parent(entities, channel, now);
    let posts = if muted {
        PostCount::None
    } else {
        post_count(entities, threads, channel, shows_unreads(entities, channel))
    };
    Some(Slot {
        badge: Badge {
            unread,
            mentions,
            posts,
        },
        read_state,
        guild: Some(guild),
        dot: counted && unread && !parent_muted,
        // A thread's mentions need a row to clear them from, so they wait for the join.
        mentions: if counted { mentions } else { 0 },
        listed,
        window_end: channel
            .thread
            .as_deref()
            .filter(|info| info.joined() && !info.archived)
            .and_then(|_| channel.window_end()),
        mute_end: channel
            .thread
            .as_deref()
            .and_then(|info| info.member)
            .filter(|member| member.muted)
            .and_then(|member| member.mute_ends),
    })
}

// Module 863005: new posts are active, unjoined, newer than the forum's ack (or the user's join
// to the guild) and never opened; the muted count is unread active posts with activity after it.
fn post_count(
    entities: &Entities,
    threads: &ThreadIndex,
    forum: &Channel,
    all_messages: bool,
) -> PostCount {
    if !matches!(
        forum.kind,
        ChannelType::GuildForum | ChannelType::GuildMedia
    ) {
        return PostCount::None;
    }
    let ack = entities
        .read_states
        .get(forum.id)
        .and_then(|state| state.acked)
        .or_else(|| {
            let joined = entities.members.get(&forum.guild_id?)?.joined_at?;
            Some(MessageId::from_unix_millis(joined.unix_millis(), 0))
        })
        .map_or(0, MessageId::get);
    let (mut new, mut unread) = (0, 0);
    for post in threads
        .of(forum.id)
        .filter_map(|id| entities.channels.get(&id))
    {
        let Some(info) = post.thread.as_deref() else {
            continue;
        };
        if info.archived {
            continue;
        }
        let opened = entities
            .read_states
            .get(post.id)
            .is_some_and(|state| !state.estimated);
        if post.id.get() > ack && !info.joined() && !opened {
            new += 1;
        }
        if post.last_message_id.is_some_and(|last| last.get() > ack) && entities.unread(post) {
            unread += 1;
        }
    }
    if all_messages && new > 0 {
        PostCount::New(new)
    } else if unread > 0 {
        PostCount::Unread(unread)
    } else {
        PostCount::None
    }
}

fn opt_in_enabled(entities: &Entities, guild: GuildId) -> bool {
    entities
        .guilds
        .get(&guild)
        .is_some_and(|stored| stored.community)
        && entities
            .settings
            .guild(Some(guild))
            .is_some_and(|settings| settings.opt_in)
}

fn opted_in(entities: &Entities, channel: &Channel) -> bool {
    let Some(settings) = entities.settings.guild(channel.guild_id) else {
        return false;
    };
    [Some(channel.id), channel.parent_id]
        .into_iter()
        .flatten()
        .any(|id| settings.channels.get(&id).is_some_and(|own| own.opted_in))
}

fn shows_unreads(entities: &Entities, channel: &Channel) -> bool {
    let settings = entities.settings.guild(channel.guild_id);
    let mut next = Some(channel.id);
    for _ in 0..3 {
        let Some(id) = next else {
            break;
        };
        if let Some(unreads) = settings
            .and_then(|settings| settings.channels.get(&id))
            .and_then(|own| own.unreads)
        {
            return unreads == Unreads::AllMessages;
        }
        next = entities
            .channels
            .get(&id)
            .and_then(|stored| stored.parent_id);
    }
    if let Some(unreads) = settings.and_then(|settings| settings.unreads) {
        return unreads == Unreads::AllMessages;
    }
    entities.settings.notification_flags & USE_NEW_NOTIFICATIONS == 0
        || entities.notification_level(channel.id) == NotificationLevel::AllMessages
}

// In the official client a muted category's channels look unread but add no dot to the server.
fn under_muted_parent(entities: &Entities, channel: &Channel, now: Timestamp) -> bool {
    let mut next = channel.parent_id;
    for _ in 0..2 {
        let Some(id) = next else {
            return false;
        };
        if entities.channel_muted_at(id, now) {
            return true;
        }
        next = entities
            .channels
            .get(&id)
            .and_then(|stored| stored.parent_id);
    }
    false
}

#[cfg(test)]
pub(crate) mod hooks {
    use std::cell::Cell;

    thread_local! {
        static IN_WRITE: Cell<bool> = const { Cell::new(false) };
        static READS: Cell<usize> = const { Cell::new(0) };
        static LOCKED_BUILDS: Cell<usize> = const { Cell::new(0) };
    }

    pub(crate) fn writing(active: bool) {
        IN_WRITE.with(|cell| cell.set(active));
    }

    pub(crate) fn reading(active: bool) {
        READS.with(|cell| {
            cell.set(if active {
                cell.get() + 1
            } else {
                cell.get() - 1
            })
        });
    }

    pub(super) fn built() {
        if IN_WRITE.with(Cell::get) || READS.with(Cell::get) > 0 {
            LOCKED_BUILDS.with(|cell| cell.set(cell.get() + 1));
        }
    }

    /// Full builds that ran under the store's read or write lock on this thread.
    pub(crate) fn locked_builds() -> usize {
        LOCKED_BUILDS.with(Cell::get)
    }
}
