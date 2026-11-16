import AkariKit
import Foundation

final class FakeStore: Store, @unchecked Sendable {
    struct State {
        var connection: ConnectionState = .offline
        var currentUser: User?
        var guildIds: [GuildId] = []
        // `nil`: `guildIds`, none in a folder.
        var serverList: [ServerListItem]?
        var guildSettings: [GuildId?: GuildSettings] = [:]
        var displaySettings = displayDefaults()
        var mutedChannels: Set<ChannelId> = []
        var guilds: [GuildId: Guild] = [:]
        var unavailableGuildIds: [GuildId] = []
        var channelLists: [GuildId: [ChannelId]] = [:]
        var privateChannels: [ChannelId] = []
        var users: [UserId: User] = [:]
        var channels: [ChannelId: Channel] = [:]
        var permissions: [ChannelId: Permissions] = [:]
        var windows: [ChannelId: MessageWindow] = [:]
        var messages: [MessageId: Message] = [:]
        var reactions: [MessageId: [Reaction]] = [:]
        // By the target's ID; none: not loaded.
        var replyTargets: [MessageId: ReplyTarget] = [:]
        var unreadPins: Set<ChannelId> = []
        // By the channel and message asked with. None: nothing is allowed.
        var messageActions: [MessageKey: MessageActions] = [:]
        var lengthLimit: UInt32 = 2000
        var slowmodes: [ChannelId: Slowmode] = [:]
        var channelBadges: [ChannelId: Badge] = [:]
        var guildBadges: [GuildId: Badge] = [:]
        var readStates: [ChannelId: ReadState] = [:]
        var unreadPrivateChannels: [ChannelId] = []
        var starters: [ChannelId: StarterMessage] = [:]
        var latestMessages: [ChannelId: Message] = [:]
        var threadBrowsers: [ChannelId: ThreadBrowser] = [:]
        var unavailableThreads: Set<ChannelId> = []
        var threadCreations: [ChannelId: ThreadCreation] = [:]
        // Messages that can start a thread.
        var startable: Set<MessageId> = []
        var threadSlowmodes: [ChannelId: Slowmode] = [:]
        var postCreations: [ChannelId: PostCreation] = [:]
        var ageGates: [ChannelId: AgeGate] = [:]
        var hidingDownloads: Set<ChannelId> = []
        // Each forum's page for a view; none: nothing listed, nothing loaded.
        var forumPages: [ChannelId: @Sendable (ForumView) -> ForumPosts] = [:]
        var postPreviews: [ChannelId: PostPreview] = [:]
        var unreadPosts: Set<ChannelId> = []
        var newPosts: Set<ChannelId> = []
        // `nil` inside: Discord hasn't said how many.
        var postNewCounts: [ChannelId: UInt32?] = [:]

        mutating func add(_ guilds: Guild...) {
            for guild in guilds {
                guildIds.append(guild.id)
                self.guilds[guild.id] = guild
            }
        }

        mutating func list(_ channels: [Channel], in guild: UInt64) {
            channelLists[id(guild)] = channels.map(\.id)
            for channel in channels {
                self.channels[channel.id] = channel
            }
        }

        mutating func talk(_ channels: Channel..., with users: User...) {
            for channel in channels {
                privateChannels.append(channel.id)
                self.channels[channel.id] = channel
            }
            for user in users {
                self.users[user.id] = user
            }
        }

        mutating func show(_ messages: Message...) {
            for message in messages {
                self.messages[message.id] = message
            }
        }
    }

    let subscription: FakeSubscription
    let log: CallLog
    private let state: Locked<State>

