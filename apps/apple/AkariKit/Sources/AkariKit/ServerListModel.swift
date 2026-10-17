import Observation

/// The server list: servers and folders in the user's order.
@MainActor @Observable
public final class ServerListModel {
    public enum Item: Equatable, Identifiable {
        case guild(Guild)
        case folder(FolderItem)

        public var id: String {
            switch self {
            case .guild(let guild): "guild \(guild.id)"
            case .folder(let item): "folder \(item.folder.id)"
            }
        }
    }

    /// A folder with its servers in the folder's order.
    public struct FolderItem: Equatable, Identifiable {
        public let folder: Folder
        public let guilds: [Guild]

        public var id: Int64 { folder.id }

        public init(folder: Folder, guilds: [Guild]) {
            self.folder = folder
            self.guilds = guilds
        }
    }

    public private(set) var items: [Item] = []
    /// Every listed server in order, folders opened.
    public private(set) var guilds: [Guild] = []
    public private(set) var unavailableIds: [GuildId] = []
    /// What each server's icon shows.
    public private(set) var badges: [GuildId: Badge] = [:]
    private var expanded: Set<Int64>

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: UserId
    @ObservationIgnored private let memory: AccountMemory
    @ObservationIgnored private var list: [ServerListItem] = []

    init(store: Store, account: UserId, memory: AccountMemory) {
        self.store = store
        self.account = account
        self.memory = memory
        expanded = memory.expandedFolders(of: account)
    }

    /// Folders start collapsed; Akari keeps each one's state per account.
    public func isExpanded(folder: Int64) -> Bool {
        expanded.contains(folder)
    }

    public func toggle(folder: Int64) {
        if expanded.remove(folder) == nil {
            expanded.insert(folder)
        }
        memory.remember(expandedFolders: expanded, of: account)
    }

    public func badge(of guild: GuildId) -> Badge {
        badges[guild] ?? Badge(unread: false, mentions: 0)
    }

    /// The folder's servers' badges added up, collapsed or not.
    public func badge(of folder: FolderItem) -> Badge {
        let badges = folder.guilds.map { badge(of: $0.id) }
        return Badge(
            unread: badges.contains(where: \.unread),
            mentions: badges.reduce(0) { $0 &+ $1.mentions })
    }

    /// A collapsed folder's tile shows its servers' badges added up; an expanded one's shows
    /// nothing, since its servers show their own.
    public func tileBadge(of folder: FolderItem) -> Badge {
        isExpanded(folder: folder.id) ? Badge(unread: false, mentions: 0) : badge(of: folder)
    }

    func reload() {
        rebuild(rereading: nil)
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged {
            reload()
        } else if batch.guildListChanged {
            rebuild(rereading: batch.guildsChanged)
        } else if !batch.guildsChanged.isEmpty {
            let held = Dictionary(guilds.map { ($0.id, $0) }) { first, _ in first }
            layOut(guild: { id in
                guard batch.guildsChanged.contains(id) else {
                    return held[id]
                }
                return store.guild(id: id) ?? held[id]
            })
        }
        if !batch.everythingChanged, !batch.guildBadgesChanged.isEmpty {
            readBadges(guilds.map(\.id).filter(batch.guildBadgesChanged.contains))
        }
    }

    // `nil` re-reads every guild, e.g. after a new session.
    private func rebuild(rereading changed: Set<GuildId>?) {
        list = store.serverList()
        unavailableIds = store.unavailableGuildIds()
        let held = Dictionary(guilds.map { ($0.id, $0) }) { first, _ in first }
        layOut(guild: { id in
            if let guild = held[id], changed?.contains(id) == false {
                return guild
            }
            return store.guild(id: id)
        })
        let joined = guilds.map(\.id).filter { changed == nil || held[$0] == nil }
        if changed == nil {
            badges = [:]
        }
        readBadges(joined)
    }

    private func readBadges(_ ids: [GuildId]) {
        guard !ids.isEmpty else {
            return
        }
        for read in store.guildBadges(ids: ids) {
            badges[read.guildId] = read.badge
        }
    }

    private func layOut(guild: (GuildId) -> Guild?) {
        items = list.compactMap { entry in
            switch entry {
            case .guild(let id):
                return guild(id).map(Item.guild)
            case .folder(let folder):
                let guilds = folder.guildIds.compactMap(guild)
                return guilds.isEmpty ? nil : .folder(FolderItem(folder: folder, guilds: guilds))
            }
        }
        guilds = items.flatMap { item in
            switch item {
            case .guild(let guild): [guild]
            case .folder(let folder): folder.guilds
            }
        }
    }
}
