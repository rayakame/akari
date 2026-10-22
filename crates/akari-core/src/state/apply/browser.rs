use std::cmp::Reverse;
use std::sync::Arc;

use super::State;
use crate::model::{self, ChannelId, ChannelType, Permissions, ThreadSearchResults};
use crate::state::{Channel, OlderThreads, StoreEvent, ThreadBrowser};

/// How many archived threads a page asks for, as the official client does.
pub(crate) const OLDER_PAGE: u32 = 25;

/// A page of a channel's archived threads being loaded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OlderPage {
    pub(crate) offset: u32,
    // The READY it was asked in; a later one clears the pages.
    generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct OlderPages {
    state: OlderThreads,
    offset: u32,
    // The oldest thread the pages brought, by `newest`.
    oldest: Option<u64>,
}

impl Default for OlderPages {
    fn default() -> Self {
        Self {
            state: OlderThreads::NotLoaded,
            offset: 0,
            oldest: None,
        }
    }
}

// Discord orders threads by their newest message; one without counts from its own ID.
fn newest(channel: &Channel) -> u64 {
    channel
        .last_message_id
        .map_or(channel.id.get(), |id| id.get())
}

fn newest_on_wire(thread: &model::Channel) -> u64 {
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
            if info.archived {
                if cut.is_some_and(|cut| newest(channel) >= cut) {
                    browser.archived.push(channel.clone());
                }
            } else if !self.unconfirmed.contains(&id) {
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

    pub(crate) fn begin_older_threads(&mut self, parent: ChannelId) -> Option<OlderPage> {
        let generation = self.older_generation;
        let pages = self.older.entry(parent).or_default();
        if !matches!(pages.state, OlderThreads::NotLoaded | OlderThreads::More) {
            return None;
        }
        pages.state = OlderThreads::Loading;
        Some(OlderPage {
            offset: pages.offset,
            generation,
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
        let found = self.keep_found_threads(results, events);
        if page.generation != self.older_generation {
            return;
        }
        let pages = self.older.entry(parent).or_default();
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

    pub(crate) fn fail_older_threads(&mut self, parent: ChannelId, page: OlderPage) {
        if page.generation == self.older_generation {
            self.older.entry(parent).or_default().state = OlderThreads::Failed;
        }
    }

    // Keeps what a thread search brought: a thread the store lacks or holds unconfirmed as
    // Discord sends it, else the gateway's copy; the user's members; owners; latest messages.
    // `None` when Discord hadn't indexed the channel. The IDs are the search's order.
    pub(crate) fn keep_found_threads(
        &mut self,
        results: ThreadSearchResults,
        events: &mut Vec<StoreEvent>,
    ) -> Option<Vec<ChannelId>> {
        let threads = results.threads?;
        let mut found = Vec::with_capacity(threads.len());
        for mut thread in threads {
            if let Some(user) = thread.owner.take().and_then(|owner| owner.user) {
                self.put_user(user, events);
            }
            let id = thread.id;
            if !self.entities.channels.contains_key(&id) || self.unconfirmed.contains(&id) {
                self.put_thread(thread, events);
            }
            if self.entities.channels.contains_key(&id) {
                found.push(id);
            }
        }
        let me = self.entities.me();
        for member in results.members {
            let Some(thread) = member.id else {
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

    // A READY lists the joined threads; the unjoined ones the store keeps may be stale.
    pub(super) fn follow_ready_threads(&mut self) {
        self.unconfirmed = self
            .entities
            .channels
            .values()
            .filter(|channel| channel.thread.as_deref().is_some_and(|info| !info.joined()))
            .map(|channel| channel.id)
            .collect();
        self.older.clear();
        self.older_generation += 1;
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
