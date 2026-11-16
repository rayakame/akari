use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use super::{Missed, Window, Windows, intern};
use crate::gateway::MessageUpdate;
use crate::model::{self, ChannelId, MessageId, MessageType, Timestamp, UserId};
use crate::state::apply::wall_millis;
use crate::state::events::StoreEvent;
use crate::state::reactions::{Reaction, ReactionChange, react};
use crate::state::types::{Message, User};

// The official client's page size.
pub(crate) const PINS_PAGE: u8 = 25;
const KEPT: usize = 10;

/// A channel's pinned messages as loaded: see [`Store::pins`](crate::state::Store::pins).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[non_exhaustive]
pub struct PinsPage {
    /// The newest pin first, as Discord lists them.
    pub items: Vec<PinnedMessage>,
    /// Discord has pins after `items`, or hasn't said yet in this connection which of the last
    /// ones are still pinned: [`Account::load_more_pins`](crate::Account::load_more_pins) asks.
    pub has_more: bool,
    pub state: PinsState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct PinnedMessage {
    /// Kept current by its edits and reactions, like a window's copy.
    pub message: Arc<Message>,
    /// When it was pinned; for a pin the gateway just brought, this device's clock until
    /// Discord names the time.
    pub pinned_at: Timestamp,
}

/// Where a channel's pins list stands in this connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum PinsState {
    /// Not asked for in this connection ([`Account::load_pins`](crate::Account::load_pins)
    /// asks); items shown are an earlier connection's until the answer replaces them.
    #[default]
    NotLoaded,
    /// A page is on its way.
    Loading,
    /// As Discord listed them, kept current by the gateway.
    Loaded,
    /// The last page failed. Without items it was the first, which isn't an empty list.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PinsLoad {
    First,
    More,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PinsAsk {
    pub(crate) channel: ChannelId,
    pub(crate) before: Option<Timestamp>,
    ticket: u64,
    // The channel's newest pin when the first page was asked for, which its answer has seen.
    seen: Option<Timestamp>,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(PartialEq))]
struct Pin {
    message: Arc<Message>,
    pinned_at: Timestamp,
    // The gateway placed it and no page has listed it yet: a page without it predates the pin.
    live: bool,
    // `pinned_at` is Discord's, not this device's clock.
    named: bool,
}

#[derive(Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct PinsList {
    items: Vec<Pin>,
    // The first ones are this connection's; the rest an earlier one's until a page reaches them.
    confirmed: usize,
    more: bool,
    // A page of this connection landed.
    fresh: bool,
    state: PinsState,
    // Restored when the request is dropped.
    previous: PinsState,
    // The newest pin time of the channel's that the list has placed.
    newest: Option<Timestamp>,
    // The page asked last: an answer to an earlier one is dropped.
    ticket: u64,
    // Changes while a page loads, which it may predate.
    missed: Vec<Missed>,
    used: u64,
}

impl PinsList {
    fn position(&self, id: MessageId) -> Option<usize> {
        self.items.iter().position(|pin| pin.message.id == id)
    }

    fn remove(&mut self, index: usize) {
        self.items.remove(index);
        if index < self.confirmed {
            self.confirmed -= 1;
        }
    }

    fn wants(&self, load: PinsLoad, last_pin: Option<Timestamp>) -> bool {
        match (load, self.state) {
            (_, PinsState::Loading) => false,
            (PinsLoad::First, PinsState::NotLoaded) => true,
            (PinsLoad::First, PinsState::Failed) if self.starts_over() => true,
            (PinsLoad::First, _) => self.behind(last_pin),
            (PinsLoad::More, _) => self.more,
        }
    }

    // No page of this connection landed, or none of its pins is left to go on from.
    fn starts_over(&self) -> bool {
        !self.fresh || self.confirmed == 0
    }

    fn complete(&self) -> bool {
        self.fresh && !self.more && self.confirmed == self.items.len()
    }

