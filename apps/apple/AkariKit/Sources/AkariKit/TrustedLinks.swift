import Foundation

/// Hosts whose masked links open without asking, per account (D65).
@MainActor
public final class TrustedLinks {
    private let defaults: UserDefaults
    private let key: String
    private let tokenKey: String
    private let token: String

    public init(account: UserId, defaults: UserDefaults = .standard) {
        self.defaults = defaults
        key = Self.key(account)
        tokenKey = Self.tokenKey(account)
        if let stored = defaults.string(forKey: tokenKey) {
            token = stored
        } else {
            token = UUID().uuidString
            defaults.set(token, forKey: tokenKey)
        }
    }

    public func isTrusted(_ url: URL) -> Bool {
        guard isCurrent, let host = Self.host(url) else {
            return false
        }
        return stored.contains(host)
    }

    /// Trusts the URL's host, spelled as `host(_:)` gives it; nothing once the account was
    /// forgotten after this list was made.
    public func trust(_ url: URL) {
        guard isCurrent, let host = Self.host(url) else {
            return
        }
        var hosts = stored
        if hosts.insert(host).inserted {
            defaults.set(hosts.sorted(), forKey: key)
        }
    }

    // Read each time, so lists of one account keep each other's changes.
    private var stored: Set<String> {
        Set(defaults.stringArray(forKey: key) ?? [])
    }

    // A confirmation still open when the session ended answers through a list made before the
    // forget, which took the account's token with its hosts.
    private var isCurrent: Bool {
        defaults.string(forKey: tokenKey) == token
    }

    nonisolated static func forget(_ account: UserId, in defaults: UserDefaults) {
        defaults.removeObject(forKey: key(account))
        defaults.removeObject(forKey: tokenKey(account))
    }

    private nonisolated static func key(_ account: UserId) -> String {
        "trustedLinkHosts.\(account.rawValue)"
    }

    private nonisolated static func tokenKey(_ account: UserId) -> String {
        "trustedLinkHosts.\(account.rawValue).token"
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
