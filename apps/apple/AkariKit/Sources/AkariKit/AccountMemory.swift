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

    /// Forgets what's kept for the account: its spot, its expanded folders and the hosts its
    /// links are trusted for.
    public func forget(_ account: UserId) {
        remember(nil, of: account)
        remember(expandedFolders: [], of: account)
        TrustedLinks.forget(account, in: defaults)
    }

    private static func spotKey(_ account: UserId) -> String {
        "lastSpot.\(account.rawValue)"
    }

    private static func foldersKey(_ account: UserId) -> String {
        "expandedFolders.\(account.rawValue)"
    }
}
