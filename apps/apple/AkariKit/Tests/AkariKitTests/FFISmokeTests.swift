import AkariKit
import Foundation
import Testing

/// The real bindings, without network: nothing listens on port 9, so connections are
/// refused at once.
@Suite(.timeLimit(.minutes(1)))
struct FFISmokeTests {
    static let user = UserId(rawValue: 100_000_000_000_000_001)
    static let host = HostInfo(osVersion: "25.0.0", systemLocale: "en-US")
    static let unreachable = Endpoints(
        api: "https://127.0.0.1:9/api/v9/",
        gateway: "wss://127.0.0.1:9/",
        remoteAuth: "wss://127.0.0.1:9/?v=2",
        origin: "https://discord.com"
    )

    static func client(_ store: MemoryTokenStore) throws -> DiscordClient {
        try DiscordClient.withEndpoints(host: host, tokenStore: store, endpoints: unreachable)
    }

    @Test @MainActor
    func tokenStoreCallsRunOffTheMainThread() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let client = try Self.client(store)

        let token = try await client.loadToken(account: Self.user)
        await #expect(throws: LogoutError.Network(kind: .connect)) {
            try await client.logout(account: Self.user)
        }

        #expect(token != nil)
        #expect(store.calls.contains(.delete(Self.user)))
        #expect(store.token(for: Self.user) == nil)
        #expect(store.callsOnMainThread == 0)
    }

    @Test
    func loginErrorsAreSwiftErrorsWithCoreText() async throws {
        let client = try Self.client(MemoryTokenStore())

        do {
            _ = try await client.passwordLogin().submit(login: "me@example.com", password: "x")
            Issue.record("the login didn't fail")
        } catch let error as LoginError {
            #expect(error == .Network(kind: .connect))
            #expect(error.localizedDescription == "network error")
        }
    }

    @Test
    func subscriptionDeliversConnectionChanges() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let client = try Self.client(store)
        let token = try #require(try await client.loadToken(account: Self.user))
        let account = try client.account(token: token)
        let subscription = account.store().subscribe()

        try account.connect()
        var events: [StoreEvent] = []
        while !events.contains(.connection(state: .connecting)) {
            events += try #require(await subscription.next())
        }
        account.close()
        while let batch = await subscription.next() {
            events += batch
        }

        #expect(events.last == .connection(state: .closed(error: nil)))
    }

    @Test
    func idsAreOrderedAndReadsOfUnknownIdsAreEmpty() async throws {
        let client = try Self.client(MemoryTokenStore(tokens: [Self.user: "stored.token"]))
        let token = try #require(try await client.loadToken(account: Self.user))
        let account = try client.account(token: token)
        let reads = account.store()

        #expect(ChannelId(rawValue: 2) > ChannelId(rawValue: 1))
        #expect(reads.channel(id: ChannelId(rawValue: 1)) == nil)
        #expect(reads.channels(ids: [ChannelId(rawValue: 1)]).isEmpty)
        #expect(reads.connection() == .offline)
        account.close()
    }

    @Test
    func permissionBitsMatchTheCore() {
        #expect(Permissions.viewChannel.rawValue == 1 << 10)
        #expect(Permissions.sendMessages.rawValue == 1 << 11)
        #expect(Permissions.sendMessagesInThreads.rawValue == 1 << 38)
        #expect(Permissions.all.contains(.administrator))
    }
}
