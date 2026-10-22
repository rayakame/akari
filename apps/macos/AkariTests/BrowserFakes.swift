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

    @MainActor
    func session() -> SessionModel {
        let store = BrowserStore(empty: empty)
        let session = SessionModel(
            userId: Self.me, account: BrowserAccount(store: store),
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

    init(store: BrowserStore) {
        browserStore = store
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
    override func loadOlderThreads(parentId: ChannelId) async throws {}
    override func searchThreads(parentId: ChannelId, name: String) async throws -> [ChannelId]? { [] }
    override func reservePlacing() -> UInt64 { 0 }
    override func viewChannel(channelId: ChannelId) {}
    override func setReading(channelIds: [ChannelId]) {}
}

nonisolated final class BrowserStore: Store, @unchecked Sendable {
    private let empty: Bool
    private let channelsById: [ChannelId: Channel]
    private let joined: [ChannelId]

    init(empty: Bool) {
        self.empty = empty
        var channels: [ChannelId: Channel] = [
            FakeBrowserStore.parent: Channel(
                id: FakeBrowserStore.parent, kind: .guildText, guildId: FakeBrowserStore.guild,
                parentId: nil, name: "new", position: 0, topic: nil, nsfw: false,
                rateLimitPerUser: 0, recipientIds: [], thread: nil)
        ]
        var joined: [ChannelId] = []
        for (index, name) in FakeBrowserStore.names.enumerated() {
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
            joined: empty ? [] : joined, others: [], archived: [], older: .all)
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
        [.viewChannel, .readMessageHistory]
    }

    override func guildSettings(guildId: GuildId?) -> GuildSettings {
        GuildSettings(
            muted: false, muteEndsAt: nil, suppressEveryone: false, suppressRoles: false,
            hideMutedChannels: false)
    }

    override func mutedChannels(ids: [ChannelId]) -> [ChannelId] { [] }
    override func window(channelId: ChannelId) -> MessageWindow? { nil }
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
