import Foundation

/// What a hover card and a popout are about: a custom emoji, a Unicode emoji (with its name,
/// `emojiShortcode`) or a sticker in a message.
public enum ExpressionTarget: Equatable, Sendable {
    case customEmoji(id: EmojiId, name: String, animated: Bool)
    case unicodeEmoji(String, name: String)
    case sticker(Sticker)

    /// The hover card the official client shows after 300 ms over it.
    public var card: ExpressionCard {
        switch self {
        case .customEmoji(let id, let name, let animated):
            ExpressionCard(
                picture: .image(.emoji(emojiId: id, animated: animated)), title: ":\(name):",
                prompt: ExpressionText.learnMore)
        case .unicodeEmoji(let glyph, let name):
            ExpressionCard(picture: .glyph(glyph), title: ":\(name):", prompt: ExpressionText.learnMore)
        case .sticker(let sticker):
            ExpressionCard(picture: .stickerSymbol, title: sticker.name, prompt: ExpressionText.learnMore)
        }
    }
}

/// A hover card's content: the emoji at 48 points (or a sticker symbol beside the name), the
/// title, and the prompt.
public struct ExpressionCard: Equatable, Sendable {
    public enum Picture: Equatable, Sendable {
        case image(ImageKey)
        case glyph(String)
        case stickerSymbol
    }

    public let picture: Picture
    public let title: String
    public let prompt: String

    public init(picture: Picture, title: String, prompt: String) {
        self.picture = picture
        self.title = title
        self.prompt = prompt
    }
}

/// A popout's content, decided by the official client's rules from the store and Discord's
/// answers. Akari leaves out what it can't do: joining a server, Nitro, Discord's help articles.
public struct ExpressionPopout: Equatable, Sendable {
    public enum Picture: Equatable, Sendable {
        case emoji(ImageKey)
        case glyph(String)
        case sticker(Sticker)
    }

    public struct Run: Equatable, Sendable {
        public let text: String
        public let bold: Bool

        public init(text: String, bold: Bool) {
            self.text = text
            self.bold = bold
        }
    }

    /// The server an emoji or a sticker is from.
    public struct Server: Equatable, Sendable {
        public enum Badge: Equatable, Sendable {
            case verified
            case partnered
        }

        public enum Line: Equatable, Sendable {
            /// A discoverable server the user isn't in.
            case online(Int)
            /// A discoverable server the user is in.
            case discoverable(Int)
            case inviteOnly
        }

        public let heading: String
        public let id: GuildId
        public let name: String
        public let icon: ImageKey?
        public let badge: Badge?
        public let line: Line
        /// The user is in it: a click opens it.
        public let opens: Bool

        public var lineText: String {
            switch line {
            case .online(let count): ExpressionText.online(count)
            case .discoverable(let count): "\(ExpressionText.online(count)) · \(ExpressionText.discoverableServer)"
            case .inviteOnly: ExpressionText.inviteOnlyServer
            }
        }
    }

    public enum More: Equatable, Sendable {
        case emoji([SourceEmoji])
        case stickers([Sticker])
    }

    public var picture: Picture?
    public var title: String
    public var body: [Run] = []
    public var server: Server?
    /// "Show More Emoji" or "Show More Stickers": up to 12 more from a server the user isn't in.
    public var more: More?
    public var moreLabel: String?
    /// A pack's other stickers, up to three.
    public var others: [Sticker] = []
    public var loading: Bool
}

/// The store reads a popout needs; `Store` answers them.
public protocol ExpressionStore {
    func emojiGuild(emojiId: EmojiId) -> GuildId?
    func guild(id: GuildId) -> Guild?
    func nitro() -> Nitro
}

extension Store: ExpressionStore {}

/// Discord's answers a popout needs; `Account` asks them.
public protocol ExpressionLookups: Sendable {
    func emojiSource(emojiId: EmojiId) async throws -> EmojiSource
    func sticker(stickerId: StickerId) async throws -> StickerDetails
    func stickerGuild(stickerId: StickerId) async throws -> SourceGuild
    func stickerPack(packId: StickerPackId) async throws -> StickerPack
}

extension Account: ExpressionLookups {}

