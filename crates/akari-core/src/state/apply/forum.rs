use std::cmp::Reverse;
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use super::State;
use super::boxes::known_user;
use super::browser::{OLDER_PAGE, OlderPage, OlderPages, newest, newest_on_wire};
use crate::gateway::{FORUM_UNREADS_LIMIT, ForumUnreads, MessageUpdate};
use crate::model::{
    self, ChannelId, GuildId, MessageId, Permissions, PostData, TagId, ThreadSearchResults, UserId,
};
use crate::state::{
    Channel, ForumPosts, ForumSort, ForumView, Message, OlderThreads, PostNewCount, PostPreview,
    StoreEvent, TagMatching,
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
    // First messages edited while being asked for, with the newest edit's time (`None`: an
    // update that named none): an answer from before it is stale.
    edited: HashMap<ChannelId, Option<i64>>,
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
    // The forum's read position as this visit began, before the visit moved it to "now": the
    // official client's `ackMessageIdAtChannelSelect`. `None` without an ack or a join time.
    read_before: Option<u64>,
}

/// Forum posts' counts of messages past the user's ack, from op 28 and what came since.
#[derive(Debug, Default)]
#[cfg_attr(test, derive(PartialEq))]
pub(super) struct PostCounts {
    counts: HashMap<ChannelId, u32>,
    // The ack each post was asked with in this connection, until its ack moves on.
    asked: HashMap<ChannelId, MessageId>,
    // The frames Discord hasn't answered, oldest first: it answers each once, in the order asked.
    unanswered: VecDeque<Frame>,
    // Between the connection's loss and RESUMED, when a resume replays dispatches.
    resuming: bool,
    // Forums whose posts' counts the current write changed.
    changed: BTreeSet<ChannelId>,
}

#[derive(Debug, PartialEq)]
struct Frame {
    ask: ForumUnreadsAsk,
    sent: Instant,
}

// How long a frame's answer is waited for.
const FORUM_UNREADS_WAIT: Duration = Duration::from_secs(30);