    // Discord named a pin time the list hasn't placed: a pin the gateway never brought.
    fn behind(&self, last_pin: Option<Timestamp>) -> bool {
        last_pin.is_some_and(|pin| newer(pin, self.newest))
    }
}

fn newer(time: Timestamp, than: Option<Timestamp>) -> bool {
    than.is_none_or(|known| time.unix_millis() > known.unix_millis())
}

fn latest(times: impl IntoIterator<Item = Option<Timestamp>>) -> Option<Timestamp> {
    times
        .into_iter()
        .flatten()
        .max_by_key(|time| time.unix_millis())
}

impl Windows {
    pub(crate) fn pins(&self, channel: ChannelId) -> PinsPage {
        let Some(list) = self.pins.get(&channel) else {
            return PinsPage::default();
        };
        PinsPage {
            items: list
                .items
                .iter()
                .map(|pin| PinnedMessage {
                    message: pin.message.clone(),
                    pinned_at: pin.pinned_at,
                })
                .collect(),
            has_more: list.more,
            state: list.state,
        }
    }

    pub(crate) fn pinned(&self, channel: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        let list = self.pins.get(&channel)?;
        list.position(id)
            .map(|index| list.items[index].message.clone())
    }

    // `last_pin`: the channel's newest pin time as Discord last said.
    pub(crate) fn wants_pins(
        &self,
        channel: ChannelId,
        load: PinsLoad,
        last_pin: Option<Timestamp>,
    ) -> bool {
        match self.pins.get(&channel) {
            Some(list) => list.wants(load, last_pin),
            None => load == PinsLoad::First,
        }
    }

    pub(crate) fn begin_pins(
        &mut self,
        channel: ChannelId,
        load: PinsLoad,
        last_pin: Option<Timestamp>,
        events: &mut Vec<StoreEvent>,
    ) -> Option<PinsAsk> {
        if !self.wants_pins(channel, load, last_pin) {
            return None;
        }
        let list = match load {
            PinsLoad::First => self.pins.entry(channel).or_default(),
            PinsLoad::More => self.pins.get_mut(&channel)?,
        };
        let before = match load {
            PinsLoad::More if !list.starts_over() => list
                .confirmed
                .checked_sub(1)
                .and_then(|last| list.items.get(last))
                .map(|pin| pin.pinned_at),
            _ => None,
        };
        self.pin_counter += 1;
        list.ticket = self.pin_counter;
        list.used = self.pin_counter;
        list.previous = list.state;
        list.state = PinsState::Loading;
        let ask = PinsAsk {
            channel,
            before,
            ticket: list.ticket,
            seen: last_pin.filter(|_| before.is_none()),
        };
        events.push(StoreEvent::PinsChanged {
            channel_id: channel,
        });
        self.evict_pins(channel, events);
        Some(ask)
    }

    fn evict_pins(&mut self, keep: ChannelId, events: &mut Vec<StoreEvent>) {
        while self.pins.len() > KEPT {
            let Some(oldest) = self
                .pins
                .iter()
                .filter(|(channel, _)| **channel != keep)
                .min_by_key(|(_, list)| list.used)
                .map(|(channel, _)| *channel)
            else {
                return;
            };
            self.drop_pins(oldest);
            events.push(StoreEvent::PinsChanged { channel_id: oldest });
        }
    }

    fn asked(&self, ask: &PinsAsk) -> bool {
        self.pins
            .get(&ask.channel)
            .is_some_and(|list| list.ticket == ask.ticket && list.state == PinsState::Loading)
    }