/// One popout: its content at once from what's known, then from Discord's answers.
@MainActor @Observable
public final class ExpressionPopoutModel {
    public let target: ExpressionTarget
    public private(set) var content: ExpressionPopout
    /// The server of the message's channel: "this server".
    @ObservationIgnored public let channelGuild: GuildId?
    @ObservationIgnored private let store: any ExpressionStore
    @ObservationIgnored private let lookups: any ExpressionLookups
    @ObservationIgnored private let sourceTimeout: Duration

    /// `channelGuild` is the server of the channel the message is in ("this server").
    public init(
        target: ExpressionTarget, channelGuild: GuildId?, store: any ExpressionStore,
        lookups: any ExpressionLookups, locale: Locale = .current,
        sourceTimeout: Duration = .seconds(5)
    ) {
        self.target = target
        self.channelGuild = channelGuild
        self.store = store
        self.lookups = lookups
        self.sourceTimeout = sourceTimeout
        content = Self.initial(target, locale: locale)
    }

    public func load() async {
        switch target {
        case .unicodeEmoji:
            return
        case .customEmoji(let id, _, _):
            let source = await Self.within(sourceTimeout) { [lookups] in
                try await lookups.emojiSource(emojiId: id)
            }
            content = describeEmoji(id, source: source)
        case .sticker(let sticker):
            content = await describeSticker(sticker)
        }
    }

    private static func initial(_ target: ExpressionTarget, locale: Locale) -> ExpressionPopout {
        switch target {
        case .customEmoji(let id, let name, let animated):
            return ExpressionPopout(
                picture: .emoji(.emoji(emojiId: id, animated: animated)), title: ":\(name):",
                loading: true)
        case .unicodeEmoji(let glyph, let name):
            let english = locale.identifier.hasPrefix("en")
            let title = name == "pizza" && english ? ":pizza: (without pineapple)" : ":\(name):"
            return ExpressionPopout(
                picture: .glyph(glyph), title: title,
                body: [.init(text: ExpressionText.defaultEmoji, bold: false)], loading: false)
        case .sticker(let sticker):
            return ExpressionPopout(picture: .sticker(sticker), title: sticker.name, loading: true)
        }
    }

    private func describeEmoji(_ id: EmojiId, source: EmojiSource?) -> ExpressionPopout {
        var content = content
        content.loading = false
        if case .application(let name) = source {
            content.body = [.init(text: ExpressionText.emojiFromApp(name), bold: false)]
            return content
        }
        let found: SourceGuild? = if case .guild(let guild) = source { guild } else { nil }
        let local = store.emojiGuild(emojiId: id).flatMap { store.guild(id: $0) }
        let words = Words.emoji
        content.body = [.init(text: body(words, local: local, found: found, nitro: store.nitro() != .none), bold: false)]
        content.server = server(words, local: local, found: found)
        if local == nil, let found, found.discoverable {
            let others = found.emojis.filter { $0.id != id }
            if !others.isEmpty {
                content.more = .emoji(Array(others.prefix(12)))
                content.moreLabel = ExpressionText.showMoreEmoji
            }
        }
        return content
    }

    private func describeSticker(_ sticker: Sticker) async -> ExpressionPopout {
        var content = content
        content.loading = false
        let details = try? await lookups.sticker(stickerId: sticker.id)
        switch details?.origin {
        case .pack(let packId):
            content.picture = nil
            if let pack = try? await lookups.stickerPack(packId: packId) {
                content.body = [
                    .init(text: "This is a ", bold: false), .init(text: pack.name, bold: true),
                    .init(text: " sticker.", bold: false),
                ]
                content.others = Array(pack.stickers.filter { $0.id != sticker.id }.prefix(3))
            }
        case .guild(let guildId):
            let found = try? await lookups.stickerGuild(stickerId: sticker.id)
            let local = store.guild(id: guildId)
            let nitro = [.nitro, .basic].contains(store.nitro())
            let words = Words.sticker
            content.body = [.init(text: body(words, local: local, found: found, nitro: nitro), bold: false)]
            content.server = server(words, local: local, found: found)
            if local == nil, let found, found.discoverable {
                let others = found.stickers.filter { $0.id != sticker.id }
                if !others.isEmpty {
                    content.more = .stickers(Array(others.prefix(12)))
                    content.moreLabel = ExpressionText.showMoreStickers
                }
            }
        default:
            content.body = [.init(text: ExpressionText.unavailable(Words.sticker), bold: false)]
        }
        return content
    }

