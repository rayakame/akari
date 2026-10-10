import Observation

/// The server list.
@MainActor @Observable
public final class GuildListModel {
    /// Server list order.
    public private(set) var guilds: [Guild] = []
    public private(set) var unavailableIds: [GuildId] = []

    @ObservationIgnored private let store: Store

    init(store: Store) {
        self.store = store
    }

    func reload() {
        rebuild(rereading: nil)
    }

    func apply(_ batch: EventBatch) {
        if batch.ready {
            reload()
        } else if batch.guildListChanged {
            rebuild(rereading: batch.guildsChanged)
        } else if !batch.guildsChanged.isEmpty {
            guilds = guilds.map { guild in
                guard batch.guildsChanged.contains(guild.id) else {
                    return guild
                }
                return store.guild(id: guild.id) ?? guild
            }
        }
    }

    // `nil` re-reads every guild, e.g. after a new session.
    private func rebuild(rereading changed: Set<GuildId>?) {
        let ids = store.guildIds()
        unavailableIds = store.unavailableGuildIds()
        let held = Dictionary(guilds.map { ($0.id, $0) }) { first, _ in first }
        guilds = ids.compactMap { id in
            if let guild = held[id], changed?.contains(id) == false {
                return guild
            }
            return store.guild(id: id)
        }
    }
}
