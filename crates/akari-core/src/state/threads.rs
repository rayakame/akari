use std::collections::{BTreeSet, HashMap};

use super::apply::Entities;
use super::types::Channel;
use crate::model::ChannelId;

/// Each channel's threads, so a change to a channel reaches them without a scan.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ThreadIndex(HashMap<ChannelId, BTreeSet<ChannelId>>);

impl ThreadIndex {
    pub(crate) fn build(entities: &Entities) -> Self {
        #[cfg(test)]
        super::apply::tests::scanned_for_threads();
        let mut index = Self::default();
        for channel in entities.channels.values() {
            index.add(channel);
        }
        index
    }

    pub(crate) fn add(&mut self, channel: &Channel) {
        if let Some(parent) = channel.parent_id.filter(|_| channel.is_thread()) {
            self.0.entry(parent).or_default().insert(channel.id);
        }
    }

    pub(crate) fn remove(&mut self, channel: &Channel) {
        self.0.remove(&channel.id);
        if let Some(parent) = channel.parent_id
            && let Some(threads) = self.0.get_mut(&parent)
        {
            threads.remove(&channel.id);
            if threads.is_empty() {
                self.0.remove(&parent);
            }
        }
    }

    pub(crate) fn of(&self, channel: ChannelId) -> impl Iterator<Item = ChannelId> + '_ {
        self.0.get(&channel).into_iter().flatten().copied()
    }
}