    init(_ state: State = State(), subscription: FakeSubscription = FakeSubscription()) {
        self.state = Locked(state)
        self.subscription = subscription
        self.log = CallLog()
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    var reads: [CallLog.Read] { log.reads }

    func update(_ change: (inout State) -> Void) {
        state.withLock(change)
    }

    private func read<T>(_ read: CallLog.Read, _ value: (State) -> T) -> T {
        log.append(.read(read))
        return value(state.current)
    }

    override func subscribe() -> StoreSubscription {
        log.append(.subscribe)
        return subscription
    }

    override func connection() -> ConnectionState {
        read(.connection) { $0.connection }
    }

    override func currentUser() -> User? {
        read(.currentUser) { $0.currentUser }
    }

    override func user(id: UserId) -> User? {
        read(.user(id)) { $0.users[id] }
    }

    override func guildIds() -> [GuildId] {
        read(.guildIds) { $0.guildIds }
    }

    override func serverList() -> [ServerListItem] {
        read(.serverList) { state in
            state.serverList ?? state.guildIds.map { .guild(guildId: $0) }
        }
    }

    override func guildSettings(guildId: GuildId?) -> GuildSettings {
        read(.guildSettings(guildId)) { $0.guildSettings[guildId] ?? settings() }
    }

    override func displaySettings() -> DisplaySettings {
        read(.displaySettings) { $0.displaySettings }
    }

    override func ageGate(channelId: ChannelId) -> AgeGate {
        state.current.ageGates[channelId] ?? .open
    }

    override func mutedChannels(ids: [ChannelId]) -> [ChannelId] {
        read(.mutedChannels(ids)) { state in ids.filter(state.mutedChannels.contains) }
    }

    override func guild(id: GuildId) -> Guild? {
        read(.guild(id)) { $0.guilds[id] }
    }

    override func unavailableGuildIds() -> [GuildId] {
        read(.unavailableGuildIds) { $0.unavailableGuildIds }
    }

    override func channelList(guildId: GuildId) -> [ChannelId] {
        read(.channelList(guildId)) { $0.channelLists[guildId] ?? [] }
    }

    override func privateChannelList() -> [ChannelId] {
        read(.privateChannelList) { $0.privateChannels }
    }

    override func channel(id: ChannelId) -> Channel? {
        read(.channel(id)) { $0.channels[id] }
    }

    override func channels(ids: [ChannelId]) -> [Channel] {
        read(.channels(ids)) { state in ids.compactMap { state.channels[$0] } }
    }

    override func permissions(channelId: ChannelId) -> Permissions? {
        read(.permissions(channelId)) { $0.permissions[channelId] }
    }

    override func window(channelId: ChannelId) -> MessageWindow? {
        read(.window(channelId)) { $0.windows[channelId] }
    }

    override func messages(channelId: ChannelId, ids: [MessageId]) -> [Message] {
        read(.messages(channelId, ids)) { state in
            ids.compactMap { state.messages[$0] }.filter { $0.channelId == channelId }
        }
    }

    override func reactions(channelId: ChannelId, messageId: MessageId) -> [Reaction] {
        read(.reactions(channelId, messageId)) { $0.reactions[messageId] ?? [] }
    }

    override func messageReactions(channelId: ChannelId, messageIds: [MessageId]) -> [MessageReactions] {
        read(.messageReactions(channelId, messageIds)) { state in
            messageIds.compactMap { id in
                state.reactions[id].flatMap { reactions in
                    reactions.isEmpty ? nil : MessageReactions(messageId: id, reactions: reactions)
                }
            }
        }
    }

    override func replyTarget(channelId: ChannelId, targetId: MessageId) -> ReplyTarget {
        read(.replyTarget(channelId, targetId)) { $0.replyTargets[targetId] ?? .notLoaded }
    }

    override func replyTargets(channelId: ChannelId, targetIds: [MessageId]) -> [ReplyTarget] {
        read(.replyTargets(channelId, targetIds)) { state in
            targetIds.map { state.replyTargets[$0] ?? .notLoaded }
        }
    }

    override func pinsUnread(channelId: ChannelId) -> Bool {
        read(.pinsUnread(channelId)) { $0.unreadPins.contains(channelId) }
    }

    override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
        read(.messageActions(channelId, messageId)) { state in
            state.messageActions[MessageKey(channelId, messageId)]
                ?? MessageActions(
                    reply: false, replyPings: false, mentionToggle: false, edit: false, delete: false,
                    pin: false, unpin: false, react: false, toggleReaction: false,
                    removeReactions: false, markUnread: false, copyLink: false, createThread: false,
                    viewThread: false)
        }
    }

    // The core builds the link; the fake names what it was asked for.
    override func messageLink(channelId: ChannelId, messageId: MessageId) -> String {
        read(.messageLink(channelId, messageId)) { _ in
            "link \(channelId.rawValue) \(messageId.rawValue)"
        }
    }

    override func threadUnavailable(threadId: ChannelId) -> Bool {
        read(.threadUnavailable(threadId)) { $0.unavailableThreads.contains(threadId) }
    }