    pub(crate) fn finish_pins(
        &mut self,
        ask: PinsAsk,
        page: model::MessagePins,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.asked(&ask) {
            return;
        }
        let channel = ask.channel;
        let recent = self
            .windows
            .get(&channel)
            .map_or(&[][..], |window| window.messages.as_slice());
        let has_more = page.has_more;
        let mut seen = HashSet::new();
        let arrived: Vec<_> = page
            .items
            .into_iter()
            .filter(|item| item.message.channel_id == channel && seen.insert(item.message.id))
            .map(|item| {
                let mut wire = item.message;
                wire.pinned = true;
                let referenced = wire.referenced_message.take();
                let message = Arc::new(Message::from_wire(wire, &mut |user| {
                    intern(user, users, recent)
                }));
                let pinned_at = item.pinned_at.unwrap_or(message.timestamp);
                (message, referenced, pinned_at)
            })
            .collect();
        let mut page = Vec::with_capacity(arrived.len());
        for (message, referenced, pinned_at) in arrived {
            if message.kind != MessageType::ThreadStarterMessage {
                self.keep_referenced(&message, referenced, users, events);
            }
            self.targets.arrived(&message, events);
            page.push(Pin {
                message,
                pinned_at,
                live: false,
                named: true,
            });
        }
        // A page that leaves nothing would be asked again from the same cursor.
        let more = has_more && !page.is_empty();
        let window = self.windows.get(&channel).filter(|window| !window.stale);
        let Some(list) = self.pins.get_mut(&channel) else {
            return;
        };
        let took = take_current_reactions(list, window, &mut page);
        let paged: HashSet<MessageId> = page.iter().map(|pin| pin.message.id).collect();
        let live: HashMap<MessageId, Pin> = list
            .items
            .iter()
            .filter(|pin| pin.live)
            .map(|pin| (pin.message.id, pin.clone()))
            .collect();
        reconcile(list, ask, page, more, &paged);
        for missed in std::mem::take(&mut list.missed) {
            match &missed {
                Missed::Deleted(id) => self.targets.delete(channel, *id, events),
                Missed::Updated(update) => {
                    self.targets
                        .update(update, &mut |user| intern(user, users, &[]), events);
                }
                Missed::Reacted(..) => {}
            }
            replay(list, missed, &paged, &took, &live, users);
        }
        list.state = PinsState::Loaded;
        events.push(StoreEvent::PinsChanged {
            channel_id: channel,
        });
        self.trim_targets(channel);
    }

    pub(crate) fn fail_pins(&mut self, ask: PinsAsk, events: &mut Vec<StoreEvent>) {
        self.end_pins(ask, None, events);
    }

    // The request was dropped before Discord answered: the page can be asked for again.
    pub(crate) fn abandon_pins(&mut self, ask: PinsAsk, events: &mut Vec<StoreEvent>) {
        let previous = self.pins.get(&ask.channel).map(|list| list.previous);
        self.end_pins(ask, previous, events);
    }

    fn end_pins(&mut self, ask: PinsAsk, state: Option<PinsState>, events: &mut Vec<StoreEvent>) {
        if !self.asked(&ask) {
            return;
        }
        if let Some(list) = self.pins.get_mut(&ask.channel) {
            list.state = state.unwrap_or(PinsState::Failed);
            list.missed.clear();
            events.push(StoreEvent::PinsChanged {
                channel_id: ask.channel,
            });
        }
    }

    pub(crate) fn name_pin(
        &mut self,
        channel: ChannelId,
        last_pin: Option<Timestamp>,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(list) = self.pins.get_mut(&channel) else {
            return;
        };
        let Some(time) = last_pin.filter(|time| newer(*time, list.newest)) else {
            return;
        };
        if let Some(pin) = list
            .items
            .iter_mut()
            .rev()
            .find(|pin| pin.live && !pin.named)
        {
            pin.pinned_at = time;
            pin.named = true;
            list.newest = Some(time);
            events.push(StoreEvent::PinsChanged {
                channel_id: channel,
            });
        }
    }

