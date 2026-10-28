use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::Arc;

use super::{Entities, State};
use crate::model::{self, ChannelId, ChannelType, GuildId, Permissions, ThreadSearchResults};
use crate::state::{Channel, OlderThreads, StoreEvent, ThreadBrowser};

/// How many archived threads a page asks for, as the official client does.
pub(crate) const OLDER_PAGE: u32 = 25;

/// A page of a channel's archived threads being loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OlderPage {
    pub(crate) offset: u32,
    pub(super) ticket: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OlderPages {
    pub(super) state: OlderThreads,
    pub(super) offset: u32,
    // The oldest thread the pages brought, by `newest`.
    pub(super) oldest: Option<u64>,
    // The last page asked for. READY and GUILD_CREATE start the pages over, so an answer whose
    // ticket isn't this one belongs to pages that are gone.
    pub(super) ticket: u64,
}

impl Default for OlderPages {
    fn default() -> Self {
        Self {
            state: OlderThreads::NotLoaded,
            offset: 0,
            oldest: None,
            ticket: 0,
        }
    }
}

// Discord orders threads by their newest message; one without counts from its own ID.
pub(super) fn newest(channel: &Channel) -> u64 {
    channel
        .last_message_id
        .map_or(channel.id.get(), |id| id.get())
}

pub(super) fn newest_on_wire(thread: &model::Channel) -> u64 {
    thread
        .last_message_id
        .map_or(thread.id.get(), |id| id.get())
}