    override func latestMessage(threadId: ChannelId) -> Message? {
        read(.latestMessage(threadId)) { $0.latestMessages[threadId] }
    }

    override func threadCreation(parentId: ChannelId) -> ThreadCreation {
        read(.threadCreation(parentId)) { state in
            state.threadCreations[parentId] ?? ThreadCreation(public: false, private: false)
        }
    }

    override func canStartThread(channelId: ChannelId, messageId: MessageId) -> Bool {
        read(.canStartThread(channelId, messageId)) { $0.startable.contains(messageId) }
    }

    override func postCreation(forumId: ChannelId) -> PostCreation {
        read(.postCreation(forumId)) { state in
            state.postCreations[forumId] ?? PostCreation(allowed: false, tags: [], requiresTag: false)
        }
    }

    override func threadSlowmode(parentId: ChannelId) -> Slowmode? {
        read(.threadSlowmode(parentId)) { $0.threadSlowmodes[parentId] }
    }

    override func threadBrowser(parentId: ChannelId) -> ThreadBrowser {
        read(.threadBrowser(parentId)) { state in
            state.threadBrowsers[parentId]
                ?? ThreadBrowser(joined: [], others: [], archived: [], older: .notLoaded)
        }
    }

    override func starterMessage(threadId: ChannelId) -> StarterMessage? {
        read(.starterMessage(threadId)) { $0.starters[threadId] }
    }

    override func messageLengthLimit() -> UInt32 {
        read(.messageLengthLimit) { $0.lengthLimit }
    }

    override func slowmode(channelId: ChannelId) -> Slowmode? {
        read(.slowmode(channelId)) { $0.slowmodes[channelId] }
    }

    override func channelBadges(ids: [ChannelId]) -> [ChannelBadge] {
        read(.channelBadges(ids)) { state in
            ids.map { ChannelBadge(channelId: $0, badge: state.channelBadges[$0] ?? noBadge) }
        }
    }

    override func guildBadges(ids: [GuildId]) -> [GuildBadge] {
        read(.guildBadges(ids)) { state in
            ids.map { GuildBadge(guildId: $0, badge: state.guildBadges[$0] ?? noBadge) }
        }
    }

    override func readState(channelId: ChannelId) -> ReadState? {
        read(.readState(channelId)) { $0.readStates[channelId] }
    }

    override func unreadPrivateChannels() -> [ChannelId] {
        read(.unreadPrivateChannels) { $0.unreadPrivateChannels }
    }

    override func hidesMediaDownloads(channelId: ChannelId) -> Bool {
        read(.hidesMediaDownloads(channelId)) { $0.hidingDownloads.contains(channelId) }
    }

    override func forumPosts(forumId: ChannelId, view: ForumView) -> ForumPosts {
        read(.forumPosts(forumId, view)) { state in
            state.forumPages[forumId]?(view)
                ?? ForumPosts(active: [], archived: [], older: .notLoaded, newPosts: 0)
        }
    }

    override func postPreview(postId: ChannelId) -> PostPreview? {
        read(.postPreview(postId)) { $0.postPreviews[postId] }
    }

    override func unreadPosts(postIds: [ChannelId]) -> [ChannelId] {
        read(.unreadPosts(postIds)) { state in postIds.filter(state.unreadPosts.contains) }
    }

    override func postNewCounts(postIds: [ChannelId]) -> [PostNewCount] {
        read(.postNewCounts(postIds)) { state in
            postIds.compactMap { post in
                state.postNewCounts[post].map { PostNewCount(postId: post, count: $0) }
            }
        }
    }

    override func newPosts(postIds: [ChannelId]) -> [ChannelId] {
        read(.newPosts(postIds)) { state in postIds.filter(state.newPosts.contains) }
    }
}

// A message as the store is asked about it: a thread's starter row and the message it shows have
// the same answer only where the core says so.
struct MessageKey: Hashable {
    let channel: ChannelId
    let message: MessageId

    init(_ channel: ChannelId, _ message: MessageId) {
        self.channel = channel
        self.message = message
    }
}

extension FakeStore.State {
    mutating func allow(_ actions: MessageActions?, message: UInt64, in channel: UInt64) {
        messageActions[MessageKey(id(channel), id(message))] = actions
    }
}