    // `known`: the window's, a held or a starter's copy of the message before the update.
    pub(super) fn update_pins(
        &mut self,
        update: &MessageUpdate,
        known: Option<Arc<Message>>,
        users: &HashMap<UserId, Arc<User>>,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(list) = self.pins.get_mut(&update.channel_id) else {
            return;
        };
        if list.state == PinsState::Loading {
            list.missed.push(Missed::Updated(Box::new(update.clone())));
        }
        let mut interned = |user| intern(user, users, &[]);
        let changed = match (list.position(update.id), update.pinned) {
            (Some(index), Some(false)) => {
                list.remove(index);
                true
            }
            (Some(index), _) => {
                let pin = &mut list.items[index];
                let next = pin.message.patch(update.clone(), &mut interned);
                let changed = next != *pin.message;
                if changed {
                    pin.message = Arc::new(next);
                }
                changed
            }
            // Pinned before and not listed: on a page not loaded yet.
            (None, Some(true)) => match known.filter(|known| !known.pinned || list.complete()) {
                Some(known) => {
                    let message = known.patch(update.clone(), &mut interned);
                    let now = Timestamp::from_unix_millis(wall_millis());
                    let front = list.items.first().map(|pin| pin.pinned_at);
                    list.items.insert(
                        0,
                        Pin {
                            message: Arc::new(message),
                            pinned_at: latest([Some(now), front]).unwrap_or(now),
                            live: true,
                            named: false,
                        },
                    );
                    list.confirmed += 1;
                    true
                }
                None => false,
            },
            (None, _) => false,
        };
        if changed {
            events.push(StoreEvent::PinsChanged {
                channel_id: update.channel_id,
            });
        }
    }

    pub(super) fn delete_pin(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(list) = self.pins.get_mut(&channel) else {
            return;
        };
        if list.state == PinsState::Loading {
            list.missed.push(Missed::Deleted(id));
        }
        if let Some(index) = list.position(id) {
            list.remove(index);
            events.push(StoreEvent::PinsChanged {
                channel_id: channel,
            });
        }
    }

    pub(super) fn react_pins(
        &mut self,
        channel: ChannelId,
        id: MessageId,
        changes: &[ReactionChange],
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(list) = self.pins.get_mut(&channel) else {
            return;
        };
        if list.state == PinsState::Loading {
            for change in changes {
                list.missed.push(Missed::Reacted(id, change.clone()));
            }
        }
        if let Some(index) = list.position(id)
            && react(&mut list.items[index].message, changes)
        {
            events.push(StoreEvent::PinsChanged {
                channel_id: channel,
            });
        }
    }

    // A new session may have missed pins and unpins.
    pub(super) fn stale_pins(&mut self, events: &mut Vec<StoreEvent>) {
        let mut channels: Vec<ChannelId> = self.pins.keys().copied().collect();
        channels.sort_unstable();
        for channel in channels {
            let Some(list) = self.pins.get_mut(&channel) else {
                continue;
            };
            list.confirmed = 0;
            list.fresh = false;
            list.state = PinsState::NotLoaded;
            list.previous = PinsState::NotLoaded;
            list.missed.clear();
            for pin in &mut list.items {
                pin.live = false;
            }
            events.push(StoreEvent::PinsChanged {
                channel_id: channel,
            });
        }
    }

    pub(super) fn drop_pins(&mut self, channel: ChannelId) {
        let Some(list) = self.pins.remove(&channel) else {
            return;
        };
        let answered: HashSet<ChannelId> = list
            .items
            .iter()
            .filter_map(|pin| pin.message.replied_to())
            .map(|(channel, _)| channel)
            .collect();
        for channel in answered {
            self.loosen_targets(channel);
        }
    }

    pub(super) fn pinned_messages(&self) -> impl Iterator<Item = &Arc<Message>> {
        self.pins
            .values()
            .flat_map(|list| list.items.iter().map(|pin| &pin.message))
    }
}

