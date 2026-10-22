import Foundation

/// The account to restore at launch, and where it was. Not a secret: IDs only.
public struct AccountMemory: Sendable {
    /// A place and the channel open there.
    public struct Spot: Equatable, Sendable {
        public let place: SessionModel.Place
        public let channel: ChannelId?

        public init(place: SessionModel.Place, channel: ChannelId?) {
            self.place = place
            self.channel = channel
        }
    }

    private static let key = "lastAccount"
    // Thread-safe per Apple's documentation, but not marked Sendable.
    nonisolated(unsafe) private let defaults: UserDefaults

    public init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    public var lastAccount: UserId? {
        get {
            defaults.string(forKey: Self.key).flatMap(UInt64.init).map(UserId.init(rawValue:))
        }
        nonmutating set {
            if let newValue {
                defaults.set(newValue.description, forKey: Self.key)
            } else {
                defaults.removeObject(forKey: Self.key)
            }
        }
    }

    /// Where the account was when it was last used.
    public func lastSpot(of account: UserId) -> Spot? {
        guard let stored = defaults.dictionary(forKey: Self.spotKey(account)) as? [String: String]
        else {
            return nil
        }
        let id = { (key: String) in stored[key].flatMap(UInt64.init) }
        let place: SessionModel.Place = id("guild").map { .guild(GuildId(rawValue: $0)) } ?? .home
        return Spot(place: place, channel: id("channel").map(ChannelId.init(rawValue:)))
    }

    /// `nil` forgets the account's spot.
    public func remember(_ spot: Spot?, of account: UserId) {
        guard let spot else {
            return defaults.removeObject(forKey: Self.spotKey(account))
        }
        var stored: [String: String] = [:]
        if case .guild(let guild) = spot.place {
            stored["guild"] = guild.description
        }
        stored["channel"] = spot.channel?.description
        defaults.set(stored, forKey: Self.spotKey(account))
    }

    @MainActor
    public func trustedLinks(of account: UserId) -> TrustedLinks {
        TrustedLinks(account: account, defaults: defaults)
    }

    @MainActor
    public func collapsedCategories() -> CollapsedCategories {
        CollapsedCategories(defaults: defaults)
    }

    /// The server folders the user expanded in Akari. Discord doesn't sync this.
    public func expandedFolders(of account: UserId) -> Set<Int64> {
        let stored = defaults.stringArray(forKey: Self.foldersKey(account)) ?? []
        return Set(stored.compactMap(Int64.init))
    }

    public func remember(expandedFolders folders: Set<Int64>, of account: UserId) {
        if folders.isEmpty {
            defaults.removeObject(forKey: Self.foldersKey(account))
        } else {
            defaults.set(folders.sorted().map(String.init), forKey: Self.foldersKey(account))
        }
    }

    /// A channel's thread in the split view, and the channel's server.
    public struct Split: Hashable, Sendable {
        public let thread: ChannelId
        public let guild: GuildId

        public init(thread: ChannelId, guild: GuildId) {
            self.thread = thread
            self.guild = guild
        }
    }

    /// The thread open beside each channel in the split view, by channel.
    public func splits(of account: UserId) -> [ChannelId: Split] {
        let stored = defaults.dictionary(forKey: Self.splitsKey(account)) as? [String: [String]]
        var splits: [ChannelId: Split] = [:]
        for (channel, ids) in stored ?? [:] {
            guard let channel = UInt64(channel), ids.count == 2, let thread = UInt64(ids[0]),
                let guild = UInt64(ids[1])
            else {
                continue
            }
            splits[ChannelId(rawValue: channel)] = Split(
                thread: ChannelId(rawValue: thread), guild: GuildId(rawValue: guild))
        }
        return splits
    }

    public func remember(splits: [ChannelId: Split], of account: UserId) {
        if splits.isEmpty {
            defaults.removeObject(forKey: Self.splitsKey(account))
        } else {
            let stored = Dictionary(
                uniqueKeysWithValues: splits.map { channel, split in
                    (channel.description, [split.thread.description, split.guild.description])
                })
            defaults.set(stored, forKey: Self.splitsKey(account))
        }
    }

    /// Forgets what's kept for the account: its spot, its split views, its expanded folders and
    /// the hosts its links are trusted for.
    public func forget(_ account: UserId) {
        remember(nil, of: account)
        remember(splits: [:], of: account)
        remember(expandedFolders: [], of: account)
        TrustedLinks.forget(account, in: defaults)
    }

    private static func spotKey(_ account: UserId) -> String {
        "lastSpot.\(account.rawValue)"
    }

    private static func splitsKey(_ account: UserId) -> String {
        "splits.\(account.rawValue)"
    }

    private static func foldersKey(_ account: UserId) -> String {
        "expandedFolders.\(account.rawValue)"
    }
}
