import AkariKit
import Foundation
import os

// One server with two text channels (1001, 1002) of formatted messages; a channel's window can
// grow while it isn't shown, as when messages arrive.
nonisolated final class FormattedAccount: Account, @unchecked Sendable {
    override func requestImage(request: ImageRequest) -> PendingImage { UnavailableImage() }

    let formattedStore: FormattedStore

    // `locked`: 1001 is a locked thread under 1002, where the user can't send. `pinned`: each
    // channel's newest message is the line for pinning its message with this number. `thread`:
    // the store also has 1003, a thread under 1001 the user hasn't joined, for the split view;
    // without `threadState` its record has no thread state. `unread`: every channel's badge
    // shows unread.
    init(
        contents: [String], formattedEvery: Int = 3, slowmode: Slowmode? = nil,
        mentioned: Bool = false, readUpTo: UInt64? = nil, locked: Bool = false,
        pinned: UInt64? = nil, thread: Bool = false, threadState: Bool = true,
        unread: Bool = false
    ) {
        formattedStore = FormattedStore(
            contents: contents, formattedEvery: formattedEvery, slowmode: slowmode,
            mentioned: mentioned, readUpTo: readUpTo, locked: locked, pinned: pinned,
            thread: thread, threadState: threadState, unread: unread)
        super.init(noHandle: NoHandle())
    }

    let markedRead = OSAllocatedUnfairLock<[ChannelId]>(initialState: [])

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store {
        formattedStore
    }

    override func connect() throws {}

    override func loadMessages(channelId: ChannelId, load: MessageLoad, placing: UInt64?)
        async throws -> LoadReport?
    { nil }

    override func reservePlacing() -> UInt64 { 0 }
    override func viewChannel(channelId: ChannelId) {}
    override func supersedeAroundLoads(channelId: ChannelId) {}
    override func setReading(channelIds: [ChannelId]) {}
    override func markRead(channelId: ChannelId) {
        markedRead.withLock { $0.append(channelId) }
    }

    let markedGuildsRead = OSAllocatedUnfairLock<[[GuildId]]>(initialState: [])

    override func markGuildsRead(guildIds: [GuildId]) {
        markedGuildsRead.withLock { $0.append(guildIds) }
    }
    override func fetchThread(parentId: ChannelId, threadId: ChannelId) async throws -> Bool {
        formattedStore.channel(id: threadId) != nil
    }

    override func checkThread(parentId: ChannelId, threadId: ChannelId) async throws -> Bool {
        formattedStore.channel(id: threadId) != nil
    }
}

