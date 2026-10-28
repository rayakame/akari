import AkariKit
import Foundation
import os

// A guild with #ideas, a forum with four tags, guidelines and five posts: one pinned, one with a
// picture, one whose first message is gone; `posts` sets how many show, `archived` how many
// older ones follow.
nonisolated struct ForumFakes {
    static let guild = GuildId(rawValue: 1)
    static let forum = ChannelId(rawValue: 13)
    static let me = UserId(rawValue: 1)
    // 2026-10-22 21:32 UTC.
    static let now = Date(timeIntervalSince1970: 1_792_704_720)
    static let tags = [
        ForumTag(id: TagId(rawValue: 501), name: "Bug", moderated: false, emoji: .unicode(emoji: "🐛")),
        ForumTag(id: TagId(rawValue: 502), name: "Feature", moderated: false, emoji: .unicode(emoji: "✨")),
        ForumTag(id: TagId(rawValue: 503), name: "Question", moderated: false, emoji: nil),
        ForumTag(id: TagId(rawValue: 504), name: "Staff", moderated: true, emoji: nil),
    ]
    static let titles = [
        "Welcome to #ideas: read this first",
        "Dark mode for the settings window",
        "The app crashes when I paste a very long message into the composer and press Enter twice",
        "How do folders sync between devices?",
        "Old idea nobody remembers",
    ]

    var posts = 5
    var archived = 0
    var layout = ForumLayout.list
    var newPosts: UInt32 = 0
    var canReadHistory = true
    var topic: String? = "Be kind. Search before you post. One idea per post."
    private(set) var account: ForumAccount?

    @MainActor
    mutating func session() -> SessionModel {
        let store = ForumStore(fakes: self)
        let account = ForumAccount(store: store)
        self.account = account
        let session = SessionModel(
            userId: Self.me, account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.open(.guild(Self.guild))
        session.open(channel: Self.forum)
        return session
    }

    static func snowflake(_ date: Date) -> UInt64 {
        (UInt64(date.timeIntervalSince1970 * 1000) - 1_420_070_400_000) << 22
    }

    static func post(_ index: Int) -> ChannelId {
        ChannelId(rawValue: snowflake(now - TimeInterval(3600 * (index + 1) * 5)))
    }
}

nonisolated final class ForumAccount: Account, @unchecked Sendable {
    override func requestImage(request: ImageRequest) -> PendingImage { UnavailableImage() }

    private let forumStore: ForumStore
    let refreshes = OSAllocatedUnfairLock(initialState: 0)
    let previewRequests = OSAllocatedUnfairLock<[[ChannelId]]>(initialState: [])

    init(store: ForumStore) {
        forumStore = store
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store { forumStore }
    override func openForum(forumId: ChannelId?) {}
    override func refreshForum(forumId: ChannelId) { refreshes.withLock { $0 += 1 } }
    override func loadOlderPosts(forumId: ChannelId, view: ForumView) async throws {}
    override func searchPosts(forumId: ChannelId, name: String, view: ForumView) async throws
        -> [ChannelId]?
    {
        []
    }
    override func requestPostPreviews(forumId: ChannelId, postIds: [ChannelId]) {
        previewRequests.withLock { $0.append(postIds) }
    }
    override func viewChannel(channelId: ChannelId) {}
    override func setReading(channelIds: [ChannelId]) {}
}

nonisolated final class ForumStore: Store, @unchecked Sendable {
    private let fakes: ForumFakes
    private let channelsById: [ChannelId: Channel]

    init(fakes: ForumFakes) {
        self.fakes = fakes
        var forum = Channel(
            id: ForumFakes.forum, kind: .guildForum, guildId: ForumFakes.guild, parentId: nil,
            name: "ideas", position: 0, topic: fakes.topic, nsfw: false, rateLimitPerUser: 0,
            recipientIds: [], thread: nil)
        forum.forum = ForumSettings(
            tags: ForumFakes.tags, defaultReaction: nil, sort: .latestActivity,
            layout: fakes.layout, tagMatching: .some, postSlowmode: 0, template: nil)
        var channels = [ForumFakes.forum: forum]
        for index in 0..<(fakes.posts + fakes.archived) {
            let id = ForumFakes.post(index)
            let tags: [TagId] =
                switch index % 5 {
                case 0: [ForumFakes.tags[3].id]
                case 1: [ForumFakes.tags[1].id]
                case 2: ForumFakes.tags.map(\.id)
                case 3: [ForumFakes.tags[2].id]
                default: []
                }
            channels[id] = Channel(
                id: id, kind: .publicThread, guildId: ForumFakes.guild, parentId: ForumFakes.forum,
                name: ForumFakes.titles[index % 5], position: 0, topic: nil, nsfw: false,
                rateLimitPerUser: 0, recipientIds: [],
                thread: ThreadInfo(
                    joined: false, archived: index >= fakes.posts, locked: false, private: false,
                    messageCount: UInt32(index * 7 + 2), ownerId: UserId(rawValue: 2),
                    lastMessageId: MessageId(rawValue: id.rawValue + 1), createdAt: nil,
                    memberCount: nil, memberIdsPreview: [],
                    archivedAt: ForumFakes.now, appliedTags: tags, pinned: index == 0))
        }
        channelsById = channels
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func displaySettings() -> DisplaySettings { displayDefaults() }
    override func ageGate(channelId: ChannelId) -> AgeGate { .open }
    override func connection() -> ConnectionState { .online }
    override func currentUser() -> User? { forumUser(ForumFakes.me, name: "rayakame") }
    override func user(id: UserId) -> User? { forumUser(id, name: "Ray") }
    override func guildIds() -> [GuildId] { [ForumFakes.guild] }
    override func serverList() -> [ServerListItem] { [.guild(guildId: ForumFakes.guild)] }
    override func guild(id: GuildId) -> Guild? { Guild(id: id, name: "Akari") }
    override func channelList(guildId: GuildId) -> [ChannelId] { [ForumFakes.forum] }
    override func channel(id: ChannelId) -> Channel? { channelsById[id] }
    override func channels(ids: [ChannelId]) -> [Channel] { ids.compactMap { channelsById[$0] } }
    override func permissions(channelId: ChannelId) -> Permissions? {
        fakes.canReadHistory ? [.viewChannel, .readMessageHistory] : [.viewChannel]
    }

    override func threadCreation(parentId: ChannelId) -> ThreadCreation {
        ThreadCreation(public: false, private: false)
    }
    override func canStartThread(channelId: ChannelId, messageId: MessageId) -> Bool { false }
    override func threadSlowmode(parentId: ChannelId) -> Slowmode? { nil }
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
    override func unavailableGuildIds() -> [GuildId] { [] }
    override func privateChannelList() -> [ChannelId] { [] }
    override func hidesMediaDownloads(channelId: ChannelId) -> Bool { false }
    override func latestMessage(threadId: ChannelId) -> Message? { nil }
    override func threadBrowser(parentId: ChannelId) -> ThreadBrowser {
        ThreadBrowser(joined: [], others: [], archived: [], older: .all)
    }

    override func forumPosts(forumId: ChannelId, view: ForumView) -> ForumPosts {
        let posts = (0..<fakes.posts).map(ForumFakes.post).filter { post in
            view.tags.isEmpty
                || channelsById[post]?.thread?.appliedTags.contains(where: view.tags.contains) == true
        }
        return ForumPosts(
            active: posts, archived: (fakes.posts..<(fakes.posts + fakes.archived)).map(ForumFakes.post),
            older: .all, newPosts: fakes.newPosts)
    }

    override func postPreview(postId: ChannelId) -> PostPreview? {
        guard let index = (0..<(fakes.posts + fakes.archived)).first(where: { ForumFakes.post($0) == postId })
        else {
            return nil
        }
        switch index % 5 {
        case 3:
            return .deleted
        case 4:
            return nil
        default:
            var message = Message(
                id: MessageId(rawValue: postId.rawValue), channelId: postId, kind: .default,
                author: forumUser(UserId(rawValue: 2), name: "Ray"), fromWebhook: false,
                content: index == 0
                    ? "Please search before posting, and give your post a clear title."
                    : "It would be great if the settings window followed the system's appearance.",
                timestamp: ForumFakes.now, editedTimestamp: nil, pinned: false,
                mentionEveryone: false, attachments: [], embeds: [], stickers: [],
                componentsV2: false, delivery: .sent, formatted: formattedText(content: ""), mentionsMe: false,
                reference: nil, hasThread: false, mentions: [], call: nil, roleSubscription: nil,
                purchasedProduct: nil, actor: nil, applicationName: nil, embedFields: [])
            message.formatted = formattedText(content: message.content)
            if index == 1 {
                message.attachments = [forumPicture(post: postId)]
            }
            return .loaded(message: message)
        }
    }
}

nonisolated func forumUser(_ id: UserId, name: String) -> User {
    User(
        id: id, username: name, globalName: nil, displayName: name, bot: false, system: false,
        avatar: .defaultAvatar(index: UInt8(id.rawValue % 6)))
}

nonisolated func forumPicture(post: ChannelId) -> AkariFFI.Attachment {
    AkariFFI.Attachment(
        id: AttachmentId(rawValue: 9), filename: "settings.png", title: nil, description: nil,
        contentType: "image/png", size: 1000, kind: .image, width: 800, height: 600,
        placeholder: "3PcNNYSFeXh/d3eld0iHZoZgVwh2", spoiler: false, animated: false,
        key: .attachment(
            channelId: post, messageId: MessageId(rawValue: post.rawValue),
            attachmentId: AttachmentId(rawValue: 9)),
        display: .media, cover: nil)
}
