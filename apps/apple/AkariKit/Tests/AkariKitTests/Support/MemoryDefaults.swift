import Foundation

// Defaults for tests that never reach cfprefsd: no domain, no plist, nothing left behind.
final class MemoryDefaults: UserDefaults, @unchecked Sendable {
    private let values = Locked<[String: Any]>([:])

    init() {
        super.init(suiteName: nil)!
    }

    override func object(forKey defaultName: String) -> Any? {
        values.withLock { $0[defaultName] }
    }

    override func set(_ value: Any?, forKey defaultName: String) {
        values.withLock { $0[defaultName] = value }
    }

    override func removeObject(forKey defaultName: String) {
        values.withLock { $0[defaultName] = nil }
    }

    var keys: [String] {
        values.withLock { Array($0.keys) }
    }
}
