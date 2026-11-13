import AkariKit
import Foundation
import os

// Message 100 in channel 11 with reactions, and who reacted with each emoji.
nonisolated final class ReactionsStore: Store, @unchecked Sendable {
    let reacted = OSAllocatedUnfairLock<[Reaction]>(initialState: [])
    let moderates: Bool

    init(_ reactions: [Reaction], moderates: Bool = false) {
        self.moderates = moderates
        super.init(noHandle: NoHandle())
        reacted.withLock { $0 = reactions }
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func displaySettings() -> DisplaySettings { displayDefaults() }

    override func reactions(channelId: ChannelId, messageId: MessageId) -> [Reaction] {
        reacted.withLock { $0 }
    }

    override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
        var actions = testMessageActions()
        actions.react = true
        actions.toggleReaction = true
        actions.removeReactions = moderates
        return actions
    }
}

nonisolated final class ReactionsAccount: Account, @unchecked Sendable {
    static let channel = ChannelId(rawValue: 11)
    static let message = MessageId(rawValue: 100)
    static let me = UserId(rawValue: 1)

    let reactionsStore: ReactionsStore
    /// Who reacted, by the emoji's name.
    let people: [String: [User]]
    let removed = OSAllocatedUnfairLock<[UserId]>(initialState: [])

    init(_ reactions: [Reaction], people: [String: [User]], moderates: Bool = false) {
        reactionsStore = ReactionsStore(reactions, moderates: moderates)
        self.people = people
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store { reactionsStore }
    override func requestImage(request: ImageRequest) -> PendingImage { UnavailableImage() }
    override func setReading(channelIds: [ChannelId]) {}
    override func setOpenChannels(channelIds: [ChannelId]) {}

    override func reactors(
        channelId: ChannelId, messageId: MessageId, emoji: ReactionEmoji, kind: ReactionKind,
        after: UserId?, limit: UInt8
    ) async throws -> ReactorsPage {
        let all = kind == .normal ? people[emoji.name ?? ""] ?? [] : []
        let rest = after.map { after in all.filter { $0.id > after } } ?? all
        let page = Array(rest.prefix(Int(limit)))
        return ReactorsPage(users: page, hasMore: page.count == Int(limit))
    }

    override func removeReaction(
        channelId: ChannelId, messageId: MessageId, emoji: ReactionEmoji, kind: ReactionKind,
        userId: UserId
    ) async throws {
        removed.withLock { $0.append(userId) }
    }

    /// A session whose reactions window is open on the message.
    @MainActor
    func window() -> SessionModel {
        let session = SessionModel(
            userId: Self.me, account: self, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.viewReactions(channelId: Self.channel, messageId: Self.message)
        return session
    }
}

nonisolated func reaction(_ emoji: String, _ count: UInt32, me: Bool = false) -> Reaction {
    Reaction(
        emoji: ReactionEmoji(id: nil, name: emoji, animated: false, key: nil), normal: count,
        burst: 0, me: me, meBurst: false)
}

nonisolated func person(_ raw: UInt64, _ name: String, _ username: String) -> User {
    User(
        id: UserId(rawValue: raw), username: username, globalName: name, displayName: name,
        bot: false, system: false, avatar: .defaultAvatar(index: UInt8(raw % 6)))
}
