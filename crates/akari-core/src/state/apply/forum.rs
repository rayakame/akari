use std::cmp::Reverse;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use super::State;
use super::boxes::known_user;
use super::browser::{OLDER_PAGE, OlderPage, OlderPages, newest, newest_on_wire};
use crate::gateway::MessageUpdate;
use crate::model::{
    self, ChannelId, GuildId, MessageId, Permissions, PostData, TagId, ThreadSearchResults, UserId,
};
use crate::state::{
    Channel, ForumPosts, ForumSort, ForumView, Message, OlderThreads, PostPreview, StoreEvent,
    TagMatching,
};

// Module 473503 asks at most this many posts a request.
const POST_BATCH: usize = 10;
// The official client asks the first 10 posts' data as the user presses the forum's row
// (module 473503).
const FIRST_POSTS: usize = 10;
// A post Discord didn't answer for is asked again after this, while its card shows (the official
// client asks again on the card's next render).
const RETRY_MILLIS: i64 = 30_000;

/// Forum posts' first messages from post-data and searches, and what's asked.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct Previews {
    // `None`: Discord said the first message is gone.
    messages: HashMap<ChannelId, Option<Arc<Message>>>,
    queue: VecDeque<(ChannelId, ChannelId)>,
    // Queued or being asked.
    asked: HashSet<ChannelId>,
    // When a post Discord didn't answer for may be asked again.
    retry: HashMap<ChannelId, i64>,
}

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

