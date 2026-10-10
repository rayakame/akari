import Foundation

/// The account to restore at launch. Not a secret: only the user ID.
public struct AccountMemory: Sendable {
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
}
