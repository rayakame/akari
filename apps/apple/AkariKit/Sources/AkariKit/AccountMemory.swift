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

    private static func spotKey(_ account: UserId) -> String {
        "lastSpot.\(account.rawValue)"
    }
}
