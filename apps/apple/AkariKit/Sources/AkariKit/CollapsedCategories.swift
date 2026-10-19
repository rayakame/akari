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
        store(categories, in: guild)
    }

    private func store(_ categories: Set<ChannelId>, in guild: GuildId) {
        collapsed[guild] = categories
        defaults.set(categories.map(\.description), forKey: Self.key(guild))
    }

    /// The list as shown: a collapsed category keeps only the channels and threads `keeps`
    /// accepts, and the channels of the threads it keeps.
    public func visible(
        _ channels: [Channel], in guild: GuildId, keeps: (Channel) -> Bool
    ) -> [Channel] {
        let categories = categories(in: guild)
        guard !categories.isEmpty else {
            return channels
        }
        let parents = Dictionary(channels.map { ($0.id, $0.parentId) }) { first, _ in first }
        let collapsed = { (channel: Channel) -> Bool in
            let category =
                channel.isThread
                ? channel.parentId.flatMap { parents[$0] ?? nil } : channel.parentId
            return category.map(categories.contains) ?? false
        }
        let threads = channels.filter { $0.isThread && collapsed($0) && keeps($0) }
        let kept = Set(threads.map(\.id)).union(threads.compactMap(\.parentId))
        return channels.filter { channel in
            guard collapsed(channel) else {
                return true
            }
            return kept.contains(channel.id) || (!channel.isThread && keeps(channel))
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
