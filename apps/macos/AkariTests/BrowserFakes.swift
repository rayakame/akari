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
    /// What the user may create in #new; Ray's "hello" (the first line) can start a thread.
    var creation = ThreadCreation(public: false, private: false)
    var threadSlowmode: Slowmode?
    /// Ray's "hello" says it `longFirstLine` times, a line each.
    var longFirstLine = 1
    /// Ray's first line ends in a custom emoji.
    var firstLineEmoji = false
    var pins = 0
    var pinsState = PinsState.loaded
    var pinsMore = false
    var pinsDot = false
    var pinsLines = false
    var unpinnable = true
    // The second line is Ray's pin of the first pin.
    var pinLine = false
    private(set) var account: BrowserAccount?

    @MainActor
    func session() -> SessionModel {
        let store = BrowserStore(
            empty: empty, extra: extra, more: more, lines: lines, creation: creation,
            threadSlowmode: threadSlowmode, longFirstLine: longFirstLine,
            firstLineEmoji: firstLineEmoji,
            pins: PinsPage(
                items: (0..<pins).map(Self.pinned), hasMore: pinsMore, state: pinsState),
            pinsDot: pinsDot, pinsLines: pinsLines, unpinnable: unpinnable, pinLine: pinLine)
        let account = BrowserAccount(store: store, holdsLoads: holdsLoads)
        self.account = account
        let session = SessionModel(
            userId: Self.me, account: account,
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.open(.guild(Self.guild))
        return session
    }

    static let pinTexts = [
        "The release is on Friday. Please test the build before then!",
        "Notes from today's call:\n- the pins popout is in\n- reactions on cards next week",
        "Welcome to #new! Read the pinned messages first.",
    ]

    // Each a day older than the one before; Ray's but the second.
    nonisolated static func pinned(_ index: Int) -> PinnedMessage {
        let date = now - TimeInterval(86_400 * (index + 1))
        let id = MessageId(rawValue: snowflake(date))
        let author = index == 1 ? me : ray
        let name = author == me ? "rayakame" : "Ray"
        let content = index < pinTexts.count ? pinTexts[index] : "Pinned message \(index + 1)"
        let message = Message(
            id: id, channelId: parent, kind: .default,
            author: User(
                id: author, username: name, globalName: nil, displayName: name, bot: false,
                system: false, avatar: .defaultAvatar(index: UInt8(author.rawValue % 6))),
            fromWebhook: false, content: content, timestamp: date, editedTimestamp: nil, pinned: true,
            mentionEveryone: false, attachments: [], embeds: [], stickers: [], componentsV2: false,
            delivery: .sent, formatted: formattedText(content: content), mentionsMe: false,
            reference: nil, hasThread: false, mentions: [], call: nil, roleSubscription: nil,
            purchasedProduct: nil, actor: nil, applicationName: nil, embedFields: [])
        return PinnedMessage(message: message, pinnedAt: date)
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
    override func requestImage(request: ImageRequest) -> PendingImage { UnavailableImage() }

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
    override func close() {}
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
    let created = OSAllocatedUnfairLock<[String]>(initialState: [])

    // Nothing is created: the form stays as it is.
    override func createThread(
        parentId: ChannelId, messageId: MessageId?, name: String, isPrivate: Bool,
        location: CreateThreadLocation
    ) async throws -> ChannelId {
        created.withLock { $0.append(name) }
        throw CreateThreadError.Slowmode
    }
    override func searchThreads(parentId: ChannelId, name: String) async throws -> [ChannelId]? {
        if holdsLoads {
            try await Task.sleep(for: .seconds(60))
        }
        return []
    }
    override func reservePlacing() -> UInt64 { 0 }
    override func viewChannel(channelId: ChannelId) {}
    override func setReading(channelIds: [ChannelId]) {}

    override func setOpenChannels(channelIds: [ChannelId]) {}

    // "load", "more", "ack", "pin <message>", "unpin <message>", as asked.
    let pinCalls = OSAllocatedUnfairLock<[String]>(initialState: [])

    override func loadPins(channelId: ChannelId) async throws {
        pinCalls.withLock { $0.append("load") }
    }

    override func loadMorePins(channelId: ChannelId) async throws {
        pinCalls.withLock { $0.append("more") }
    }

    override func ackPins(channelId: ChannelId) {
        pinCalls.withLock { $0.append("ack") }
    }

    override func pin(channelId: ChannelId, messageId: MessageId) async throws {
        pinCalls.withLock { $0.append("pin \(messageId.rawValue)") }
    }

    override func unpin(channelId: ChannelId, messageId: MessageId) async throws {
        pinCalls.withLock { $0.append("unpin \(messageId.rawValue)") }
    }
}

nonisolated final class BrowserStore: Store, @unchecked Sendable {
    override func displaySettings() -> DisplaySettings { displayDefaults() }
    override func reactions(channelId: ChannelId, messageId: MessageId) -> [Reaction] { [] }
    override func messageReactions(channelId: ChannelId, messageIds: [MessageId]) -> [MessageReactions] { [] }
    override func ageGate(channelId: ChannelId) -> AgeGate { .open }

    private let empty: Bool
    private let more: Bool
    private let lines: Bool
    private let creation: ThreadCreation
    private let slowmode: Slowmode?
    private let longFirstLine: Int
    private let firstLineEmoji: Bool
    private let channelsById: [ChannelId: Channel]
    private let joined: [ChannelId]
    private let pinsPage: PinsPage
    private let pinsDot: Bool
    private let pinsLines: Bool
    private let unpinnable: Bool
    private let pinLine: Bool
    static let lineIds = (1...3).map {
        MessageId(rawValue: FakeBrowserStore.snowflake(FakeBrowserStore.now - 3600) + $0)
    }

    init(
        empty: Bool, extra: Int = 0, more: Bool = false, lines: Bool = false,
        creation: ThreadCreation = ThreadCreation(public: false, private: false),
        threadSlowmode: Slowmode? = nil, longFirstLine: Int = 1, firstLineEmoji: Bool = false,
        pins: PinsPage = PinsPage(items: [], hasMore: false, state: .loaded), pinsDot: Bool = false,
        pinsLines: Bool = false, unpinnable: Bool = true, pinLine: Bool = false
    ) {
        pinsPage = pins
        self.pinsDot = pinsDot
        self.pinsLines = pinsLines
        self.unpinnable = unpinnable
        self.pinLine = pinLine
        self.longFirstLine = longFirstLine
        self.firstLineEmoji = firstLineEmoji
        self.empty = empty
        self.more = more
        self.lines = lines
        self.creation = creation
        slowmode = threadSlowmode
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

    override func hidesMediaDownloads(channelId: ChannelId) -> Bool { false }

    override func threadCreation(parentId: ChannelId) -> ThreadCreation {
        parentId == FakeBrowserStore.parent ? creation : ThreadCreation(public: false, private: false)
    }

    override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
        testMessageActions(
            createThread: canStartThread(channelId: channelId, messageId: messageId),
            pin: pinsLines && Self.lineIds.contains(messageId),
            unpin: unpinnable && pinsPage.items.contains { $0.message.id == messageId })
    }

    override func pins(channelId: ChannelId) -> PinsPage {
        channelId == FakeBrowserStore.parent
            ? pinsPage : PinsPage(items: [], hasMore: false, state: .notLoaded)
    }

    override func pinsUnread(channelId: ChannelId) -> Bool {
        pinsDot && channelId == FakeBrowserStore.parent
    }

    override func canStartThread(channelId: ChannelId, messageId: MessageId) -> Bool {
        creation.public && messageId == Self.lineIds[0]
    }

    override func threadSlowmode(parentId: ChannelId) -> Slowmode? { slowmode }

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
            pinned: false, mentionEveryone: false, attachments: [], embeds: [],
            stickers: [], componentsV2: false, delivery: .sent,
            formatted: formattedText(content: "test"),
            mentionsMe: false, reference: nil, hasThread: false, mentions: [], call: nil,
            roleSubscription: nil, purchasedProduct: nil, actor: nil, applicationName: nil,
            embedFields: [])
    }

    private func user(_ id: UserId) -> User {
        let name = id == FakeBrowserStore.me ? "rayakame" : "Ray"
        return User(
            id: id, username: name, globalName: nil, displayName: name, bot: false, system: false,
            avatar: .defaultAvatar(index: UInt8(id.rawValue % 6)))
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
                case 0:
                    (
                        FakeBrowserStore.ray, .default,
                        Array(repeating: "hello", count: longFirstLine).joined(separator: "\n")
                            + (firstLineEmoji ? " <:pog:5>" : "")
                    )
                case 1 where pinLine: (FakeBrowserStore.ray, .channelPinnedMessage, "")
                case 1: (FakeBrowserStore.ray, .threadCreated, "b1")
                default: (FakeBrowserStore.me, .default, "test")
                }
            return Message(
                id: id, channelId: channelId, kind: kind, author: user(author),
                fromWebhook: false, content: content,
                timestamp: FakeBrowserStore.date(id.rawValue), editedTimestamp: nil,
                pinned: false, mentionEveryone: false, attachments: [], embeds: [],
                stickers: [], componentsV2: false, delivery: .sent,
                formatted: formattedText(content: content), mentionsMe: false,
                reference: kind == .threadCreated
                    ? MessageReference(
                        messageId: nil, channelId: joined.first, guildId: FakeBrowserStore.guild)
                    : kind == .channelPinnedMessage
                        ? MessageReference(
                            messageId: pinsPage.items.first?.message.id, channelId: channelId,
                            guildId: FakeBrowserStore.guild)
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