/// The open forum's list as it last refreshed: who's in it and the activity each was sorted by.
#[derive(Debug)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct ForumList {
    forum: ChannelId,
    posts: HashMap<ChannelId, u64>,
    new_posts: u32,
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
    /// Returns its first 10 posts as its default view shows them.
    pub(crate) fn open_forum(
        &mut self,
        forum: Option<ChannelId>,
        events: &mut Vec<StoreEvent>,
    ) -> Vec<ChannelId> {
        let Some(forum) = forum else {
            self.forum_list = None;
            return Vec::new();
        };
        // Another forum's cards are gone; reopening this one keeps its own.
        let channels = &self.entities.channels;
        let of_forum = |post: &ChannelId| {
            channels
                .get(post)
                .is_some_and(|post| post.parent_id == Some(forum))
        };
        self.previews.messages.retain(|post, _| of_forum(post));
        self.previews.retry.retain(|post, _| of_forum(post));
        self.list_forum(forum, events);
        let view = self
            .entities
            .channels
            .get(&forum)
            .and_then(|channel| channel.forum.as_deref())
            .map(|settings| ForumView::new(settings.sort, Box::default(), settings.tag_matching))
            .unwrap_or_default();
        self.forum_posts(forum, &view)
            .active
            .iter()
            .take(FIRST_POSTS)
            .map(|post| post.id)
            .collect()
    }

    /// The open forum's list as the store has it now; the new-post count starts over. A forum
    /// that isn't open changes nothing.
    pub(crate) fn refresh_forum(&mut self, forum: ChannelId, events: &mut Vec<StoreEvent>) {
        if self
            .forum_list
            .as_ref()
            .is_some_and(|list| list.forum == forum)
        {
            self.list_forum(forum, events);
        }
    }

    fn list_forum(&mut self, forum: ChannelId, events: &mut Vec<StoreEvent>) {
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
            (Reverse(post.is_pinned()), Reverse(order), Reverse(post.id))
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
                    // A READY or GUILD_CREATE may have kept it after it went meanwhile.
                    && !self.unconfirmed.contains(&post.id)
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

    pub(super) fn forum_post_deleted(&mut self, post: &Channel, events: &mut Vec<StoreEvent>) {
        if let Some(list) = self.forum_list.as_mut()
            && (list.posts.remove(&post.id).is_some() || post.parent_id == Some(list.forum))
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
        if before.is_pinned() == after.is_pinned() {
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
        if after.is_pinned() && current.contains_key(&after.id) {
            list.posts.insert(after.id, 0);
        }
        for (id, activity) in &mut list.posts {
            if let Some(now) = current.get(id) {
                *activity = *now;
            }
        }
        events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
    }

    // THREAD_LIST_SYNC for the open forum's guild, which also follows each new connection: the
    // list refreshes; a forum that's gone closes.
    pub(super) fn refresh_open_forum(&mut self, guild: GuildId, events: &mut Vec<StoreEvent>) {
        let Some(forum) = self.forum_list.as_ref().map(|list| list.forum) else {
            return;
        };
        let Some(channel) = self.entities.channels.get(&forum) else {
            self.forum_list = None;
            return;
        };
        if channel.guild_id != Some(guild) {
            return;
        }
        self.list_forum(forum, events);
    }

    /// A post's first message: what post-data, a search or its own messages brought, else its
    /// window's; `None` until one of them has it.
    pub(crate) fn post_preview(&self, post: ChannelId) -> Option<PostPreview> {
        if let Some(known) = self.previews.messages.get(&post) {
            return Some(
                known
                    .clone()
                    .map_or(PostPreview::Deleted, PostPreview::Loaded),
            );
        }
        self.windows
            .message(post, post.cast())
            .map(PostPreview::Loaded)
    }

    /// Queues the posts whose cards show without a first message, unless asked already or failed
    /// within the last 30 s; `true` when one was queued.
    pub(crate) fn want_post_previews(
        &mut self,
        forum: ChannelId,
        posts: &[ChannelId],
        _events: &mut Vec<StoreEvent>,
    ) -> bool {
        let now = self.now_millis();
        let mut queued = false;
        for post in posts {
            if self.post_preview(*post).is_some()
                || self.previews.asked.contains(post)
                || self
                    .previews
                    .retry
                    .get(post)
                    .is_some_and(|until| *until > now)
            {
                continue;
            }
            self.previews.asked.insert(*post);
            self.previews.queue.push_back((forum, *post));
            queued = true;
        }
        queued
    }

    /// The next request: up to 10 posts of the first forum waiting.
    pub(crate) fn next_post_batch(&mut self) -> Option<(ChannelId, Vec<ChannelId>)> {
        let (forum, _) = *self.previews.queue.front()?;
        let mut batch = Vec::with_capacity(POST_BATCH);
        self.previews.queue.retain(|(queued, post)| {
            if *queued == forum && batch.len() < POST_BATCH {
                batch.push(*post);
                false
            } else {
                true
            }
        });
        Some((forum, batch))
    }

    /// What post-data answered for `asked`, or `None` when the request failed.
    pub(crate) fn finish_post_batch(
        &mut self,
        _forum: ChannelId,
        asked: &[ChannelId],
        answer: Option<PostData>,
        events: &mut Vec<StoreEvent>,
    ) {
        let mut threads = answer.map(|answer| answer.threads).unwrap_or_default();
        let retry = self.now_millis() + RETRY_MILLIS;
        for post in asked {
            self.previews.asked.remove(post);
            let Some(entry) = threads.remove(post) else {
                self.previews.retry.insert(*post, retry);
                continue;
            };
            if let Some(user) = entry.owner.and_then(|owner| owner.user) {
                self.put_user(user, events);
            }
            let users = &self.entities.users;
            let message = entry.first_message.map(|message| {
                Arc::new(Message::from_wire(message, &mut |user| {
                    known_user(user, users)
                }))
            });
            self.previews.messages.insert(*post, message);
            self.previews.retry.remove(post);
            events.push(StoreEvent::PostPreviewChanged { post_id: *post });
        }
    }

    // A forum post's first message has the post's ID.
    // The cards that show, and those known already: other forums ask when they open.
    fn keeps_first_message(&self, post: ChannelId) -> bool {
        if self.previews.messages.contains_key(&post) {
            return true;
        }
        let parent = self
            .entities
            .channels
            .get(&post)
            .and_then(|post| post.parent_id);
        parent.is_some_and(|parent| {
            self.forum_list
                .as_ref()
                .is_some_and(|list| list.forum == parent)
        })
    }

    pub(super) fn forget_first_message(&mut self, post: ChannelId) {
        self.previews.messages.remove(&post);
        self.previews.retry.remove(&post);
    }

    pub(super) fn note_first_message(
        &mut self,
        message: &model::Message,
        events: &mut Vec<StoreEvent>,
    ) {
        if message.id.get() != message.channel_id.get()
            || !self.keeps_first_message(message.channel_id)
        {
            return;
        }
        let users = &self.entities.users;
        let first = Message::from_wire(message.clone(), &mut |user| known_user(user, users));
        self.previews
            .messages
            .insert(message.channel_id, Some(Arc::new(first)));
        self.previews.retry.remove(&message.channel_id);
        events.push(StoreEvent::PostPreviewChanged {
            post_id: message.channel_id,
        });
    }

    pub(super) fn update_first_message(
        &mut self,
        update: &MessageUpdate,
        events: &mut Vec<StoreEvent>,
    ) {
        let Some(Some(known)) = self.previews.messages.get(&update.channel_id) else {
            return;
        };
        if known.id != update.id {
            return;
        }
        let users = &self.entities.users;
        let next = known.patch(update.clone(), &mut |user| known_user(user, users));
        if next != **known {
            self.previews
                .messages
                .insert(update.channel_id, Some(Arc::new(next)));
            events.push(StoreEvent::PostPreviewChanged {
                post_id: update.channel_id,
            });
        }
    }

    pub(super) fn delete_first_message(
        &mut self,
        channel: ChannelId,
        message: MessageId,
        events: &mut Vec<StoreEvent>,
    ) {
        if message.get() != channel.get() || !self.keeps_first_message(channel) {
            return;
        }
        self.previews.messages.insert(channel, None);
        events.push(StoreEvent::PostPreviewChanged { post_id: channel });
    }
}