impl State {
    pub(crate) fn thread_browser(&self, parent: ChannelId) -> ThreadBrowser {
        let now = self.now_millis();
        let pages = self.older.get(&parent).copied().unwrap_or_default();
        let cut = match pages.state {
            OlderThreads::All => Some(0),
            _ => pages.oldest,
        };
        let moderates = self
            .permissions(parent, now)
            .is_some_and(|permissions| permissions.contains(Permissions::MANAGE_THREADS));
        let mut browser = ThreadBrowser {
            joined: Vec::new(),
            others: Vec::new(),
            archived: Vec::new(),
            older: pages.state,
        };
        for id in self.threads.of(parent) {
            let Some(channel) = self.entities.channels.get(&id) else {
                continue;
            };
            let Some(info) = channel.thread.as_deref() else {
                continue;
            };
            let viewable = self
                .permissions(id, now)
                .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL));
            if !viewable
                || (channel.kind == ChannelType::PrivateThread && !info.joined() && !moderates)
            {
                continue;
            }
            // A READY may have kept it after it was deleted or unarchived meanwhile.
            if self.unconfirmed.contains(&id) {
                continue;
            }
            if info.archived {
                if cut.is_some_and(|cut| newest(channel) >= cut) {
                    browser.archived.push(channel.clone());
                }
            } else {
                if info.joined() {
                    browser.joined.push(channel.clone());
                } else {
                    browser.others.push(channel.clone());
                }
            }
        }
        for section in [
            &mut browser.joined,
            &mut browser.others,
            &mut browser.archived,
        ] {
            section.sort_by_key(|channel| Reverse((newest(channel), channel.id)));
        }
        browser
    }

    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn set_browsing(&mut self, parent: Option<ChannelId>) {
        self.browsing = parent;
    }

    pub(crate) fn can_load_older_threads(&self, parent: ChannelId) -> bool {
        let pages = self.older.get(&parent).copied().unwrap_or_default();
        matches!(pages.state, OlderThreads::NotLoaded | OlderThreads::More)
    }

    pub(crate) fn begin_older_threads(
        &mut self,
        parent: ChannelId,
        events: &mut Vec<StoreEvent>,
    ) -> Option<OlderPage> {
        let pages = self.older.entry(parent).or_default();
        if !matches!(pages.state, OlderThreads::NotLoaded | OlderThreads::More) {
            return None;
        }
        self.page_tickets += 1;
        pages.state = OlderThreads::Loading;
        pages.ticket = self.page_tickets;
        events.push(StoreEvent::OlderThreadsChanged { parent_id: parent });
        Some(OlderPage {
            offset: pages.offset,
            ticket: pages.ticket,
        })
    }

    pub(crate) fn finish_older_threads(
        &mut self,
        parent: ChannelId,
        page: OlderPage,
        results: ThreadSearchResults,
        events: &mut Vec<StoreEvent>,
    ) {
        let oldest = results.threads.iter().flatten().map(newest_on_wire).min();
        let more = results.has_more;
        let current = self.is_current(parent, page);
        let found = self.keep_threads_found(results, current, events);
        if !current {
            return;
        }
        let Some(pages) = self.older.get_mut(&parent) else {
            return;
        };
        events.push(StoreEvent::OlderThreadsChanged { parent_id: parent });
        if found.is_none() {
            pages.state = OlderThreads::Failed;
            return;
        }
        pages.offset = page.offset + OLDER_PAGE;
        pages.oldest = match (pages.oldest, oldest) {
            (Some(known), Some(oldest)) => Some(known.min(oldest)),
            (known, oldest) => known.or(oldest),
        };
        pages.state = if more {
            OlderThreads::More
        } else {
            OlderThreads::All
        };
    }

    pub(crate) fn fail_older_threads(
        &mut self,
        parent: ChannelId,
        page: OlderPage,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.is_current(parent, page)
            && let Some(pages) = self.older.get_mut(&parent)
        {
            pages.state = OlderThreads::Failed;
            events.push(StoreEvent::OlderThreadsChanged { parent_id: parent });
        }
    }

    // The request was dropped before Discord answered: the page can be asked for again.
    pub(crate) fn abandon_older_threads(
        &mut self,
        parent: ChannelId,
        page: OlderPage,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.is_current(parent, page) {
            return;
        }
        let Some(pages) = self.older.get_mut(&parent) else {
            return;
        };
        if pages.state == OlderThreads::Loading {
            pages.state = if pages.offset == 0 {
                OlderThreads::NotLoaded
            } else {
                OlderThreads::More
            };
            events.push(StoreEvent::OlderThreadsChanged { parent_id: parent });
        }
    }

    // Keeps what a thread search brought: a thread the store lacks or holds unconfirmed as
    // Discord sends it, else the gateway's copy; owners; latest messages; the user's members of
    // the threads it brought. An answer asked before the last READY confirms and joins nothing.
    // `None` when Discord hadn't indexed the channel. The IDs are the search's order.
    pub(crate) fn keep_found_threads(
        &mut self,
        results: ThreadSearchResults,
        asked: u64,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Vec<ChannelId>> {
        self.keep_threads_found(results, asked == self.generation, events)
    }

    // A page whose channel's pages started over (or were dropped) since it was asked for.
    fn is_current(&self, parent: ChannelId, page: OlderPage) -> bool {
        self.older
            .get(&parent)
            .is_some_and(|pages| pages.ticket == page.ticket)
    }

    pub(super) fn keep_threads_found(
        &mut self,
        results: ThreadSearchResults,
        current: bool,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Vec<ChannelId>> {
        let threads = results.threads?;
        let stale = !current;
        let mut found = Vec::with_capacity(threads.len());
        let mut brought = HashSet::new();
        for mut thread in threads {
            // Its parent was deleted while the answer was on its way.
            if !thread
                .parent_id
                .is_some_and(|parent| self.entities.channels.contains_key(&parent))
            {
                continue;
            }
            if let Some(user) = thread.owner.take().and_then(|owner| owner.user) {
                self.put_user(user, events);
            }
            let id = thread.id;
            if !self.entities.channels.contains_key(&id) || self.unconfirmed.contains(&id) {
                self.put_thread(thread, events);
                if !self.entities.channels.contains_key(&id) {
                    continue;
                }
                if stale {
                    self.unconfirmed.insert(id);
                } else {
                    brought.insert(id);
                }
            }
            found.push(id);
        }
        let me = self.entities.me();
        for member in results.members {
            let Some(thread) = member.id.filter(|thread| brought.contains(thread)) else {
                continue;
            };
            if member.user_id.is_none_or(|user| Some(user) == me) {
                self.set_membership(
                    thread,
                    Some(crate::state::ThreadMembership::from_wire(member)),
                    events,
                );
            }
        }
        for message in &results.most_recent_messages {
            if self.note_latest(message) {
                self.reread_boxes(message.channel_id, events);
            }
        }
        self.settle(self.now_millis(), events);
        Some(found)
    }

    // An unjoined archived thread shows in its own messages, its box, its "started a thread"
    // line or its parent's open Threads browser.
    pub(super) fn shows(&self, thread: &Channel) -> bool {
        let Some(parent) = thread.parent_id else {
            return false;
        };
        self.browsing == Some(parent)
            || self.windows.has(thread.id)
            || self.windows.box_row(parent, thread.id).is_some()
            || self.windows.names_thread(parent, thread.id)
    }

    // GUILD_CREATE lists a guild's joined, active threads as READY does: what it lists is
    // confirmed, what the store kept besides may be stale, and the guild's pages start again.
    pub(super) fn follow_guild_threads(
        &mut self,
        guild: GuildId,
        next: &mut Entities,
        events: &mut Vec<StoreEvent>,
    ) {
        let kept = std::mem::take(&mut next.kept_threads);
        for channel in next.channels.keys() {
            if kept.contains(channel) {
                self.unconfirmed.insert(*channel);
            } else {
                self.unconfirmed.remove(channel);
            }
        }
        let channels = &self.entities.channels;
        let parents: Vec<ChannelId> = self
            .older
            .keys()
            .filter(|parent| {
                next.channels.contains_key(parent)
                    || channels
                        .get(parent)
                        .is_some_and(|channel| channel.guild_id == Some(guild))
            })
            .copied()
            .collect();
        for parent in parents {
            self.older.remove(&parent);
            events.push(StoreEvent::OlderThreadsChanged { parent_id: parent });
        }
    }

    // A READY lists the joined, active threads: the ones the store kept besides may be stale.
    pub(super) fn follow_ready_threads(&mut self) {
        self.unconfirmed = std::mem::take(&mut self.entities.kept_threads);
        self.older.clear();
        self.forum_pages.clear();
        self.generation += 1;
    }
}

pub(super) fn thread_activity(channel: &Arc<Channel>) -> Option<StoreEvent> {
    if !channel.is_thread() {
        return None;
    }
    Some(StoreEvent::ThreadActivity {
        guild_id: channel.guild_id?,
        parent_id: channel.parent_id?,
        thread_id: channel.id,
    })
}
