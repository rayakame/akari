use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Arc;

use super::{Entities, State};
use crate::model::{ChannelId, ChannelType, GuildId, MessageId, UserId};
use crate::state::embeds::{self, ShownEmbed};
use crate::state::events::StoreEvent;
use crate::state::media::{self, AgeGate, Covered, MediaCover, Scan, ShownAttachment};
use crate::state::types::{Message, StarterMessage};

const HIDE_MEDIA_DOWNLOAD_OPTIONS: u64 = 1 << 15;

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

    /// `spoilered_link`: the content has a link inside a spoiler, which covers every embed.
    pub(crate) fn shown_embeds(
        &self,
        message: &Message,
        spoilered_link: bool,
        now_millis: i64,
    ) -> Vec<ShownEmbed> {
        let covered = self.covered(message);
        let scan = self.scan(message.id);
        let bot = message.author.bot;
        let spoilered = spoilered_link && !self.spoilers_shown(message.channel_id, now_millis);
        embeds::shown_embeds(message, &self.entities.settings.display, |embed| {
            embeds::embed_cover(embed, covered, scan, bot, spoilered)
        })
    }

    pub(crate) fn embeds_hide_link(&self, shown: &[ShownEmbed], single_link: bool) -> bool {
        embeds::hides_link(shown, &self.entities.settings.display, single_link)
    }

    pub(crate) fn spoilers_shown(&self, channel: ChannelId, now_millis: i64) -> bool {
        media::spoilers_shown(&self.entities.settings.display, || {
            self.permissions(channel, now_millis)
        })
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

    /// HIDE_MEDIA_DOWNLOAD_OPTIONS (1 << 15) as the client reads it (module 207133): a forum or
    /// media channel's post takes it from its parent; nothing else has it.
    pub(crate) fn hides_media_downloads(&self, channel: ChannelId) -> bool {
        let Some(parent) = self
            .entities
            .channels
            .get(&channel)
            .filter(|channel| channel.thread.is_some())
            .and_then(|post| post.parent_id)
            .and_then(|parent| self.entities.channels.get(&parent))
        else {
            return false;
        };
        matches!(
            parent.kind,
            ChannelType::GuildForum | ChannelType::GuildMedia
        ) && parent.flags & HIDE_MEDIA_DOWNLOAD_OPTIONS != 0
    }

    pub(crate) fn agree_to_age_restricted(&mut self, guild: GuildId, events: &mut Vec<StoreEvent>) {
        let agreements = &mut self.entities.settings.age_agreements;
        if agreements.contains(&guild) {
            return;
        }
        Arc::make_mut(agreements).insert(guild);
        events.push(StoreEvent::AgeGateChanged { guild_id: guild });
    }

    // A READY that beat the restore knows nothing of them; the flusher would erase them.
    pub(crate) fn keep_cached_agreements(
        &mut self,
        cached: &Entities,
        events: &mut Vec<StoreEvent>,
    ) {
        for guild in cached.settings.age_agreements.iter() {
            self.agree_to_age_restricted(*guild, events);
        }
    }

    /// Per channel, the messages of viewed windows and threads' starters whose media waits for a
    /// scan nobody asked for yet; with `retry`, also those whose ask failed and that Discord
    /// still hasn't scanned.
    pub(crate) fn scans_due(&self, retry: bool) -> Vec<(ChannelId, Vec<MessageId>)> {
        let mut due: BTreeMap<ChannelId, BTreeSet<MessageId>> = BTreeMap::new();
        let mut consider = |message: &Message| {
            let wanted = if self.scans.asked.contains(&message.id) {
                retry
                    && self.scans.failed.contains(&message.id)
                    && self.waits(
                        message,
                        Scan {
                            settled: false,
                            ..self.scan(message.id)
                        },
                    )
            } else {
                self.waits(message, self.scan(message.id))
            };
            if wanted {
                due.entry(message.channel_id)
                    .or_default()
                    .insert(message.id);
            }
        };
        for channel in self.windows.channels() {
            for message in self.windows.messages(channel) {
                consider(message);
            }
            if let Some(StarterMessage::Loaded(starter)) = self.windows.starter_message(channel) {
                consider(&starter);
            }
        }
        due.into_iter()
            .map(|(channel, ids)| (channel, ids.into_iter().collect()))
            .collect()
    }

    pub(crate) fn ask_scans(&mut self, ids: &[MessageId]) {
        for id in ids {
            self.scans.asked.insert(*id);
            self.scans.failed.remove(id);
        }
    }

    pub(crate) fn fail_scans(&mut self, ids: &[MessageId]) {
        self.scans.failed.extend(ids.iter().copied());
    }

    /// The scans' wait is over (the client's 3 s): their media shows unless Discord flagged it
    /// meanwhile, in the channel's window and in the starters of threads that show it.
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
            let Some(message) = self.windows.shown(channel, *id) else {
                continue;
            };
            let unsettled = Scan {
                settled: false,
                ..self.scan(*id)
            };
            if self.waits(&message, unsettled) {
                let rows = self.windows.message(channel, *id).into_iter();
                let starters = self.windows.starter_rows_showing(channel, *id);
                events.extend(rows.chain(starters).map(StoreEvent::MessageUpdated));
            }
        }
    }

    // A READY: the scans of messages no window or starter shows any more go.
    pub(crate) fn forget_gone_scans(&mut self) {
        let mut shown = HashSet::new();
        for channel in self.windows.channels() {
            shown.extend(
                self.windows
                    .messages(channel)
                    .iter()
                    .map(|message| message.id),
            );
            if let Some(StarterMessage::Loaded(starter)) = self.windows.starter_message(channel) {
                shown.insert(starter.id);
            }
        }
        let scans = &mut self.scans;
        for ids in [&mut scans.asked, &mut scans.settled, &mut scans.failed] {
            ids.retain(|id| shown.contains(id));
        }
    }

    // Whether the author is a friend decides a DM's covers: their messages with media change.
    pub(crate) fn friendship_changed(&self, user: UserId, was: bool, events: &mut Vec<StoreEvent>) {
        if self.entities.read_states.is_friend(user) == was {
            return;
        }
        for channel in self.windows.channels() {
            let private = self.entities.channels.get(&channel).is_some_and(|channel| {
                matches!(channel.kind, ChannelType::Dm | ChannelType::GroupDm)
            });
            if !private {
                continue;
            }
            events.extend(
                self.windows
                    .messages(channel)
                    .iter()
                    .filter(|message| {
                        message.author.id == user
                            && (message
                                .attachments
                                .iter()
                                .any(|attachment| attachment.is_visual())
                                || !message.embeds.is_empty())
                    })
                    .cloned()
                    .map(StoreEvent::MessageUpdated),
            );
        }
    }

    // The user's own messages, pending and failed ones among them, never wait.
    fn waits(&self, message: &Message, scan: Scan) -> bool {
        let covered = self.covered(message);
        let attachments = message.attachments.iter().any(|attachment| {
            attachment.is_visual()
                && media::media_cover(attachment, covered, scan, true) == Some(MediaCover::Pending)
        });
        attachments
            || (!message.author.bot
                && embeds::shows_embeds(message, &self.entities.settings.display)
                && message
                    .embeds
                    .iter()
                    .any(|embed| embeds::waits_for_scan(embed, covered, scan)))
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
