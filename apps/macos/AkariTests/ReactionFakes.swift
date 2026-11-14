import AkariKit
import Foundation
import os

// Message 100 in channel 11 with reactions, and who reacted with each emoji; what the emoji
// picker offers.
nonisolated final class ReactionsStore: Store, @unchecked Sendable {
    struct Picker {
        var recent: [PickerEmoji] = []
        var servers: [EmojiSection] = []
        var names: [GuildId: String] = [:]
        var icons: [GuildId: ImageKey] = [:]
        /// What a search for each text finds.
        var found: [String: [PickerEmoji]] = [:]
    }

    let reacted = OSAllocatedUnfairLock<[Reaction]>(initialState: [])
    let picker = OSAllocatedUnfairLock(initialState: Picker())
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

    override func channel(id: ChannelId) -> Channel? { nil }

    override func guild(id: GuildId) -> Guild? {
        picker.withLock { picker in
            picker.names[id].map { Guild(id: id, name: $0, icon: picker.icons[id]) }
        }
    }

    override func recentReactions(channelId: ChannelId, newest: EmojiVersion) -> [PickerEmoji] {
        picker.withLock { $0.recent }
    }

    override func quickReactions(channelId: ChannelId, newest: EmojiVersion) -> [PickerEmoji] {
        ["💯", "😆", "💖"].map(pickerEmoji)
    }

    override func customEmojiSections(channelId: ChannelId) -> [EmojiSection] {
        picker.withLock { $0.servers }
    }

    override func searchEmoji(
        channelId: ChannelId, query: String, newest: EmojiVersion, limit: UInt32
    ) async -> [PickerEmoji] {
        picker.withLock { $0.found[query] ?? [] }
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
    /// Each page asked for, by the user it was asked after.
    let asked = OSAllocatedUnfairLock<[UserId?]>(initialState: [])
    /// Set: no page comes.
    let fails = OSAllocatedUnfairLock(initialState: false)
    /// Set: Discord says there is more after every page, as with more people than it sent.
    let more = OSAllocatedUnfairLock(initialState: false)
    /// The reactions made, by the emoji's name and where from.
    let made = OSAllocatedUnfairLock<[String]>(initialState: [])

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
        asked.withLock { $0.append(after) }
        if fails.withLock({ $0 }) {
            throw RequestError.Network(kind: .other)
        }
        let all = kind == .normal ? people[emoji.name ?? ""] ?? [] : []
        let rest = after.map { after in all.filter { $0.id > after } } ?? all
        let page = Array(rest.prefix(Int(limit)))
        return ReactorsPage(users: page, hasMore: page.count == Int(limit) || more.withLock { $0 })
    }

    override func react(
        channelId: ChannelId, messageId: MessageId, emoji: ReactionEmoji, location: ReactionLocation
    ) async throws {
        made.withLock { $0.append("\(ReactionPill.name(of: emoji)) from \(location)") }
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

nonisolated private let catalog = unicodeEmoji(newest: newestEmojiVersion()).flatMap(\.emojis)

/// A Unicode emoji as the core's picker reads give it.
nonisolated func pickerEmoji(_ emoji: String) -> PickerEmoji {
    .unicode(
        emoji: catalog.first { $0.emoji == emoji }
            ?? UnicodeEmoji(
                emoji: emoji, name: emoji, shortcodes: [], cldrName: emoji, group: .symbols,
                skinTones: false, version: EmojiVersion(major: 6, minor: 0)))
}

nonisolated func pickerCustom(
    _ raw: UInt64, _ name: String, in guild: UInt64 = 1, animated: Bool = false,
    access: EmojiAccess = .usable
) -> CustomEmoji {
    CustomEmoji(
        id: EmojiId(rawValue: raw), guildId: GuildId(rawValue: guild), name: name, animated: animated,
        key: .emoji(emojiId: EmojiId(rawValue: raw), animated: animated), access: access)
}
