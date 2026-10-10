import AkariKit
import Foundation

// An online account with more of everything than a window shows: 40 servers, 60 channels in
// server 1, 60 DMs, and 100 messages in each conversation.
nonisolated final class CrowdedAccount: Account, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store {
        CrowdedStore()
    }

    override func connect() throws {}

    override func loadMessages(channelId: ChannelId, load: MessageLoad) async throws {}

    override func viewChannel(channelId: ChannelId) {}
}

nonisolated final class CrowdedStore: Store, @unchecked Sendable {
    static let guildIds = (1...40).map { GuildId(rawValue: $0) }
    static let channelIds = (1001...1060).map { ChannelId(rawValue: $0) }
    static let dmIds = (5001...5060).map { ChannelId(rawValue: $0) }

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    private func person(_ raw: UInt64) -> User {
        User(
            id: UserId(rawValue: raw), username: "user\(raw)", globalName: nil,
            displayName: "User \(raw)", bot: false, system: false)
    }

    override func subscribe() -> StoreSubscription {
        IdleSubscription()
    }

    override func connection() -> ConnectionState {
        .online
    }

    override func currentUser() -> User? {
        person(1)
    }

    override func user(id: UserId) -> User? {
        person(id.rawValue)
    }

    override func guildIds() -> [GuildId] {
        Self.guildIds
    }

    override func guild(id: GuildId) -> Guild? {
        Self.guildIds.contains(id) ? Guild(id: id, name: "Server \(id.rawValue)") : nil
    }

    override func unavailableGuildIds() -> [GuildId] {
        []
    }

    override func channelList(guildId: GuildId) -> [ChannelId] {
        guildId.rawValue == 1 ? Self.channelIds : []
    }

    override func privateChannelList() -> [ChannelId] {
        Self.dmIds
    }

    override func channel(id: ChannelId) -> Channel? {
        if Self.channelIds.contains(id) {
            return Channel(
                id: id, kind: .guildText, guildId: GuildId(rawValue: 1), parentId: nil,
                name: "channel-\(id.rawValue)", position: Int32(id.rawValue), topic: nil,
                nsfw: false, rateLimitPerUser: 0, recipientIds: [])
        }
        if Self.dmIds.contains(id) {
            return Channel(
                id: id, kind: .dm, guildId: nil, parentId: nil, name: nil, position: 0,
                topic: nil, nsfw: false, rateLimitPerUser: 0,
                recipientIds: [UserId(rawValue: id.rawValue + 2000)])
        }
        return nil
    }

    override func channels(ids: [ChannelId]) -> [Channel] {
        ids.compactMap { channel(id: $0) }
    }

    override func permissions(channelId: ChannelId) -> Permissions? {
        nil
    }

    override func window(channelId: ChannelId) -> MessageWindow? {
        guard channel(id: channelId) != nil else {
            return nil
        }
        let base = channelId.rawValue * 1000
        return MessageWindow(
            messageIds: (1...100).map { MessageId(rawValue: base + $0) }, pendingIds: [],
            latest: true, oldest: true, stale: false)
    }

    override func messages(channelId: ChannelId, ids: [MessageId]) -> [Message] {
        ids.map { id in
            Message(
                id: id, channelId: channelId, kind: .default, author: person(2),
                fromWebhook: false, content: "message \(id.rawValue)",
                timestamp: Date(timeIntervalSince1970: 1_700_000_000 + Double(id.rawValue)),
                editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
                embedCount: 0, stickerNames: [], componentsV2: false, delivery: .sent)
        }
    }

    override func messageLengthLimit() -> UInt32 {
        2000
    }

    override func slowmode(channelId: ChannelId) -> Slowmode? {
        nil
    }
}

// Never delivers events; ends when the session closes it.
nonisolated final class IdleSubscription: StoreSubscription, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func next() async -> [StoreEvent]? {
        try? await Task.sleep(for: .seconds(3600))
        return nil
    }

    override func close() {}
}
