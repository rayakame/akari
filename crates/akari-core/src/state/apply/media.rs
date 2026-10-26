use std::sync::Arc;

use super::State;
use crate::model::{ChannelId, ChannelType, GuildId, MessageId};
use crate::state::events::StoreEvent;
use crate::state::media::{self, AgeGate, Covered, MediaCover, Scan, ShownAttachment};
use crate::state::types::Message;

impl State {
    pub(crate) fn shown_attachments(
        &self,
        message: &Message,
        now_millis: i64,
    ) -> Vec<ShownAttachment> {
        let display = &self.entities.settings.display;
        let covered = self.covered(message);
        let scan = self.scan(message.id);
        let spoilers = self.spoilers_shown(message.channel_id, now_millis);
        media::shown_attachments(message, display, |attachment| {
            media::media_cover(attachment, covered, scan, spoilers)
        })
    }

    pub(crate) fn spoilers_shown(&self, channel: ChannelId, now_millis: i64) -> bool {
        media::spoilers_shown(
            &self.entities.settings.display,
            self.permissions(channel, now_millis),
        )
    }

    pub(crate) fn age_gate(&self, channel: ChannelId) -> AgeGate {
        let Some(channel) = self.entities.channels.get(&channel) else {
            return AgeGate::Open;
        };
        let Some(guild) = channel.guild_id else {
            return AgeGate::Open;
        };
        // Threads and forum posts are as their parent (the client's ChannelStore, 734057).
        let restricted = channel.nsfw
            || channel
                .parent_id
                .filter(|_| channel.thread.is_some())
                .and_then(|parent| self.entities.channels.get(&parent))
                .is_some_and(|parent| parent.nsfw);
        let server = self
            .entities
            .guilds
            .get(&guild)
            .is_some_and(|guild| guild.age_restricted);
        media::age_gate(
            restricted,
            server,
            self.nsfw_allowed(),
            self.entities.settings.age_agreements.contains(&guild),
        )
    }

    pub(crate) fn agree_to_age_restricted(&mut self, guild: GuildId, events: &mut Vec<StoreEvent>) {
        let agreements = &mut self.entities.settings.age_agreements;
        if agreements.contains(&guild) {
            return;
        }
        Arc::make_mut(agreements).insert(guild);
        events.push(StoreEvent::AgeGateChanged { guild_id: guild });
    }

    /// Per viewed channel, the messages whose media waits for a scan nobody asked for yet.
    pub(crate) fn scans_due(&self) -> Vec<(ChannelId, Vec<MessageId>)> {
        let mut due = Vec::new();
        for channel in self.windows.channels() {
            let Some(window) = self.windows.snapshot(channel) else {
                continue;
            };
            let ids: Vec<MessageId> = window
                .messages
                .iter()
                .filter(|message| {
                    !self.scans.asked.contains(&message.id) && self.waits_for_scan(message)
                })
                .map(|message| message.id)
                .collect();
            if !ids.is_empty() {
                due.push((channel, ids));
            }
        }
        due.sort_unstable_by_key(|(channel, _)| *channel);
        due
    }

    pub(crate) fn ask_scans(&mut self, ids: &[MessageId]) {
        self.scans.asked.extend(ids.iter().copied());
    }

    /// The scans' wait is over (the client's 3 s): their media shows unless Discord flagged it
    /// meanwhile.
    pub(crate) fn settle_scans(
        &mut self,
        channel: ChannelId,
        ids: &[MessageId],
        events: &mut Vec<StoreEvent>,
    ) {
        for id in ids {
            if !self.scans.settled.insert(*id) {
                continue;
            }
            let Some(message) = self.windows.message(channel, *id) else {
                continue;
            };
            let unsettled = Scan {
                settled: false,
                ..self.scan(*id)
            };
            let covered = self.covered(&message);
            let waited = message.attachments.iter().any(|attachment| {
                media::media_cover(attachment, covered, unsettled, true)
                    == Some(MediaCover::Pending)
            });
            if waited {
                events.push(StoreEvent::MessageUpdated(message));
            }
        }
    }

    // The user's own messages, pending and failed ones among them, never wait.
    fn waits_for_scan(&self, message: &Message) -> bool {
        let covered = self.covered(message);
        let scan = self.scan(message.id);
        message.attachments.iter().any(|attachment| {
            attachment.is_visual()
                && media::media_cover(attachment, covered, scan, true) == Some(MediaCover::Pending)
        })
    }

    // What the message's place covers; nothing of the user's own.
    fn covered(&self, message: &Message) -> Covered {
        let me = self
            .entities
            .current_user
            .as_ref()
            .map(|current| current.user.id);
        if me == Some(message.author.id) {
            return Covered::default();
        }
        let kind = self
            .entities
            .channels
            .get(&message.channel_id)
            .map_or(ChannelType::GuildText, |channel| channel.kind);
        let place = media::place(kind, self.entities.read_states.is_friend(message.author.id));
        media::covered(&self.entities.settings.display, place, self.nsfw_allowed())
    }

    fn scan(&self, message: MessageId) -> Scan {
        Scan {
            settled: self.scans.settled.contains(&message),
            newest: self.entities.scan_version,
        }
    }

    fn nsfw_allowed(&self) -> Option<bool> {
        self.entities
            .current_user
            .as_ref()
            .and_then(|current| current.nsfw_allowed)
    }
}
