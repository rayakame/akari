import Foundation
import Observation

/// Collapsed categories per guild, kept across launches.
@MainActor @Observable
public final class CollapsedCategories {
    private var collapsed: [GuildId: Set<ChannelId>] = [:]
    @ObservationIgnored private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public func isCollapsed(_ category: ChannelId, in guild: GuildId) -> Bool {
        categories(in: guild).contains(category)
    }

    public func toggle(_ category: ChannelId, in guild: GuildId) {
        var categories = categories(in: guild)
        if categories.remove(category) == nil {
            categories.insert(category)
        }
        collapsed[guild] = categories
        defaults.set(categories.map(\.description), forKey: Self.key(guild))
    }

    /// The list as shown: a collapsed category keeps only the open channel among its own.
    public func visible(_ channels: [Channel], in guild: GuildId, open: ChannelId?) -> [Channel] {
        let categories = categories(in: guild)
        guard !categories.isEmpty else {
            return channels
        }
        return channels.filter { channel in
            guard let parent = channel.parentId, categories.contains(parent) else {
                return true
            }
            return channel.id == open
        }
    }

    // Reads the stored set without caching it, since views call this while rendering.
    private func categories(in guild: GuildId) -> Set<ChannelId> {
        if let categories = collapsed[guild] {
            return categories
        }
        let stored = defaults.stringArray(forKey: Self.key(guild)) ?? []
        return Set(stored.compactMap(UInt64.init).map(ChannelId.init(rawValue:)))
    }

    private static func key(_ guild: GuildId) -> String {
        "collapsedCategories.\(guild.rawValue)"
    }
}
