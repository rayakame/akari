import AkariKit
import Foundation
import Security
import Testing

@Suite(.serialized)
struct KeychainTokenStoreTests {
    static let account = UserId(rawValue: 100_000_000_000_000_001)

    @Test
    func keychainRoundTrip() throws {
        let service = Self.uniqueService()
        let store = KeychainTokenStore(service: service)
        defer { try? store.delete(account: Self.account) }

        #expect(try store.load(account: Self.account) == nil)
        try store.save(account: Self.account, token: "first.token")
        #expect(try store.load(account: Self.account) == "first.token")
        try store.save(account: Self.account, token: "second.token")
        #expect(try store.load(account: Self.account) == "second.token")
        try store.delete(account: Self.account)
        #expect(try store.load(account: Self.account) == nil)
        try store.delete(account: Self.account)
    }

    @Test
    func savedItemsAreNeverSynchronizable() throws {
        let service = Self.uniqueService()
        let store = KeychainTokenStore(service: service)
        defer { try? store.delete(account: Self.account) }

        try store.save(account: Self.account, token: "a.token")
        let query: [CFString: Any] = [
            kSecClass: kSecClassGenericPassword,
            kSecAttrService: service,
            kSecAttrSynchronizable: kSecAttrSynchronizableAny,
            kSecMatchLimit: kSecMatchLimitAll,
            kSecReturnAttributes: true,
        ]
        var result: CFTypeRef?
        #expect(SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess)
        let items = try #require(result as? [[CFString: Any]])

        #expect(items.count == 1)
        #expect(items.first?[kSecAttrAccount] as? String == "100000000000000001")
        #expect((items.first?[kSecAttrSynchronizable] as? Bool ?? false) == false)
    }

    @Test
    func keychainWorksBehindTheClient() async throws {
        let client = try DiscordClient(
            host: HostInfo(osVersion: "25.0.0", systemLocale: "en-US"),
            tokenStore: KeychainTokenStore(service: Self.uniqueService())
        )

        #expect(try await client.loadToken(account: Self.account) == nil)
    }

    static func uniqueService() -> String {
        "app.akari.tests.\(UUID().uuidString)"
    }
}
