import Foundation

/// Hosts whose masked links open without asking, per account (D65).
@MainActor
public final class TrustedLinks {
    private let defaults: UserDefaults
    private let key: String
    private var hosts: Set<String>

    public init(account: UserId, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        key = Self.key(account)
        hosts = Set(defaults.stringArray(forKey: key) ?? [])
    }

    public func isTrusted(_ url: URL) -> Bool {
        Self.host(url).map(hosts.contains) ?? false
    }

    /// Trusts the URL's host, lowercased.
    public func trust(_ url: URL) {
        guard let host = Self.host(url), hosts.insert(host).inserted else {
            return
        }
        defaults.set(hosts.sorted(), forKey: key)
    }

    nonisolated static func key(_ account: UserId) -> String {
        "trustedLinkHosts.\(account.rawValue)"
    }

    private static func host(_ url: URL) -> String? {
        url.host(percentEncoded: true)?.lowercased()
    }
}