/// An op 28 to send: posts of the open forum with the acks to count from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ForumUnreadsAsk {
    pub(crate) guild: GuildId,
    pub(crate) forum: ChannelId,
    pub(crate) threads: Vec<(ChannelId, MessageId)>,
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
        self.begin_forum_visit(events);
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
        // A refresh keeps what the visit began with.
        let read_before = self
            .forum_list
            .as_ref()
            .filter(|list| list.forum == forum)
            .and_then(|list| list.read_before);
        self.forum_list = Some(ForumList {
            forum,
            posts,
            new_posts: 0,
            read_before,
        });
        events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
    }

    // Takes the open forum's read position for its NEW badges: when it opens, and with a
    // session's first READY, whose ack a forum shown from the disk cache hadn't yet.
    pub(super) fn begin_forum_visit(&mut self, events: &mut Vec<StoreEvent>) {
        let Some(forum) = self.forum_list.as_ref().map(|list| list.forum) else {
            return;
        };
        let read_before = self.forum_read_position(forum);
        if let Some(list) = self.forum_list.as_mut()
            && list.read_before != read_before
        {
            list.read_before = read_before;
            events.push(StoreEvent::ForumPostsChanged { forum_id: forum });
        }
    }

    // The open forum has no position yet (opened before the store had it): takes it once it
    // can, before the forum is read.
    pub(super) fn find_forum_position(&mut self, forum: ChannelId, events: &mut Vec<StoreEvent>) {
        if self
            .forum_list
            .as_ref()
            .is_some_and(|list| list.forum == forum && list.read_before.is_none())
        {
            self.begin_forum_visit(events);
        }
    }

    // The forum's ack, else the user's join to its server as a message ID.
    fn forum_read_position(&self, forum: ChannelId) -> Option<u64> {
        self.entities
            .read_states
            .get(forum)
            .and_then(|state| state.acked)
            .map(MessageId::get)
            .or_else(|| {
                let guild = self.entities.channels.get(&forum)?.guild_id?;
                let joined = self.entities.members.get(&guild)?.joined_at?;
                Some(MessageId::from_unix_millis(joined.unix_millis(), 0).get())
            })
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
        let now = self.now_millis();
        let visible = |post: &Channel| {
            self.permissions(post.id, now)
                .is_some_and(|permissions| permissions.contains(Permissions::VIEW_CHANNEL))
        };
        let mut active: Vec<(Arc<Channel>, u64)> = match open {
            // The frozen order stays; a post the user can no longer see goes.
            Some(list) => list
                .posts
                .iter()
                .filter_map(|(id, newest)| {
                    let post = self.entities.channels.get(id)?;
                    (!archived(post) && visible(post)).then(|| (post.clone(), *newest))
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
                    && visible(post)
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
        // Module 474399: a post made while its forum shows is counted from the start.
        self.post_counts.counts.entry(post).or_insert(0);
        events.push(StoreEvent::ForumPostsChanged {
            forum_id: list.forum,
        });
    }

    pub(super) fn forum_post_deleted(&mut self, post: &Channel, events: &mut Vec<StoreEvent>) {
        self.forget_post_count(post.id);
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

    /// Of `posts`, the active ones with messages past where the user stopped reading.
    pub(crate) fn unread_posts(&self, posts: &[ChannelId]) -> Vec<ChannelId> {
        posts
            .iter()
            .copied()
            .filter(|post| {
                self.entities.channels.get(post).is_some_and(|channel| {
                    channel.thread.as_deref().is_some_and(|info| !info.archived)
                        && self.entities.unread(channel)
                })
            })
            .collect()
    }

    // Module 218461 `isNewForumThread`; the user's own post counts as opened there.
    pub(crate) fn new_posts(&self, posts: &[ChannelId]) -> Vec<ChannelId> {
        let Some((list, since)) = self
            .forum_list
            .as_ref()
            .and_then(|list| Some((list, list.read_before?)))
        else {
            return Vec::new();
        };
        let channels = &self.entities.channels;
        let Some(joined) = channels
            .get(&list.forum)
            .and_then(|forum| self.entities.members.get(&forum.guild_id?)?.joined_at)
            // The last ID of the join's millisecond: Discord compares times.
            .map(|joined| {
                MessageId::from_unix_millis(joined.unix_millis() + 1, 0)
                    .get()
                    .saturating_sub(1)
            })
        else {
            return Vec::new();
        };
        let me = self.me();
        posts
            .iter()
            .copied()
            .filter(|id| {
                let Some(post) = channels.get(id) else {
                    return false;
                };
                let opened = self
                    .entities
                    .read_states
                    .get(post.id)
                    .is_some_and(|state| !state.estimated);
                post.parent_id == Some(list.forum)
                    && !archived(post)
                    && id.get() > since.max(joined)
                    && !opened
                    && post.owner_id.is_none_or(|owner| Some(owner) != me)
            })
            .collect()
    }

    pub(crate) fn preview_message(&self, post: ChannelId, id: MessageId) -> Option<Arc<Message>> {
        self.previews
            .messages
            .get(&post)?
            .clone()
            .filter(|message| message.id == id)
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
            if !self.wants_post_preview(*post, now) {
                continue;
            }
            self.previews.asked.insert(*post);
            self.previews.queue.push_back((forum, *post));
            queued = true;
        }
        queued
    }

    pub(crate) fn wants_post_previews(&self, posts: &[ChannelId]) -> bool {
        let now = self.now_millis();
        posts.iter().any(|post| self.wants_post_preview(*post, now))
    }

    fn wants_post_preview(&self, post: ChannelId, now: i64) -> bool {
        self.post_preview(post).is_none()
            && !self.previews.asked.contains(&post)
            && self
                .previews
                .retry
                .get(&post)
                .is_none_or(|until| *until <= now)
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
        forum: ChannelId,
        asked: &[ChannelId],
        answer: Option<PostData>,
        events: &mut Vec<StoreEvent>,
    ) {
        let mut threads = answer.map(|answer| answer.threads).unwrap_or_default();
        let retry = self.now_millis() + RETRY_MILLIS;
        for post in asked {
            self.previews.asked.remove(post);
            let edited = self.previews.edited.remove(post);
            // What the gateway brought while the request was on its way is newer.
            if self.previews.messages.contains_key(post) {
                self.previews.retry.remove(post);
                continue;
            }
            let Some(entry) = threads.remove(post) else {
                self.previews.retry.insert(*post, retry);
                continue;
            };
            if let Some(user) = entry.owner.and_then(|owner| owner.user) {
                self.put_user(user, events);
            }
            // Edited meanwhile: only an answer that has the edit is shown, else asked again.
            if let (Some(seen), Some(first)) = (edited, &entry.first_message) {
                let answered = first.edited_timestamp.map(|at| at.unix_millis());
                if seen.is_none_or(|seen| answered.is_none_or(|answered| answered < seen)) {
                    self.previews.asked.insert(*post);
                    self.previews.queue.push_back((forum, *post));
                    continue;
                }
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
            if update.id.get() == update.channel_id.get()
                && self.previews.asked.contains(&update.channel_id)
                && !self.previews.messages.contains_key(&update.channel_id)
            {
                let at = update.edited_timestamp.flatten().map(|at| at.unix_millis());
                self.previews
                    .edited
                    .entry(update.channel_id)
                    .and_modify(|seen| *seen = seen.zip(at).map(|(seen, at)| seen.max(at)))
                    .or_insert(at);
            }
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

    // Module 767581: an active post that is unread, opened (a read state of the user's own) and
    // acked; the ack is what its count starts after.
    fn counted_from(&self, post: &Channel) -> Option<MessageId> {
        if archived(post) || !self.entities.unread(post) {
            return None;
        }
        self.entities
            .read_states
            .get(post.id)
            .filter(|state| !state.estimated)?
            .acked
    }

    fn forum_of(&self, post: ChannelId) -> Option<ChannelId> {
        let parent = self.entities.channels.get(&post)?.parent_id?;
        self.entities
            .channels
            .get(&parent)
            .is_some_and(|parent| parent.is_forum())
            .then_some(parent)
    }

    fn is_open_forum(&self, forum: ChannelId) -> bool {
        self.forum_list
            .as_ref()
            .is_some_and(|list| list.forum == forum)
    }

    /// The open forum's posts to ask Discord's counts for, each once a connection, marked as
    /// asked; `None` when there are none.
    pub(crate) fn next_forum_unreads(&mut self) -> Option<ForumUnreadsAsk> {
        self.give_up_forum_unreads();
        let forum = self.forum_list.as_ref()?.forum;
        let guild = self.entities.channels.get(&forum)?.guild_id?;
        let counts = &self.post_counts;
        let mut posts: Vec<(u64, ChannelId, MessageId)> = self
            .listable_posts(forum)
            .filter(|post| {
                !counts.counts.contains_key(&post.id) && !counts.asked.contains_key(&post.id)
            })
            .filter_map(|post| Some((newest(post), post.id, self.counted_from(post)?)))
            .collect();
        if posts.is_empty() {
            return None;
        }
        posts.sort_unstable_by_key(|(newest, post, _)| (Reverse(*newest), Reverse(*post)));
        posts.truncate(FORUM_UNREADS_LIMIT);
        let threads: Vec<(ChannelId, MessageId)> = posts
            .into_iter()
            .map(|(_, post, ack)| (post, ack))
            .collect();
        self.post_counts.asked.extend(threads.iter().copied());
        let ask = ForumUnreadsAsk {
            guild,
            forum,
            threads,
        };
        self.post_counts.unanswered.push_back(Frame {
            ask: ask.clone(),
            sent: Instant::now(),
        });
        Some(ask)
    }

    /// The frame never left: its posts may be asked again.
    pub(crate) fn forget_forum_unreads(&mut self, threads: &[(ChannelId, MessageId)]) {
        let frames = &mut self.post_counts.unanswered;
        if let Some(at) = frames
            .iter()
            .rposition(|frame| frame.ask.threads == threads)
        {
            frames.remove(at);
        }
        self.forget_asked(threads);
    }

    // A resume replays the answers missed before RESUMED: the frames still unanswered went into
    // the dead connection.
    pub(super) fn forget_unanswered_forum_unreads(&mut self) {
        self.post_counts.resuming = false;
        for frame in std::mem::take(&mut self.post_counts.unanswered) {
            self.forget_asked(&frame.ask.threads);
        }
    }

    pub(crate) fn connection_lost(&mut self) {
        self.post_counts.resuming = true;
    }

    // Discord may never answer a frame: its posts are then asked again with the next ask.
    fn give_up_forum_unreads(&mut self) {
        let now = Instant::now();
        while let Some(frame) = self.post_counts.unanswered.front() {
            if now.duration_since(frame.sent) < FORUM_UNREADS_WAIT {
                break;
            }
            let Some(frame) = self.post_counts.unanswered.pop_front() else {
                break;
            };
            tracing::debug!(
                forum = frame.ask.forum.get(),
                posts = frame.ask.threads.len(),
                "op 28 unanswered after 30 s, given up"
            );
            self.forget_asked(&frame.ask.threads);
        }
    }

    fn forget_asked(&mut self, threads: &[(ChannelId, MessageId)]) {
        for (post, ack) in threads {
            if self.post_counts.asked.get(post) == Some(ack) {
                self.post_counts.asked.remove(post);
            }
        }
    }

    // The answer's frame: the oldest of the forum's that names the answer's posts. The forum's
    // frames before it were never answered.
    fn answered_frame(&mut self, unreads: &ForumUnreads) -> Option<ForumUnreadsAsk> {
        self.give_up_forum_unreads();
        let frames = &mut self.post_counts.unanswered;
        let of_forum = |frame: &Frame| frame.ask.forum == unreads.channel_id;
        let names = |frame: &Frame| {
            unreads.threads.iter().all(|thread| {
                frame
                    .ask
                    .threads
                    .iter()
                    .any(|(post, _)| *post == thread.thread_id)
            })
        };
        let at = frames
            .iter()
            .position(|frame| of_forum(frame) && names(frame))?;
        let frame = frames.remove(at)?;
        let mut before = 0;
        frames.retain(|frame| {
            before += 1;
            before > at || !of_forum(frame)
        });
        Some(frame.ask)
    }

    // Module 474399 keeps every count; here only the answer to the post's newest ask, and only
    // while the post still has the ack that ask named.
    pub(super) fn forum_unreads(&mut self, unreads: &ForumUnreads) {
        // Nothing ties an answer to its frame but the order, which a replay breaks: the frames
        // are asked again at RESUMED instead.
        if self.post_counts.resuming {
            return;
        }
        // A refusal answers its frame too.
        let Some(frame) = self.answered_frame(unreads) else {
            return;
        };
        if unreads.permission_denied {
            return;
        }
        for thread in &unreads.threads {
            let post = thread.thread_id;
            let named = frame
                .threads
                .iter()
                .find(|(asked, _)| *asked == post)
                .map(|(_, ack)| *ack);
            let (Some(count), Some(named)) = (thread.count, named) else {
                continue;
            };
            let current = self
                .entities
                .read_states
                .get(post)
                .and_then(|state| state.acked);
            if self.post_counts.asked.get(&post) != Some(&named) || current != Some(named) {
                continue;
            }
            self.post_counts.counts.insert(post, count);
            self.post_counts.changed.insert(unreads.channel_id);
        }
    }

    // Runs before the message raises the post's newest message, so a replayed one adds nothing.
    pub(super) fn count_post_message(&mut self, message: &model::Message) {
        let post = message.channel_id;
        let Some(forum) = self.forum_of(post) else {
            return;
        };
        let Some(stored) = self.entities.channels.get(&post) else {
            return;
        };
        let replayed = stored
            .last_message_id
            .is_some_and(|last| last.get() >= message.id.get());
        if replayed {
            return;
        }
        // A read post the user opened before shows "1+" from this message on.
        let qualifies_now = !self.entities.unread(stored)
            && self
                .entities
                .read_states
                .get(post)
                .is_some_and(|state| !state.estimated && state.acked.is_some());
        if let Some(count) = self.post_counts.counts.get_mut(&post) {
            *count = count.saturating_add(1);
            self.post_counts.changed.insert(forum);
        } else if qualifies_now && self.is_open_forum(forum) {
            self.post_counts.changed.insert(forum);
        }
    }

    // Every change of a channel's ack ends here. Discord's count only follows another session's
    // ack; Akari's starts over with any: at 0 once nothing is unread, else unknown until asked.
    pub(super) fn post_ack_moved(&mut self, post: ChannelId) {
        let Some(forum) = self.forum_of(post) else {
            return;
        };
        let counts = &self.post_counts;
        let tracked = counts.counts.contains_key(&post) || counts.asked.contains_key(&post);
        if !tracked && !self.is_open_forum(forum) {
            return;
        }
        let unread = self
            .entities
            .channels
            .get(&post)
            .is_some_and(|post| self.entities.unread(post));
        self.leave_asked_ack(post);
        if unread {
            self.post_counts.counts.remove(&post);
        } else {
            self.post_counts.counts.insert(post, 0);
        }
        self.post_counts.changed.insert(forum);
    }

    // The answers still on their way are for an ack the post no longer has.
    fn leave_asked_ack(&mut self, post: ChannelId) {
        self.post_counts.asked.remove(&post);
    }

    // A post that left the store.
    pub(super) fn forget_post_count(&mut self, post: ChannelId) {
        self.post_counts.counts.remove(&post);
        self.post_counts.asked.remove(&post);
    }

    // After channels left the store other than one by one.
    pub(super) fn prune_post_counts(&mut self) {
        let channels = &self.entities.channels;
        self.post_counts
            .counts
            .retain(|post, _| channels.contains_key(post));
        self.post_counts
            .asked
            .retain(|post, _| channels.contains_key(post));
    }

    pub(super) fn forget_post_counts(&mut self) {
        self.post_counts = PostCounts::default();
    }

    pub(super) fn flush_post_counts(&mut self, events: &mut Vec<StoreEvent>) {
        for forum_id in std::mem::take(&mut self.post_counts.changed) {
            events.push(StoreEvent::PostCountsChanged { forum_id });
        }
    }

    /// Of `posts`, the active ones that are unread, opened and acked, each with its count of
    /// new messages when one above 0 is known.
    pub(crate) fn post_new_counts(&self, posts: &[ChannelId]) -> Vec<PostNewCount> {
        posts
            .iter()
            .filter_map(|id| {
                let post = self.entities.channels.get(id)?;
                self.forum_of(*id)?;
                self.counted_from(post)?;
                // Without a message count there is nothing to cap with.
                let messages = post.thread.as_deref().and_then(|info| info.message_count);
                let count = self
                    .post_counts
                    .counts
                    .get(id)
                    .map(|count| messages.map_or(*count, |messages| (*count).min(messages)))
                    .filter(|count| *count > 0);
                Some(PostNewCount { post: *id, count })
            })
            .collect()
    }
}
