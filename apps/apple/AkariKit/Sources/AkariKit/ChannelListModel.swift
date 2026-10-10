import Observation

/// A guild's channel list.
@MainActor @Observable
public final class ChannelListModel {
    public let guildId: GuildId
    /// Display order; categories are channels of kind `.guildCategory`.
    public private(set) var channels: [Channel] = []

    @ObservationIgnored private let store: Store

    init(guildId: GuildId, store: Store) {
        self.guildId = guildId
        self.store = store
        reload(rereading: nil)
    }

    func apply(_ batch: EventBatch) {
        if batch.ready {
            return reload(rereading: nil)
        }
        let changed = batch.channelsChanged[guildId]
        // Member and role changes can show or hide channels.
        if changed != nil || batch.membersChanged.contains(guildId)
            || batch.guildsChanged.contains(guildId)
        {
            reload(rereading: changed ?? [])
        }
    }

    /// Re-reads the list, then the channels it doesn't hold and `rereading` (all when `nil`).
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
        channels = ids.compactMap { held[$0] }
    }
}
