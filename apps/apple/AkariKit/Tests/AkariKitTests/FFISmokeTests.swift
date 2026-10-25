import AkariKit
import Foundation
import Testing

// The real bindings, without network: nothing listens on port 9, so connections are refused
// at once.
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
        try DiscordClient.withEndpoints(
            host: host, tokenStore: store, endpoints: unreachable, cacheDirectory: nil)
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
        let account = try client.account(token: token, userId: Self.user)
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
        let account = try client.account(token: token, userId: Self.user)
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

    @Test
    func messageLengthCountsCodePoints() {
        #expect(messageLength(content: "👍🏽") == 2)
        #expect("👍🏽".count == 1)
        #expect("👍🏽".utf16.count == 4)
    }

    @Test @MainActor
    func endSessionWithoutNetworkReportsNetwork() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let client = try Self.client(store)
        let token = try #require(try await client.loadToken(account: Self.user))

        await #expect(throws: LogoutError.Network(kind: .connect)) {
            try await client.endSession(token: token)
        }
    }

    @Test
    func imagesCrossAsPendingRequests() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let client = try Self.client(store)
        let token = try #require(try await client.loadToken(account: Self.user))
        let account = try client.account(token: token, userId: Self.user)
        let request = ImageRequest(
            key: .avatar(userId: Self.user, hash: "0123456789abcdef0123456789abcdef"),
            width: 80, height: 80, animated: .webP)

        let pending = account.requestImage(request: request)

        await #expect(throws: ImageError.Unavailable) { try await pending.result() }
        await #expect(throws: ImageError.Unavailable, "every result() gets the same answer") {
            try await pending.result()
        }
        account.close()
    }

    // Connecting (to nothing) keeps the account off its offline fast path, and a CDN that
    // accepts the connection and never answers keeps `result()` waiting until the cancel.
    static func stalled(_ store: MemoryTokenStore, images: URL?) async throws -> (DiscordClient, StallingHost) {
        let stalling = try await StallingHost.start()
        let endpoints = Endpoints(
            api: unreachable.api, gateway: unreachable.gateway, remoteAuth: unreachable.remoteAuth,
            origin: unreachable.origin, cdn: stalling.url, media: stalling.url)
        let client = try DiscordClient.withEndpoints(
            host: host, tokenStore: store, endpoints: endpoints, cacheDirectory: nil,
            imageDirectory: images?.path)
        return (client, stalling)
    }

    @Test
    func aCancelledTaskCancelsTheImageRequest() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let images = FileManager.default.temporaryDirectory
            .appendingPathComponent("akari-images-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: images) }
        let (client, stalling) = try await Self.stalled(store, images: images)
        defer { stalling.stop() }
        let token = try #require(try await client.loadToken(account: Self.user))
        let account = try client.account(token: token, userId: Self.user)
        try account.connect()
        let request = ImageRequest(
            key: .avatar(userId: Self.user, hash: "0123456789abcdef0123456789abcdef"),
            width: 80, height: 80, animated: nil)
        let loading = Task { try await account.image(request) }

        try await Task.sleep(for: .milliseconds(100))
        loading.cancel()
        let outcome = await loading.result

        #expect(throws: ImageError.Cancelled) { try outcome.get() }
        account.close()
    }

    @Test
    func aCancelledTaskCancelsTheSave() async throws {
        let store = MemoryTokenStore(tokens: [Self.user: "stored.token"])
        let (client, stalling) = try await Self.stalled(store, images: nil)
        defer { stalling.stop() }
        let token = try #require(try await client.loadToken(account: Self.user))
        let account = try client.account(token: token, userId: Self.user)
        try account.connect()
        let destination = FileManager.default.temporaryDirectory
            .appendingPathComponent("akari-save-\(UUID().uuidString).png")
        try Data("the old file".utf8).write(to: destination)
        defer { try? FileManager.default.removeItem(at: destination) }
        let saving = Task {
            try await account.saveOriginal(
                .avatar(userId: Self.user, hash: "0123456789abcdef0123456789abcdef"),
                to: destination)
        }

        try await Task.sleep(for: .milliseconds(100))
        saving.cancel()
        let outcome = await saving.result

        #expect(throws: ImageError.Cancelled) { try outcome.get() }
        #expect(try Data(contentsOf: destination) == Data("the old file".utf8))
        account.close()
    }

    @Test
    func placeholdersAndLayoutCross() throws {
        let placeholder = try #require(placeholderImage(thumbhash: "6BdS9LIx6ztgCNvPNYoH8Kd5wr5s46fPJx0"))
        #expect(placeholder.width == 32 && placeholder.height == 18)
        #expect(placeholder.rgba.count == 32 * 18 * 4)

        let mosaic = mediaMosaic(items: Array(repeating: MediaSize(width: 1000, height: 1000), count: 3))
        #expect(mosaic.cells.map(\.width) == [364, 182, 182])
        #expect(mediaContain(source: MediaSize(width: 1920, height: 1080), max: MediaSize(width: 550, height: 350))
            == MediaSize(width: 550, height: 309))
    }
}