nonisolated final class FormattedStore: Store, @unchecked Sendable {
    override func displaySettings() -> DisplaySettings { displayDefaults() }

    static let channelIds = [ChannelId(rawValue: 1001), ChannelId(rawValue: 1002)]
    static let threadId = ChannelId(rawValue: 1003)

    private let contents: [String]
    private let formattedEvery: Int
    private let slowmode: Slowmode?
    private let mentioned: Bool
    // The number of each channel's last read message; without it every channel is read.
    private let readUpTo: UInt64?
    private let locked: Bool
    private let pinned: UInt64?
    private let thread: Bool
    private let threadState: Bool
    private let unread: Bool
    private let counts = OSAllocatedUnfairLock(initialState: [UInt64: UInt64]())

    init(
        contents: [String], formattedEvery: Int, slowmode: Slowmode?, mentioned: Bool,
        readUpTo: UInt64?, locked: Bool = false, pinned: UInt64? = nil, thread: Bool = false,
        threadState: Bool = true, unread: Bool = false
    ) {
        self.contents = contents
        self.formattedEvery = formattedEvery
        self.slowmode = slowmode
        self.mentioned = mentioned
        self.readUpTo = readUpTo
        self.locked = locked
        self.pinned = pinned
        self.thread = thread
        self.threadState = threadState
        self.unread = unread
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    func add(_ count: UInt64, to channel: ChannelId) {
        counts.withLock { $0[channel.rawValue, default: 40] += count }
    }

    func newest(of channel: ChannelId) -> MessageId {
        MessageId(rawValue: channel.rawValue * 1000 + counts.withLock { $0[channel.rawValue, default: 40] })
    }

    private func person(_ raw: UInt64) -> User {
        User(
            id: UserId(rawValue: raw), username: "user\(raw)", globalName: nil,
            displayName: "User \(raw)", bot: false, system: false,
            avatar: .defaultAvatar(index: UInt8(raw % 6)))
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
        [GuildId(rawValue: 1)]
    }

    override func serverList() -> [ServerListItem] {
        guildIds().map { .guild(guildId: $0) }
    }

    override func guildSettings(guildId: GuildId?) -> GuildSettings {
        GuildSettings(
            muted: false, muteEndsAt: nil, suppressEveryone: false, suppressRoles: false,
            hideMutedChannels: false)
    }

    override func mutedChannels(ids: [ChannelId]) -> [ChannelId] {
        []
    }

    override func guild(id: GuildId) -> Guild? {
        id.rawValue == 1 ? Guild(id: id, name: "Server") : nil
    }

    override func unavailableGuildIds() -> [GuildId] {
        []
    }

    override func channelList(guildId: GuildId) -> [ChannelId] {
        guildId.rawValue == 1 ? Self.channelIds : []
    }

    override func privateChannelList() -> [ChannelId] {
        []
    }

    private func has(_ id: ChannelId) -> Bool {
        Self.channelIds.contains(id) || (thread && id == Self.threadId)
    }

    override func channel(id: ChannelId) -> Channel? {
        if thread && id == Self.threadId {
            return Channel(
                id: id, kind: .publicThread, guildId: GuildId(rawValue: 1),
                parentId: Self.channelIds[0], name: "t1", position: 0, topic: nil, nsfw: false,
                rateLimitPerUser: 0, recipientIds: [],
                thread: threadState
                    ? ThreadInfo(
                        joined: false, archived: false, locked: false, private: false,
                        messageCount: 40, ownerId: UserId(rawValue: 2), lastMessageId: nil,
                        createdAt: nil, memberCount: nil, memberIdsPreview: [],
                        archivedAt: Date(timeIntervalSince1970: 0))
                    : nil)
        }
        guard Self.channelIds.contains(id) else {
            return nil
        }
        let thread = locked && id == Self.channelIds[0]
        return Channel(
            id: id, kind: thread ? .publicThread : .guildText, guildId: GuildId(rawValue: 1),
            parentId: thread ? Self.channelIds[1] : nil,
            name: "channel-\(id.rawValue)", position: Int32(id.rawValue), topic: nil,
            nsfw: false, rateLimitPerUser: 0, recipientIds: [],
            thread: thread
                ? ThreadInfo(
                    joined: true, archived: false, locked: true, private: false,
                    messageCount: 40, ownerId: UserId(rawValue: 2), lastMessageId: nil,
                    createdAt: nil, memberCount: nil, memberIdsPreview: [],
                    archivedAt: Date(timeIntervalSince1970: 0))
                : nil)
    }

    override func channels(ids: [ChannelId]) -> [Channel] {
        ids.compactMap { channel(id: $0) }
    }

    override func permissions(channelId: ChannelId) -> Permissions? {
        if locked && channelId == Self.channelIds[0] {
            return [.viewChannel, .readMessageHistory]
        }
        // The split view's composers can take the focus only where the user may send.
        return thread ? [.viewChannel, .sendMessages, .readMessageHistory] : nil
    }

    override func window(channelId: ChannelId) -> MessageWindow? {
        guard has(channelId) else {
            return nil
        }
        let count = counts.withLock { $0[channelId.rawValue, default: 40] }
        let base = channelId.rawValue * 1000
        return MessageWindow(
            messageIds: (1...count).map { MessageId(rawValue: base + $0) }, pendingIds: [],
            latest: true, oldest: true, stale: false)
    }

    // Every so many messages formatted, the rest one plain line.
    override func messages(channelId: ChannelId, ids: [MessageId]) -> [Message] {
        let newest = counts.withLock { $0[channelId.rawValue, default: 40] }
        return ids.map { id in
            let number = Int(id.rawValue % 1000)
            let content =
                number % formattedEvery == 0
                ? contents[number / formattedEvery % contents.count] : "test \(number)"
            let pin = pinned.flatMap { pinned in
                UInt64(number) == newest
                    ? MessageReference(
                        messageId: MessageId(rawValue: channelId.rawValue * 1000 + pinned),
                        channelId: channelId, guildId: GuildId(rawValue: 1)) : nil
            }
            return Message(
                id: id, channelId: channelId, kind: pin == nil ? .default : .channelPinnedMessage,
                author: person(UInt64(2 + number % 2)),
                fromWebhook: false, content: content,
                timestamp: Date(timeIntervalSince1970: 1_700_000_000 + Double(number) * 600),
                editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
                embedCount: 0, stickers: [], componentsV2: false, delivery: .sent,
                formatted: formattedText(content: content), mentionsMe: mentioned,
                reference: pin, hasThread: false, mentions: [],
                call: nil, roleSubscription: nil, purchasedProduct: nil, actor: nil,
                applicationName: nil, embedFields: [])
        }
    }

    override func messageLengthLimit() -> UInt32 {
        2000
    }

    // Both kinds in the text channels; no message starts one, no cooldown.
    override func threadCreation(parentId: ChannelId) -> ThreadCreation {
        ThreadCreation(public: true, private: true)
    }

    override func canStartThread(channelId: ChannelId, messageId: MessageId) -> Bool { false }
    override func threadSlowmode(parentId: ChannelId) -> Slowmode? { nil }

    override func slowmode(channelId: ChannelId) -> Slowmode? {
        slowmode
    }

    override func channelBadges(ids: [ChannelId]) -> [ChannelBadge] {
        ids.map { ChannelBadge(channelId: $0, badge: Badge(unread: unread, mentions: 0)) }
    }

    override func guildBadges(ids: [GuildId]) -> [GuildBadge] {
        ids.map { GuildBadge(guildId: $0, badge: Badge(unread: false, mentions: 0)) }
    }

    override func unreadPrivateChannels() -> [ChannelId] {
        []
    }

    override func readState(channelId: ChannelId) -> ReadState? {
        readUpTo.map { number in
            ReadState(
                acked: MessageId(rawValue: channelId.rawValue * 1000 + number), mentions: 0,
                estimated: false)
        }
    }
}