    // The official matrix: in one of the user's servers (this one or another), a discoverable
    // one they're not in, or neither; with or without Nitro.
    private func body(_ words: Words, local: Guild?, found: SourceGuild?, nitro: Bool) -> String {
        if let local {
            let here = local.id == channelGuild
            return ExpressionText.fromYourServer(words, here: here, nitro: nitro)
        }
        if found?.discoverable == true {
            return ExpressionText.joinToUse(words, nitro: nitro)
        }
        return ExpressionText.unavailable(words)
    }

    private func server(_ words: Words, local: Guild?, found: SourceGuild?) -> ExpressionPopout.Server? {
        let heading = local == nil ? words.getFrom : words.isFrom
        if let found {
            return ExpressionPopout.Server(
                heading: heading, id: found.id, name: found.name, icon: found.icon,
                badge: found.verified ? .verified : found.partnered ? .partnered : nil,
                line: found.presenceCount.map { count in
                    local == nil ? .online(Int(count)) : .discoverable(Int(count))
                } ?? .inviteOnly,
                opens: local != nil)
        }
        if let local {
            return ExpressionPopout.Server(
                heading: heading, id: local.id, name: local.name, icon: local.icon, badge: nil,
                line: .inviteOnly, opens: true)
        }
        return nil
    }

    // The answer, or `nil` after a failure or `limit`. A task group would wait for the call,
    // which UniFFI's futures never let go of; two loose tasks race, the first one answers.
    private static func within<T: Sendable>(
        _ limit: Duration, _ work: @escaping @Sendable () async throws -> T
    ) async -> T? {
        let answered = OnceBox<T?>()
        return await withCheckedContinuation { continuation in
            answered.waiting(continuation)
            let call = Task {
                answered.answer(try? await work())
            }
            Task {
                try? await Task.sleep(for: limit)
                answered.answer(nil)
                call.cancel()
            }
        }
    }
}

// Whose words: an emoji's or a sticker's.
struct Words: Sendable {
    let noun: String
    let isFrom: String
    let getFrom: String

    static let emoji = Words(noun: "emoji", isFrom: "This emoji is from", getFrom: "Get this emoji from")
    static let sticker = Words(
        noun: "sticker", isFrom: "This sticker is from", getFrom: "Get this sticker from")
}

/// The official client's en-US words (canary 649848).
enum ExpressionText {
    static let learnMore = "Click to learn more"
    static let defaultEmoji = "A default emoji. You can use this emoji everywhere on Discord."
    static let discoverableServer = "Discoverable Server"
    static let inviteOnlyServer = "Invite-only server"
    static let showMoreEmoji = "Show More Emoji"
    static let showMoreStickers = "Show More Stickers"

    static func online(_ count: Int) -> String {
        "\(count.formatted(.number.locale(Locale(identifier: "en_US")))) Online"
    }

    static func emojiFromApp(_ name: String) -> String {
        "This emoji is from the \(name) app"
    }

    static func fromYourServer(_ words: Words, here: Bool, nitro: Bool) -> String {
        let place = here ? "this server" : "one of your servers"
        switch (here, nitro) {
        case (true, true): return "This \(words.noun) is from this server. You can use it everywhere."
        case (false, true):
            return "This \(words.noun) is from \(place). Type its name in the chat bar to use it."
        case (_, false): return "This \(words.noun) is from \(place). Get Nitro to use it everywhere."
        }
    }

    static func joinToUse(_ words: Words, nitro: Bool) -> String {
        nitro
            ? "Want to use this \(words.noun) everywhere? Join the server."
            : "Want to use this \(words.noun) everywhere? Get Nitro and join the server."
    }

    static func unavailable(_ words: Words) -> String {
        "This \(words.noun) is from a server that is either invite-only or unavailable."
    }
}

// A continuation resumed by the first answer only.
private final class OnceBox<T: Sendable>: @unchecked Sendable {
    private let lock = NSLock()
    private var continuation: CheckedContinuation<T, Never>?

    func waiting(_ continuation: CheckedContinuation<T, Never>) {
        lock.withLock { self.continuation = continuation }
    }

    func answer(_ value: T) {
        let waiting = lock.withLock {
            defer { continuation = nil }
            return continuation
        }
        waiting?.resume(returning: value)
    }
}
