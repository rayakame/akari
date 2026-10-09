import AkariKit
import Foundation

/// Tokens in memory; records each call and whether it ran on the main thread.
final class MemoryTokenStore: TokenStore, @unchecked Sendable {
    enum Call: Equatable {
        case load(UserId)
        case save(UserId)
        case delete(UserId)
    }

    private let lock = NSLock()
    private var tokens: [UserId: String]
    private var recorded: [Call] = []
    private var onMainThread = 0

    init(tokens: [UserId: String] = [:]) {
        self.tokens = tokens
    }

    var calls: [Call] { lock.withLock { recorded } }
    var callsOnMainThread: Int { lock.withLock { onMainThread } }

    func token(for account: UserId) -> String? {
        lock.withLock { tokens[account] }
    }

    func load(account: UserId) throws -> String? {
        record(.load(account))
        return lock.withLock { tokens[account] }
    }

    func save(account: UserId, token: String) throws {
        record(.save(account))
        lock.withLock { tokens[account] = token }
    }

    func delete(account: UserId) throws {
        record(.delete(account))
        _ = lock.withLock { tokens.removeValue(forKey: account) }
    }

    private func record(_ call: Call) {
        let main = Thread.isMainThread
        lock.withLock {
            recorded.append(call)
            if main { onMainThread += 1 }
        }
    }
}
