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

    /// Trusts the URL's host, spelled as `host(_:)` gives it.
    public func trust(_ url: URL) {
        guard let host = Self.host(url), hosts.insert(host).inserted else {
            return
        }
        defaults.set(hosts.sorted(), forKey: key)
    }

    nonisolated static func key(_ account: UserId) -> String {
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