// A copy that was current counted every reaction since; the page may or may not have them.
fn take_current_reactions(
    list: &PinsList,
    window: Option<&Window>,
    page: &mut [Pin],
) -> HashSet<MessageId> {
    let current = |id: MessageId| -> Option<Option<Arc<[Reaction]>>> {
        let listed = list
            .items
            .iter()
            .take(list.confirmed)
            .find(|pin| pin.message.id == id)
            .map(|pin| pin.message.reactions.clone());
        listed.or_else(|| {
            let window = window?;
            [&window.messages, &window.held]
                .into_iter()
                .find_map(|copies| {
                    let index = Window::position(copies, id).ok()?;
                    copies.get(index)
                })
                .map(|message| message.reactions.clone())
        })
    };
    let mut took = HashSet::new();
    for pin in page {
        if let Some(reactions) = current(pin.message.id) {
            if reactions != pin.message.reactions {
                pin.message = Arc::new(Message {
                    reactions,
                    ..(*pin.message).clone()
                });
            }
            took.insert(pin.message.id);
        }
    }
    took
}

// Pin times say which of the list's pins a page answers for.
fn reconcile(
    list: &mut PinsList,
    ask: PinsAsk,
    page: Vec<Pin>,
    more: bool,
    paged: &HashSet<MessageId>,
) {
    let old = std::mem::take(&mut list.items);
    let confirmed = list.confirmed;
    let (newer, rest) = match ask.before {
        Some(_) => old.split_at(confirmed.min(old.len())),
        None => old.split_at(0),
    };
    let first = page.first().map(|pin| pin.pinned_at);
    let low = page
        .last()
        .map(|pin| pin.pinned_at.unix_millis())
        .filter(|_| more);
    let mut kept = Vec::new();
    let mut older = Vec::new();
    let mut older_confirmed = 0;
    for (index, pin) in rest.iter().enumerate() {
        if paged.contains(&pin.message.id) {
            continue;
        }
        if pin.live {
            kept.push(pin.clone());
        } else if low.is_some_and(|low| pin.pinned_at.unix_millis() < low) {
            if newer.len() + index < confirmed && older_confirmed == older.len() {
                older_confirmed += 1;
            }
            older.push(pin.clone());
        }
    }
    let newer: Vec<Pin> = newer
        .iter()
        .filter(|pin| !paged.contains(&pin.message.id))
        .cloned()
        .collect();
    list.confirmed = kept.len() + newer.len() + page.len() + older_confirmed;
    list.items = kept
        .into_iter()
        .chain(newer)
        .chain(page)
        .chain(older)
        .collect();
    list.more = more;
    list.fresh = true;
    list.newest = latest([list.newest, first, ask.seen]);
}

// `live`: the pins the gateway had placed before the page landed.
fn replay(
    list: &mut PinsList,
    missed: Missed,
    paged: &HashSet<MessageId>,
    took: &HashSet<MessageId>,
    live: &HashMap<MessageId, Pin>,
    users: &HashMap<UserId, Arc<User>>,
) {
    match missed {
        Missed::Deleted(id) => {
            if let Some(index) = list.position(id) {
                list.remove(index);
            }
        }
        Missed::Reacted(id, change) => {
            if paged.contains(&id)
                && !took.contains(&id)
                && let Some(index) = list.position(id)
            {
                react(
                    &mut list.items[index].message,
                    std::slice::from_ref(&change),
                );
            }
        }
        Missed::Updated(update) => {
            if !paged.contains(&update.id) {
                return;
            }
            let Some(index) = list.position(update.id) else {
                // An unpin replayed before took the page's copy: the gateway's repin after it stays.
                if update.pinned == Some(true)
                    && let Some(pin) = live.get(&update.id)
                {
                    let at = list.items.partition_point(|other| {
                        other.pinned_at.unix_millis() >= pin.pinned_at.unix_millis()
                    });
                    list.items.insert(at, pin.clone());
                    if at <= list.confirmed {
                        list.confirmed += 1;
                    }
                }
                return;
            };
            if update.pinned == Some(false) {
                list.remove(index);
                return;
            }
            let pin = &mut list.items[index];
            let page_is_newer = match (update.edited_timestamp, pin.message.edited_timestamp) {
                (Some(Some(edit)), Some(shown)) => shown > edit,
                _ => false,
            };
            if !page_is_newer {
                pin.message = Arc::new(
                    pin.message
                        .patch(*update, &mut |user| intern(user, users, &[])),
                );
            }
        }
    }
}
