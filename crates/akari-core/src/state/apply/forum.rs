use std::cmp::Reverse;
use std::collections::HashMap;
use std::sync::Arc;

use super::State;
use super::browser::{OLDER_PAGE, OlderPage, OlderPages, newest, newest_on_wire};
use crate::model::{ChannelId, GuildId, Permissions, TagId, ThreadSearchResults, UserId};
use crate::state::{
    Channel, ForumPosts, ForumSort, ForumView, OlderThreads, StoreEvent, TagMatching,
};

// The official client keeps the pages of at most this many filters.
const KEPT_FILTERS: usize = 50;

/// A forum tag filter's pages: the tags in ID order, Match All only with tags (without, it
/// changes nothing).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct PageKey {
    pub(crate) forum: ChannelId,
    pub(crate) tags: Box<[TagId]>,
    pub(crate) match_all: bool,
}

impl PageKey {
    pub(crate) fn new(forum: ChannelId, view: &ForumView) -> Self {
        let mut tags = view.tags.to_vec();
        tags.sort();
        tags.dedup();
        let match_all = !tags.is_empty() && view.matching == TagMatching::All;
        Self {
            forum,
            tags: tags.into(),
            match_all,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(crate) struct ForumPages {
    pages: OlderPages,
    // When it was last asked for, for keeping the newest filters.
    used: u64,
}

// A forum post pinned to the top of its forum.
const PINNED: u64 = 1 << 1;

/// The open forum's list as it last refreshed: who's in it and the activity each was sorted by.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct ForumList {
    forum: ChannelId,
    posts: HashMap<ChannelId, u64>,
    new_posts: u32,
}

fn pinned(channel: &Channel) -> bool {
    channel.flags & PINNED != 0
}

fn archived(channel: &Channel) -> bool {
    channel.thread.as_deref().is_none_or(|info| info.archived)
}

// Module 207777: an untagged post drops out of a filter.
fn matches(channel: &Channel, view: &ForumView) -> bool {
    if view.tags.is_empty() {
        return true;
    }
    let applied = channel
        .thread
        .as_deref()
        .map_or(&[][..], |info| &info.applied_tags[..]);
    match view.matching {
        TagMatching::Some => view.tags.iter().any(|tag| applied.contains(tag)),
        TagMatching::All => view.tags.iter().all(|tag| applied.contains(tag)),
    }
}

impl State {
    /// The forum whose page shows, `None` once it closes: opening one refreshes its list.
    pub(crate) fn open_forum(&mut self, forum: Option<ChannelId>, events: &mut Vec<StoreEvent>) {
        match forum {
            Some(forum) => self.refresh_forum(forum, events),
            None => self.forum_list = None,
        }
    }

    /// The open forum's list as the store has it now; the new-post count starts over.
    pub(crate) fn refresh_forum(&mut self, forum: ChannelId, events: &mut Vec<StoreEvent>) {
        let posts = self
            .listable_posts(forum)
            .map(|post| (post.id, newest(post)))
            .collect();
        self.forum_list = Some(ForumList {
            forum,
            posts,
            new_posts: 0,
        });
        events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
    }

    // The forum's active posts the user may see.
    fn listable_posts(&self, forum: ChannelId) -> impl Iterator<Item = &Arc<Channel>> {
        let now = self.now_millis();
        self.threads
            .of(forum)
            .filter_map(|id| self.entities.channels.get(&id))
            .filter(move |post| {
                !archived(post)
                    // A READY may have kept it after it was deleted or archived meanwhile.
                    && !self.unconfirmed.contains(&post.id)
                    && self
                        .permissions(post.id, now)
                        .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
            })
    }

    pub(crate) fn forum_posts(&self, forum: ChannelId, view: &ForumView) -> ForumPosts {
        let open = self.forum_list.as_ref().filter(|list| list.forum == forum);
        let mut active: Vec<(Arc<Channel>, u64)> = match open {
            Some(list) => list
                .posts
                .iter()
                .filter_map(|(id, newest)| {
                    let post = self.entities.channels.get(id)?;
                    (!archived(post)).then(|| (post.clone(), *newest))
                })
                .collect(),
            None => self
                .listable_posts(forum)
                .map(|post| (post.clone(), newest(post)))
                .collect(),
        };
        active.retain(|(post, _)| matches(post, view));
        active.sort_by_key(|(post, newest)| {
            let order = match view.sort {
                ForumSort::LatestActivity => *newest,
                ForumSort::CreationDate => post.id.get(),
            };
            (Reverse(pinned(post)), Reverse(order), Reverse(post.id))
        });
        let pages = self
            .forum_pages
            .get(&PageKey::new(forum, view))
            .map(|pages| pages.pages)
            .unwrap_or_default();
        let cut = match pages.state {
            OlderThreads::All => Some(0),
            _ => pages.oldest,
        };
        let now = self.now_millis();
        let mut older: Vec<Arc<Channel>> = self
            .threads
            .of(forum)
            .filter_map(|id| self.entities.channels.get(&id))
            .filter(|post| {
                archived(post)
                    && cut.is_some_and(|cut| newest(post) >= cut)
                    && matches(post, view)
                    && self
                        .permissions(post.id, now)
                        .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
            })
            .cloned()
            .collect();
        older.sort_by_key(|post| {
            let order = match view.sort {
                ForumSort::LatestActivity => newest(post),
                ForumSort::CreationDate => post.id.get(),
            };
            (Reverse(order), Reverse(post.id))
        });
        ForumPosts {
            active: active.into_iter().map(|(post, _)| post).collect(),
            archived: older,
            older: pages.state,
            new_posts: open.map_or(0, |list| list.new_posts),
        }
    }

    pub(crate) fn can_load_older_posts(&self, key: &PageKey) -> bool {
        let pages = self
            .forum_pages
            .get(key)
            .map(|pages| pages.pages)
            .unwrap_or_default();
        matches!(pages.state, OlderThreads::NotLoaded | OlderThreads::More)
    }

    pub(crate) fn begin_older_posts(
        &mut self,
        key: PageKey,
        events: &mut Vec<StoreEvent>,
    ) -> Option<OlderPage> {
        if !self.can_load_older_posts(&key) {
            return None;
        }
        self.page_tickets += 1;
        let ticket = self.page_tickets;
        if !self.forum_pages.contains_key(&key) && self.forum_pages.len() >= KEPT_FILTERS {
            let oldest = self
                .forum_pages
                .iter()
                .filter(|(_, pages)| pages.pages.state != OlderThreads::Loading)
                .min_by_key(|(_, pages)| pages.used)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                self.forum_pages.remove(&oldest);
            }
        }
        let forum = key.forum;
        let pages = self.forum_pages.entry(key).or_default();
        pages.pages.state = OlderThreads::Loading;
        pages.pages.ticket = ticket;
        pages.used = ticket;
        events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
        Some(OlderPage {
            offset: pages.pages.offset,
            ticket,
        })
    }

    pub(crate) fn finish_older_posts(
        &mut self,
        key: &PageKey,
        page: OlderPage,
        results: ThreadSearchResults,
        events: &mut Vec<StoreEvent>,
    ) {
        let oldest = results.threads.iter().flatten().map(newest_on_wire).min();
        let more = results.has_more;
        let current = self.is_current_post_page(key, page);
        let found = self.keep_threads_found(results, current, events);
        if !current {
            return;
        }
        let Some(pages) = self.forum_pages.get_mut(key) else {
            return;
        };
        events.push(StoreEvent::ForumPostsChanged {
            forum_id: key.forum,
        });
        let pages = &mut pages.pages;
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

    pub(crate) fn fail_older_posts(
        &mut self,
        key: &PageKey,
        page: OlderPage,
        events: &mut Vec<StoreEvent>,
    ) {
        if self.is_current_post_page(key, page)
            && let Some(pages) = self.forum_pages.get_mut(key)
        {
            pages.pages.state = OlderThreads::Failed;
            events.push(StoreEvent::ForumPostsChanged {
                forum_id: key.forum,
            });
        }
    }

    // The request was dropped before Discord answered: the page can be asked for again.
    pub(crate) fn abandon_older_posts(
        &mut self,
        key: &PageKey,
        page: OlderPage,
        events: &mut Vec<StoreEvent>,
    ) {
        if !self.is_current_post_page(key, page) {
            return;
        }
        let Some(pages) = self.forum_pages.get_mut(key) else {
            return;
        };
        let pages = &mut pages.pages;
        if pages.state == OlderThreads::Loading {
            pages.state = if pages.offset == 0 {
                OlderThreads::NotLoaded
            } else {
                OlderThreads::More
            };
            events.push(StoreEvent::ForumPostsChanged {
                forum_id: key.forum,
            });
        }
    }

    fn is_current_post_page(&self, key: &PageKey, page: OlderPage) -> bool {
        self.forum_pages
            .get(key)
            .is_some_and(|pages| pages.pages.ticket == page.ticket)
    }

    // THREAD_CREATE: someone else's new post waits for the pill; the user's own shows at once.
    pub(super) fn forum_post_created(
        &mut self,
        post: ChannelId,
        parent: Option<ChannelId>,
        owner: Option<UserId>,
        newly_created: bool,
        events: &mut Vec<StoreEvent>,
    ) {
        if !newly_created {
            return;
        }
        let mine = owner.is_some() && owner == self.entities.me();
        let activity = self.entities.channels.get(&post).map(|post| newest(post));
        let Some(list) = self
            .forum_list
            .as_mut()
            .filter(|list| Some(list.forum) == parent)
        else {
            return;
        };
        if mine {
            if let Some(activity) = activity {
                list.posts.insert(post, activity);
            }
        } else {
            list.new_posts += 1;
        }
        events.push(StoreEvent::ForumPostsChanged {
            forum_id: list.forum,
        });
    }

    pub(super) fn forum_post_deleted(&mut self, post: ChannelId, events: &mut Vec<StoreEvent>) {
        if let Some(list) = self.forum_list.as_mut()
            && list.posts.remove(&post).is_some()
        {
            events.push(StoreEvent::ForumPostsChanged {
                forum_id: list.forum,
            });
        }
    }

    // A pin changed: the kept posts are sorted again by what they are now, a newly pinned post
    // joins them.
    pub(super) fn forum_post_updated(
        &mut self,
        before: &Channel,
        after: &Channel,
        events: &mut Vec<StoreEvent>,
    ) {
        if pinned(before) == pinned(after) {
            return;
        }
        let Some(forum) = self
            .forum_list
            .as_ref()
            .map(|list| list.forum)
            .filter(|forum| after.parent_id == Some(*forum))
        else {
            return;
        };
        let current: HashMap<ChannelId, u64> = self
            .listable_posts(forum)
            .map(|post| (post.id, newest(post)))
            .collect();
        let Some(list) = self.forum_list.as_mut() else {
            return;
        };
        if pinned(after) && current.contains_key(&after.id) {
            list.posts.insert(after.id, 0);
        }
        for (id, activity) in &mut list.posts {
            if let Some(now) = current.get(id) {
                *activity = *now;
            }
        }
        events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
    }

    // THREAD_LIST_SYNC for the open forum's guild, or a new connection (`None`): the list
    // refreshes; a forum that's gone closes.
    pub(super) fn refresh_open_forum(
        &mut self,
        guild: Option<GuildId>,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(forum) = self.forum_list.as_ref().map(|list| list.forum) else {
            return;
        };
        let Some(channel) = self.entities.channels.get(&forum) else {
            self.forum_list = None;
            return;
        };
        if guild.is_some_and(|guild| channel.guild_id != Some(guild)) {
            return;
        }
        self.refresh_forum(forum, events);
    }
}
