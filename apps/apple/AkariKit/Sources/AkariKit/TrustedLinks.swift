import Foundation

/// Hosts whose masked links open without asking, per account (D65).
@MainActor
public final class TrustedLinks {
    private let defaults: UserDefaults
    private let key: String
    private var hosts: Set<String>
    private let generation: Int

    public init(account: UserId, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        key = Self.key(account)
        hosts = Set(defaults.stringArray(forKey: key) ?? [])
        generation = defaults.integer(forKey: Self.generationKey)
    }

    public func isTrusted(_ url: URL) -> Bool {
        isCurrent && (Self.host(url).map(hosts.contains) ?? false)
    }

    /// Trusts the URL's host, spelled as `host(_:)` gives it; nothing once the account was
    /// forgotten after this list was made.
    public func trust(_ url: URL) {
        guard isCurrent, let host = Self.host(url), hosts.insert(host).inserted else {
            return
        }
        defaults.set(hosts.sorted(), forKey: key)
    }

    // A confirmation still open when the session ended answers through a list made before.
    private var isCurrent: Bool {
        defaults.integer(forKey: Self.generationKey) == generation
    }

    nonisolated static func forget(_ account: UserId, in defaults: UserDefaults) {
        defaults.removeObject(forKey: key(account))
        defaults.set(defaults.integer(forKey: generationKey) + 1, forKey: generationKey)
    }

    // A count of forgets, so it names no account.
    private nonisolated static let generationKey = "trustedLinkHosts.generation"

    private nonisolated static func key(_ account: UserId) -> String {
        "trustedLinkHosts.\(account.rawValue)"
    }

    /// The host a trust covers: lowercased, in its ASCII spelling (punycode), without a
    /// trailing dot.
    public nonisolated static func host(_ url: URL) -> String? {
        // Unencoded, Foundation gives an international host in punycode.
        var host = url.host(percentEncoded: false)?.lowercased() ?? ""
        while host.hasSuffix(".") {
            host.removeLast()
        }
        return host.isEmpty ? nil : host
    }
}
