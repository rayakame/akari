use std::collections::HashMap;
use std::hash::Hash;
use std::sync::Arc;

use super::{Cache, CacheError, Writer};
use crate::model::ChannelId;
use crate::state::apply::Entities;
use crate::state::{CachedWindow, Message, Store};

pub(crate) const MESSAGES_PER_CHANNEL: usize = 50;
pub(crate) const CHANNELS_WITH_MESSAGES: usize = 50;
// Free pages past this are given back to the file system after a flush.
const FREE_PAGES: i64 = 256;

pub(crate) struct Snapshot {
    pub(crate) entities: Entities,
    pub(crate) windows: HashMap<ChannelId, CachedWindow>,
}

impl Snapshot {
    // None before a session or a restore: writing that would delete the file's state.
    pub(crate) fn take(store: &Store) -> Option<Self> {
        let (entities, windows) = store.cache_parts(MESSAGES_PER_CHANNEL);
        entities.current_user.as_ref()?;
        Some(Self {
            entities,
            windows: windows.into_iter().collect(),
        })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Flushed {
    pub(crate) guilds: usize,
    pub(crate) channels: usize,
    pub(crate) windows: usize,
    pub(crate) messages: usize,
    // The current user, DM recipients, memberships and unavailable guilds.
    pub(crate) other: usize,
    pub(crate) deleted: usize,
}

impl Flushed {
    pub(crate) fn rows(&self) -> usize {
        self.guilds + self.channels + self.windows + self.messages + self.other
    }
}

// What the file holds as far as this session knows; a flush writes only the difference.
#[derive(Default)]
pub(crate) struct Written {
    pub(crate) entities: Entities,
    windows: HashMap<ChannelId, CachedWindow>,
}

impl Written {
    pub(crate) fn read(entities: Entities) -> Self {
        Self {
            entities,
            windows: HashMap::new(),
        }
    }
}

fn same<T: PartialEq>(a: &Arc<T>, b: &Arc<T>) -> bool {
    Arc::ptr_eq(a, b) || a == b
}

fn same_window(a: &CachedWindow, b: &CachedWindow) -> bool {
    a.viewed_at == b.viewed_at
        && a.messages.len() == b.messages.len()
        && a.messages.iter().zip(&b.messages).all(|(a, b)| same(a, b))
}

fn diff<'w, K: Copy + Eq + Hash, T: PartialEq>(
    old: &HashMap<K, Arc<T>>,
    new: &HashMap<K, Arc<T>>,
    writer: &mut Writer<'w>,
    put: fn(&mut Writer<'w>, &T) -> rusqlite::Result<()>,
    delete: fn(&mut Writer<'w>, K) -> rusqlite::Result<()>,
) -> rusqlite::Result<(usize, usize)> {
    let mut counts = (0, 0);
    for (key, value) in new {
        if !old.get(key).is_some_and(|known| same(known, value)) {
            put(writer, value)?;
            counts.0 += 1;
        }
    }
    for key in old.keys().filter(|key| !new.contains_key(key)) {
        delete(writer, *key)?;
        counts.1 += 1;
    }
    Ok(counts)
}

// `written` follows only once the transaction committed, so a failed write is retried in full.
pub(crate) fn flush(
    cache: &mut Cache,
    written: &mut Written,
    next: Snapshot,
) -> Result<Flushed, CacheError> {
    let old = &written.entities;
    let new = &next.entities;
    let (flushed, evicted) = cache.write(|writer| {
        let mut flushed = Flushed::default();
        if let Some(current) = &new.current_user
            && !old
                .current_user
                .as_ref()
                .is_some_and(|known| same(known, current))
        {
            writer.put_current_user(current)?;
            flushed.other += 1;
        }
        let (put, deleted) = diff(
            &old.users,
            &new.users,
            writer,
            Writer::put_user,
            Writer::delete_user,
        )?;
        flushed.other += put;
        flushed.deleted += deleted;
        let (put, deleted) = diff(
            &old.guilds,
            &new.guilds,
            writer,
            Writer::put_guild,
            Writer::delete_guild,
        )?;
        flushed.guilds += put;
        flushed.deleted += deleted;
        let (put, deleted) = diff(
            &old.members,
            &new.members,
            writer,
            Writer::put_member,
            Writer::delete_member,
        )?;
        flushed.other += put;
        flushed.deleted += deleted;
        let (put, deleted) = diff(
            &old.channels,
            &new.channels,
            writer,
            Writer::put_channel,
            Writer::delete_channel,
        )?;
        flushed.channels += put;
        flushed.deleted += deleted;
        for id in new.unavailable.difference(&old.unavailable) {
            writer.put_unavailable(*id)?;
            flushed.other += 1;
        }
        for id in old.unavailable.difference(&new.unavailable) {
            writer.delete_unavailable(*id)?;
            flushed.deleted += 1;
        }
        for (channel, window) in &next.windows {
            if written
                .windows
                .get(channel)
                .is_some_and(|known| same_window(known, window))
            {
                continue;
            }
            let skip = window.messages.len().saturating_sub(MESSAGES_PER_CHANNEL);
            let messages: &[Arc<Message>] = &window.messages[skip..];
            writer.put_window(*channel, window.viewed_at, messages)?;
            flushed.windows += 1;
            flushed.messages += messages.len();
        }
        let (evicted, deleted) = writer.evict(CHANNELS_WITH_MESSAGES)?;
        flushed.deleted += deleted;
        Ok((flushed, evicted))
    })?;
    let Snapshot { entities, windows } = next;
    written.entities = entities;
    written.windows.extend(windows);
    written
        .windows
        .retain(|channel, _| !evicted.contains(channel));
    cache.release_free_pages(FREE_PAGES)?;
    Ok(flushed)
}
