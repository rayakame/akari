import Foundation
import Testing

@testable import AkariKit

// A custom emoji's, a Unicode emoji's and a sticker's hover card and popout, as the official
// client words and decides them (canary 649848; Akari leaves out joining, buying and help links).
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ExpressionPopoutTests {
    static let here = id(200_000_000_000_000_001) as GuildId
    static let elsewhere = id(200_000_000_000_000_002) as GuildId
    static let hub = id(200_000_000_000_000_050) as GuildId
    static let wave = id(700_000_000_000_000_001) as EmojiId
    static let hubIcon = ImageKey.guildIcon(guildId: hub, hash: "0123456789abcdef0123456789abcdef")

    struct Store: ExpressionStore {
        var emojis: [EmojiId: GuildId] = [:]
        var guilds: [GuildId: Guild] = [:]
        var tier = Nitro.none
        var down: [GuildId] = []

        func emojiGuild(emojiId: EmojiId) -> GuildId? { emojis[emojiId] }
        func guild(id: GuildId) -> Guild? { guilds[id] }
        func nitro() -> Nitro { tier }
        func unavailableGuildIds() -> [GuildId] { down }
    }

    final class Lookups: ExpressionLookups, @unchecked Sendable {
        var source: Result<EmojiSource, Error> = .failure(RequestError.UnexpectedResponse)
        var details: Result<StickerDetails, Error> = .failure(RequestError.UnexpectedResponse)
        var stickerGuild: Result<SourceGuild, Error> = .failure(RequestError.UnexpectedResponse)
        var pack: Result<StickerPack, Error> = .failure(RequestError.UnexpectedResponse)
        var delay: Duration = .zero
        // UniFFI's futures ignore cancellation: a wait that only time ends.
        var deaf = false
        let asked = Locked<[String]>([])

        func emojiSource(emojiId: EmojiId) async throws -> EmojiSource {
            asked.withLock { $0.append("source") }
            if deaf {
                let seconds = Double(delay.components.seconds)
                await withCheckedContinuation { continuation in
                    DispatchQueue.global().asyncAfter(deadline: .now() + seconds) { continuation.resume() }
                }
            } else {
                try await Task.sleep(for: delay)
            }
            return try source.get()
        }

        func sticker(stickerId: StickerId) async throws -> StickerDetails {
            asked.withLock { $0.append("sticker") }
            return try details.get()
        }

        func stickerGuild(stickerId: StickerId) async throws -> SourceGuild {
            asked.withLock { $0.append("stickerGuild") }
            return try stickerGuild.get()
        }

        func stickerPack(packId: StickerPackId) async throws -> StickerPack {
            asked.withLock { $0.append("pack") }
            return try pack.get()
        }
    }

    static func hubGuild(discoverable: Bool = true, presence: UInt32? = 1234, emojis: Int = 3)
        -> SourceGuild
    {
        SourceGuild(
            id: hub, name: "Emoji Hub", icon: hubIcon, discoverable: discoverable, verified: true,
            partnered: false, presenceCount: presence,
            emojis: (0..<emojis).map { index in
                let emoji = id(700_000_000_000_000_001 + UInt64(index)) as EmojiId
                return SourceEmoji(
                    id: emoji, name: "e\(index)", animated: false,
                    key: .emoji(emojiId: emoji, animated: false))
            },
            stickers: [])
    }

    static func sticker(_ raw: UInt64, _ name: String = "wave", format: StickerFormatType = .png)
        -> Sticker
    {
        let sticker = id(raw) as StickerId
        return Sticker(
            id: sticker, name: name, format: format, key: .sticker(stickerId: sticker, format: format))
    }

    func popout(
        _ target: ExpressionTarget, store: Store = Store(), lookups: Lookups = Lookups(),
        in channelGuild: GuildId? = here
    ) async -> ExpressionPopout {
        let model = ExpressionPopoutModel(
            target: target, channelGuild: channelGuild, store: store, lookups: lookups,
            locale: Locale(identifier: "en_US"))
        await model.load()
        return model.content
    }

    var wave: ExpressionTarget { .customEmoji(id: Self.wave, name: "wave", animated: true) }

    func text(_ runs: [ExpressionPopout.Run]) -> String {
        runs.map(\.text).joined()
    }

    // The hover card: the emoji large, its name with colons, "Click to learn more"; a sticker's:
    // a sticker symbol and its plain name.
    @Test
    func theHoverCardsSay() {
        #expect(
            wave.card
                == ExpressionCard(
                    picture: .image(.emoji(emojiId: Self.wave, animated: true)), title: ":wave:",
                    prompt: "Click to learn more"))
        #expect(
            ExpressionTarget.unicodeEmoji("😀", name: "grinning").card
                == ExpressionCard(picture: .glyph("😀"), title: ":grinning:", prompt: "Click to learn more"))
        #expect(
            ExpressionTarget.sticker(Self.sticker(1, "when the imposter is sus")).card
                == ExpressionCard(
                    picture: .stickerSymbol, title: "when the imposter is sus",
                    prompt: "Click to learn more"))
    }

    // Before the answer the emoji and its name show, and the rest loads.
    @Test
    func theTopShowsBeforeTheAnswer() {
        let model = ExpressionPopoutModel(
            target: wave, channelGuild: Self.here, store: Store(), lookups: Lookups())

        #expect(model.content.picture == .emoji(.emoji(emojiId: Self.wave, animated: true)))
        #expect(model.content.title == ":wave:")
        #expect(model.content.loading)
    }

    // The emoji is from a server the user is in: this one, or one of theirs; Nitro or not. The
    // section names the server; a click opens it.
    @Test
    func anEmojiFromTheUsersServers() async {
        let lookups = Lookups()
        lookups.source = .success(.guild(guild: Self.hubGuild()))
        let local = Store(
            emojis: [Self.wave: Self.hub], guilds: [Self.hub: Guild(id: Self.hub, name: "Emoji Hub")])

        var nitro = local
        nitro.tier = .classic
        let thisNitro = await popout(wave, store: nitro, lookups: lookups, in: Self.hub)
        #expect(text(thisNitro.body) == "This emoji is from this server. You can use it everywhere.")
        let otherNitro = await popout(wave, store: nitro, lookups: lookups)
        #expect(
            text(otherNitro.body)
                == "This emoji is from one of your servers. Type its name in the chat bar to use it.")
        let thisFree = await popout(wave, store: local, lookups: lookups, in: Self.hub)
        #expect(text(thisFree.body) == "This emoji is from this server. Get Nitro to use it everywhere.")
        let otherFree = await popout(wave, store: local, lookups: lookups)
        #expect(
            text(otherFree.body) == "This emoji is from one of your servers. Get Nitro to use it everywhere.")

        let server = try? #require(otherFree.server)
        #expect(server?.heading == "This emoji is from")
        #expect(server?.name == "Emoji Hub")
        #expect(server?.icon == Self.hubIcon)
        #expect(server?.badge == .verified)
        #expect(server?.line == .discoverable(1234))
        #expect(server?.lineText == "1,234 Online · Discoverable Server")
        #expect(server?.opens == true)
        #expect(otherFree.more == nil, "no Show More for the user's own server")
        #expect(!otherFree.loading)
    }

    // A server of the user's that's down: the store has no emoji for it, Discord names it; it's
    // still one of theirs.
    @Test
    func anEmojiFromTheUsersServerWhileItsDown() async {
        let lookups = Lookups()
        lookups.source = .success(.guild(guild: Self.hubGuild()))

        let content = await popout(wave, store: Store(down: [Self.hub]), lookups: lookups)

        #expect(text(content.body) == "This emoji is from one of your servers. Get Nitro to use it everywhere.")
        #expect(content.server?.heading == "This emoji is from")
        #expect(content.server?.line == .discoverable(1234))
        #expect(content.server?.opens == true)
        #expect(content.more == nil)
    }

    // A server the user is in that isn't discoverable: Discord answers nothing, the store knows
    // it; its line says invite-only.
    @Test
    func anEmojiFromTheUsersInviteOnlyServer() async {
        let local = Store(
            emojis: [Self.wave: Self.elsewhere],
            guilds: [Self.elsewhere: Guild(id: Self.elsewhere, name: "Friends", icon: Self.hubIcon)])

        let content = await popout(wave, store: local, lookups: Lookups())

        #expect(text(content.body) == "This emoji is from one of your servers. Get Nitro to use it everywhere.")
        #expect(content.server?.name == "Friends")
        #expect(content.server?.line == .inviteOnly)
        #expect(content.server?.lineText == "Invite-only server")
        #expect(content.server?.opens == true)
        #expect(content.server?.heading == "This emoji is from")
    }

    // A discoverable server the user isn't in: "Get this emoji from", its online count, Show More
    // Emoji with up to 12 others; no click (Akari has no server preview).
    @Test
    func anEmojiFromADiscoverableServer() async {
        let lookups = Lookups()
        lookups.source = .success(.guild(guild: Self.hubGuild(emojis: 20)))

        let free = await popout(wave, lookups: lookups)
        #expect(text(free.body) == "Want to use this emoji everywhere? Get Nitro and join the server.")
        #expect(free.server?.heading == "Get this emoji from")
        #expect(free.server?.line == .online(1234))
        #expect(free.server?.lineText == "1,234 Online")
        #expect(free.server?.opens == false)
        guard case .emoji(let others) = free.more else {
            Issue.record("Show More Emoji")
            return
        }
        #expect(others.count == 12)
        #expect(!others.contains { $0.id == Self.wave })
        #expect(free.moreLabel == "Show More Emoji")

        let nitro = await popout(wave, store: Store(tier: .nitro), lookups: lookups)
        #expect(text(nitro.body) == "Want to use this emoji everywhere? Join the server.")
    }

    // Unknown or failed: the invite-only line, no section. An app's emoji names the app.
    @Test
    func anEmojiDiscordWontPlace() async {
        let failed = await popout(wave)
        #expect(
            text(failed.body) == "This emoji is from a server that is either invite-only or unavailable.")
        #expect(failed.server == nil)
        #expect(!failed.loading)

        let lookups = Lookups()
        lookups.source = .success(.application(name: "Emoji Bot"))
        let app = await popout(wave, lookups: lookups)
        #expect(text(app.body) == "This emoji is from the Emoji Bot app")
        #expect(app.server == nil)
    }

    // Discord's source call is given 5 s, as the official client does.
    @Test
    func aSlowSourceGivesUpAfterFiveSeconds() async {
        let lookups = Lookups()
        lookups.source = .success(.guild(guild: Self.hubGuild()))
        lookups.delay = .seconds(30)
        let model = ExpressionPopoutModel(
            target: wave, channelGuild: Self.here, store: Store(), lookups: lookups,
            sourceTimeout: .milliseconds(50))

        await model.load()

        #expect(model.content.server == nil)
        #expect(!model.content.loading)
    }

    // The real call can't be cancelled (UniFFI's futures don't cancel): the limit stops waiting
    // for it all the same.
    @Test
    func theLimitHoldsForACallThatIgnoresCancellation() async {
        let lookups = Lookups()
        lookups.source = .success(.guild(guild: Self.hubGuild()))
        lookups.delay = .seconds(2)
        lookups.deaf = true
        let model = ExpressionPopoutModel(
            target: wave, channelGuild: Self.here, store: Store(), lookups: lookups,
            sourceTimeout: .milliseconds(50))
        let started = ContinuousClock.now

        await model.load()

        #expect(ContinuousClock.now - started < .milliseconds(1000))
        #expect(!model.content.loading)
        #expect(model.content.server == nil)
    }

    @Test
    func aUnicodeEmojiIsADefaultEmoji() async {
        let grinning = await popout(.unicodeEmoji("😀", name: "grinning"))
        #expect(grinning.picture == .glyph("😀"))
        #expect(grinning.title == ":grinning:")
        #expect(text(grinning.body) == "A default emoji. You can use this emoji everywhere on Discord.")
        #expect(!grinning.loading)

        let pizza = await popout(.unicodeEmoji("🍕", name: "pizza"))
        #expect(pizza.title == ":pizza: (without pineapple)")
        let elsewhere = ExpressionPopoutModel(
            target: .unicodeEmoji("🍕", name: "pizza"), channelGuild: nil, store: Store(),
            lookups: Lookups(), locale: Locale(identifier: "de_DE"))
        #expect(elsewhere.content.title == ":pizza:")
    }

    // A server's sticker: the same states with sticker words; Nitro for stickers is Nitro or
    // Basic, not Classic.
    @Test
    func aStickerFromAServer() async {
        let wave = Self.sticker(1)
        let lookups = Lookups()
        lookups.details = .success(StickerDetails(sticker: wave, origin: .guild(guildId: Self.hub)))
        var hub = Self.hubGuild()
        hub.stickers = (1...5).map { Self.sticker(UInt64($0), "s\($0)") }
        lookups.stickerGuild = .success(hub)
        let joined = Store(guilds: [Self.hub: Guild(id: Self.hub, name: "Emoji Hub")])
        let down = await popout(.sticker(wave), store: Store(down: [Self.hub]), lookups: lookups)
        #expect(down.server?.heading == "This sticker is from", "a server that's down is still the user's")

        let here = await popout(.sticker(wave), store: joined, lookups: lookups, in: Self.hub)
        #expect(here.picture == .sticker(wave))
        #expect(here.title == "wave")
        #expect(text(here.body) == "This sticker is from this server. Get Nitro to use it everywhere.")
        #expect(here.server?.heading == "This sticker is from")
        var classic = joined
        classic.tier = .classic
        let classicHere = await popout(.sticker(wave), store: classic, lookups: lookups, in: Self.hub)
        #expect(text(classicHere.body) == "This sticker is from this server. Get Nitro to use it everywhere.")
        var basic = joined
        basic.tier = .basic
        let other = await popout(.sticker(wave), store: basic, lookups: lookups)
        #expect(
            text(other.body)
                == "This sticker is from one of your servers. Type its name in the chat bar to use it.")

        let stranger = await popout(.sticker(wave), lookups: lookups)
        #expect(text(stranger.body) == "Want to use this sticker everywhere? Get Nitro and join the server.")
        #expect(stranger.server?.heading == "Get this sticker from")
        #expect(stranger.moreLabel == "Show More Stickers")
        guard case .stickers(let others) = stranger.more else {
            Issue.record("Show More Stickers")
            return
        }
        #expect(others.map(\.name) == ["s2", "s3", "s4", "s5"])
        #expect(lookups.asked.current.contains("stickerGuild"))
    }

    // A pack's sticker: "This is a **pack** sticker." and up to three others from the pack.
    @Test
    func aStickerFromAPack() async {
        let wave = Self.sticker(1)
        let pack = id(720_000_000_000_000_001) as StickerPackId
        let lookups = Lookups()
        lookups.details = .success(StickerDetails(sticker: wave, origin: .pack(packId: pack)))
        lookups.pack = .success(
            StickerPack(id: pack, name: "Wumpus Beyond", stickers: (1...6).map { Self.sticker(UInt64($0)) }))

        let content = await popout(.sticker(wave), lookups: lookups)

        #expect(content.title == "wave")
        #expect(content.picture == nil)
        #expect(
            content.body
                == [
                    .init(text: "This is a ", bold: false), .init(text: "Wumpus Beyond", bold: true),
                    .init(text: " sticker.", bold: false),
                ])
        #expect(content.others.map(\.id.rawValue) == [2, 3, 4])
        #expect(content.server == nil)
    }

    @Test
    func aStickerDiscordWontPlace() async {
        let content = await popout(.sticker(Self.sticker(1)))

        #expect(content.picture == .sticker(Self.sticker(1)))
        #expect(content.title == "wave")
        #expect(
            text(content.body) == "This sticker is from a server that is either invite-only or unavailable.")
        #expect(!content.loading)
    }
}
