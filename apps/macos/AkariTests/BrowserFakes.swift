import AkariKit
import Foundation
import os

// Chris's #new as his screenshots show it: nine joined threads, b1 with Ray's "test" 6h ago and
// the others without a recent message, each with the user's and Ray's faces. `empty` has none.
nonisolated final class FakeBrowserStore: @unchecked Sendable {
    static let parent = ChannelId(rawValue: 11)
    static let guild = GuildId(rawValue: 1)
    static let me = UserId(rawValue: 1)
    static let ray = UserId(rawValue: 2)
    // 2026-10-22 21:32 UTC.
    static let now = Date(timeIntervalSince1970: 1_792_704_720)
    static let names = ["b1", "b4", "b2", "t13", "t2", "t3", "t6", "t5", "t1"]

    var empty = false
    /// Joined threads after the nine.
    var extra = 0
    /// Discord has more older threads to give.
    var more = false
    /// #new shows Ray's "hello", "Ray started a thread: b1. See all threads." and the user's
    /// "test", and the user may send there.
    var lines = false
    /// Pages and searches never answer.
    var holdsLoads = false
    private(set) var account: BrowserAccount?

    @MainActor
    func session() -> SessionModel {
        let store = BrowserStore(empty: empty, extra: extra, more: more, lines: lines)
        let account = BrowserAccount(store: store, holdsLoads: holdsLoads)
        self.account = account
        let session = SessionModel(
            userId: Self.me, account: account,
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.open(.guild(Self.guild))
        return session
    }

    // A snowflake at `date`, and back.
    nonisolated static func snowflake(_ date: Date) -> UInt64 {
        (UInt64(date.timeIntervalSince1970 * 1000) - 1_420_070_400_000) << 22
    }

    nonisolated static func date(_ raw: UInt64) -> Date {
        Date(timeIntervalSince1970: Double((raw >> 22) + 1_420_070_400_000) / 1000)
    }
}

nonisolated final class BrowserAccount: Account, @unchecked Sendable {
    private let browserStore: BrowserStore
    private let holdsLoads: Bool

    init(store: BrowserStore, holdsLoads: Bool = false) {
        browserStore = store
        self.holdsLoads = holdsLoads
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store { browserStore }
    override func loadMessages(channelId: ChannelId, load: MessageLoad, placing: UInt64?)
        async throws -> LoadReport?
    {
        nil
    }
    override func supersedeAroundLoads(channelId: ChannelId) {}
    let olderLoads = OSAllocatedUnfairLock(initialState: 0)

    override func loadOlderThreads(parentId: ChannelId) async throws {
        olderLoads.withLock { $0 += 1 }
        if holdsLoads {
            try await Task.sleep(for: .seconds(60))
        }
    }
    override func setBrowsing(parentId: ChannelId?) {}
    override func searchThreads(parentId: ChannelId, name: String) async throws -> [ChannelId]? {
        if holdsLoads {
            try await Task.sleep(for: .seconds(60))
        }
        return []
    }
    override func reservePlacing() -> UInt64 { 0 }
    override func viewChannel(channelId: ChannelId) {}
    override func setReading(channelIds: [ChannelId]) {}
}

nonisolated final class BrowserStore: Store, @unchecked Sendable {
    private let empty: Bool
    private let more: Bool
    private let lines: Bool
    private let channelsById: [ChannelId: Channel]
    private let joined: [ChannelId]
    static let lineIds = (1...3).map {
        MessageId(rawValue: FakeBrowserStore.snowflake(FakeBrowserStore.now - 3600) + $0)
    }

    init(empty: Bool, extra: Int = 0, more: Bool = false, lines: Bool = false) {
        self.empty = empty
        self.more = more
        self.lines = lines
        var channels: [ChannelId: Channel] = [
            FakeBrowserStore.parent: Channel(
                id: FakeBrowserStore.parent, kind: .guildText, guildId: FakeBrowserStore.guild,
                parentId: nil, name: "new", position: 0, topic: nil, nsfw: false,
                rateLimitPerUser: 0, recipientIds: [], thread: nil)
        ]
        var joined: [ChannelId] = []
        let names = FakeBrowserStore.names + (0..<extra).map { "x\($0)" }
        for (index, name) in names.enumerated() {
            let id = ChannelId(rawValue: 1_000 + UInt64(index))
            let last = FakeBrowserStore.now - TimeInterval(index == 0 ? 6 * 3600 : 86_400 * index)
            channels[id] = Channel(
                id: id, kind: .publicThread, guildId: FakeBrowserStore.guild,
                parentId: FakeBrowserStore.parent, name: name, position: 0, topic: nil,
                nsfw: false, rateLimitPerUser: 0, recipientIds: [],
                thread: ThreadInfo(
                    joined: true, archived: false, locked: false, private: false,
                    messageCount: 3, ownerId: FakeBrowserStore.ray,
                    lastMessageId: MessageId(rawValue: FakeBrowserStore.snowflake(last)),
                    createdAt: nil, memberCount: 2,
                    memberIdsPreview: [FakeBrowserStore.me, FakeBrowserStore.ray],
                    archivedAt: last))
            joined.append(id)
        }
        channelsById = channels
        self.joined = joined
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func threadBrowser(parentId: ChannelId) -> ThreadBrowser {
        ThreadBrowser(
            joined: empty ? [] : joined, others: [], archived: [], older: more ? .more : .all)
    }

    override func latestMessage(threadId: ChannelId) -> Message? {
        guard threadId == joined.first, let info = channelsById[threadId]?.thread,
            let last = info.lastMessageId
        else {
            return nil
        }
        return Message(
            id: last, channelId: threadId, kind: .default, author: user(FakeBrowserStore.ray),
            fromWebhook: false, content: "test",
            timestamp: FakeBrowserStore.date(last.rawValue), editedTimestamp: nil,
            pinned: false, mentionEveryone: false, attachments: [], embedCount: 0,
            stickerNames: [], componentsV2: false, delivery: .sent,
            formatted: formattedText(content: "test"),
            mentionsMe: false, reference: nil, hasThread: false, mentions: [], call: nil,
            roleSubscription: nil, purchasedProduct: nil, actor: nil, applicationName: nil,
            embedFields: [])
    }

    private func user(_ id: UserId) -> User {
        let name = id == FakeBrowserStore.me ? "rayakame" : "Ray"
        return User(
            id: id, username: name, globalName: nil, displayName: name, bot: false, system: false)
    }

    override func connection() -> ConnectionState { .online }
    override func user(id: UserId) -> User? { user(id) }
    override func currentUser() -> User? { user(FakeBrowserStore.me) }
    override func channelList(guildId: GuildId) -> [ChannelId] { [FakeBrowserStore.parent] }
    override func channel(id: ChannelId) -> Channel? { channelsById[id] }
    override func channels(ids: [ChannelId]) -> [Channel] { ids.compactMap { channelsById[$0] } }

    override func permissions(channelId: ChannelId) -> Permissions? {
        lines
            ? [.viewChannel, .readMessageHistory, .sendMessages]
            : [.viewChannel, .readMessageHistory]
    }

    override func guildSettings(guildId: GuildId?) -> GuildSettings {
        GuildSettings(
            muted: false, muteEndsAt: nil, suppressEveryone: false, suppressRoles: false,
            hideMutedChannels: false)
    }

    override func mutedChannels(ids: [ChannelId]) -> [ChannelId] { [] }
    override func window(channelId: ChannelId) -> MessageWindow? {
        guard lines, channelId == FakeBrowserStore.parent else {
            return nil
        }
        return MessageWindow(
            messageIds: Self.lineIds, pendingIds: [], latest: true, oldest: true, stale: false)
    }

    override func messages(channelId: ChannelId, ids: [MessageId]) -> [Message] {
        ids.compactMap { id in
            guard let index = Self.lineIds.firstIndex(of: id) else {
                return nil
            }
            let (author, kind, content): (UserId, MessageType, String) =
                switch index {
                case 0: (FakeBrowserStore.ray, .default, "hello")
                case 1: (FakeBrowserStore.ray, .threadCreated, "b1")
                default: (FakeBrowserStore.me, .default, "test")
                }
            return Message(
                id: id, channelId: channelId, kind: kind, author: user(author),
                fromWebhook: false, content: content,
                timestamp: FakeBrowserStore.date(id.rawValue), editedTimestamp: nil,
                pinned: false, mentionEveryone: false, attachments: [], embedCount: 0,
                stickerNames: [], componentsV2: false, delivery: .sent,
                formatted: formattedText(content: content), mentionsMe: false,
                reference: kind == .threadCreated
                    ? MessageReference(
                        messageId: nil, channelId: joined.first, guildId: FakeBrowserStore.guild)
                    : nil,
                hasThread: false, mentions: [], call: nil, roleSubscription: nil,
                purchasedProduct: nil, actor: nil, applicationName: nil, embedFields: [])
        }
    }
    override func messageLengthLimit() -> UInt32 { 2000 }
    override func slowmode(channelId: ChannelId) -> Slowmode? { nil }

    override func channelBadges(ids: [ChannelId]) -> [ChannelBadge] {
        ids.map { ChannelBadge(channelId: $0, badge: Badge(unread: false, mentions: 0)) }
    }

    override func guildBadges(ids: [GuildId]) -> [GuildBadge] {
        ids.map { GuildBadge(guildId: $0, badge: Badge(unread: false, mentions: 0)) }
    }

    override func unreadPrivateChannels() -> [ChannelId] { [] }
    override func readState(channelId: ChannelId) -> ReadState? { nil }
}
