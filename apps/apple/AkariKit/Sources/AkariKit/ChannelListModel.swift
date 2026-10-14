import Observation

/// A guild's channel list.
@MainActor @Observable
public final class ChannelListModel {
    public let guildId: GuildId
    /// Display order; categories are channels of kind `.guildCategory`.
    public private(set) var channels: [Channel] = []
    /// Channels and categories muted now by their own settings; a muted category doesn't
    /// mute its channels.
    public private(set) var muted: Set<ChannelId> = []
    /// The user hides muted channels in this guild.
    public private(set) var hidesMuted = false

    @ObservationIgnored private let store: Store

    init(guildId: GuildId, store: Store) {
        self.guildId = guildId
        self.store = store
        reload(rereading: nil)
        readSettings()
    }

    /// The list as shown: without muted channels when the user hides them, except `open`.
    /// Categories stay.
    public func shown(open: ChannelId?) -> [Channel] {
        guard hidesMuted else {
            return channels
        }
        return channels.filter { channel in
            channel.kind == .guildCategory || channel.id == open || !muted.contains(channel.id)
        }
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged {
            reload(rereading: nil)
            readSettings()
            return
        }
        let changed = batch.channelsChanged[guildId]
        // Member and role changes can show or hide channels.
        if changed != nil || batch.membersChanged.contains(guildId)
            || batch.guildsChanged.contains(guildId)
        {
            reload(rereading: changed ?? [])
        }
        if batch.guildSettingsChanged.contains(guildId) {
            readSettings()
        }
    }

    // `nil` re-reads every channel, e.g. after a new session.
    private func reload(rereading changed: Set<ChannelId>?) {
        let ids = store.channelList(guildId: guildId)
        var held = Dictionary(channels.map { ($0.id, $0) }) { first, _ in first }
        let unread = ids.filter { id in
            held[id] == nil || changed?.contains(id) != false
        }
        if !unread.isEmpty {
            for channel in store.channels(ids: unread) {
                held[channel.id] = channel
            }
        }
        let listed = channels.map(\.id)
        channels = ids.compactMap { held[$0] }
        if changed != nil, channels.map(\.id) != listed {
            muted = Set(store.mutedChannels(ids: channels.map(\.id)))
        }
    }

    private func readSettings() {
        muted = Set(store.mutedChannels(ids: channels.map(\.id)))
        hidesMuted = store.guildSettings(guildId: guildId).hideMutedChannels
    }
}
