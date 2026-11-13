import Foundation
import Testing

@testable import AkariKit

func reaction(
    _ emoji: String, _ normal: UInt32 = 1, burst: UInt32 = 0, me: Bool = false, meBurst: Bool = false
) -> Reaction {
    Reaction(emoji: .unicode(emoji), normal: normal, burst: burst, me: me, meBurst: meBurst)
}

func custom(_ raw: UInt64, _ name: String? = "wave", animated: Bool = false) -> ReactionEmoji {
    ReactionEmoji(
        id: id(raw), name: name, animated: animated,
        key: .emoji(emojiId: id(raw), animated: animated))
}

func reacting(react: Bool = true, toggle: Bool = true, remove: Bool = false) -> MessageActions {
    var actions = allowed(react: react)
    actions.toggleReaction = toggle
    actions.removeReactions = remove
    return actions
}

struct ReactionPillTests {
    @Test
    func plainAndSuperReactionsAreOnePill() {
        let pills = ReactionPill.merged([
            reaction("👍", 2, burst: 3), reaction("🎉", 0, burst: 1, meBurst: true),
            reaction("😀", 4, me: true), reaction("👀", 0),
        ])

        #expect(pills.map(\.name) == ["👍", "🎉", "😀"])
        #expect(pills.map(\.count) == [5, 1, 4])
        #expect(pills.map(\.mine) == [false, true, true])
    }

    @Test
    func aCustomEmojiIsToldApartByItsIDAndNamedInColons() {
        let renamed = ReactionPill(emoji: custom(7, "hello"), normal: 1, mine: false)
        let pill = ReactionPill(emoji: custom(7), normal: 1, mine: false)

        #expect(pill.id == renamed.id)
        #expect(pill.id != ReactionPill(emoji: custom(8), normal: 1, mine: false).id)
        #expect(pill.name == ":wave:")
        #expect(ReactionPill(emoji: custom(7, nil), normal: 1, mine: false).name == ":emoji:")
    }

    @Test
    func unicodeEmojiAreToldApartExactly() {
        let heart = ReactionPill(emoji: .unicode("❤️"), normal: 1, mine: false)
        let bare = ReactionPill(emoji: .unicode("❤"), normal: 1, mine: false)

        #expect(heart.id != bare.id)
    }

    @Test
    func voiceOverReadsTheOfficialWords() {
        #expect(
            ReactionPill(emoji: custom(7, "party_parrot"), normal: 1, mine: false).accessibilityLabel
                == "party parrot, 1 reaction, press to react")
        #expect(
            ReactionPill(emoji: .unicode("👍"), normal: 2, burst: 1, mine: true).accessibilityLabel
                == "👍, 3 reactions, press to remove your reaction")
    }

    @Test
    func theMostReactionsComeFirstAndTiesKeepTheirOrder() {
        let pills = ReactionPill.merged([
            reaction("a", 1), reaction("b", 3), reaction("c", 1, burst: 2), reaction("d", 2),
        ])

        #expect(ReactionPill.byCount(pills).map(\.name) == ["b", "c", "d", "a"])
    }

    @Test(arguments: [
        ([], 0, nil),
        ([], 1, "👍 reacted by 1 person"),
        ([], 4, "👍 reacted by 4 people"),
        (["Ann"], 1, "👍 reacted by Ann"),
        (["Ann"], 2, "👍 reacted by Ann and 1 other"),
        (["Ann", "Bo"], 2, "👍 reacted by Ann and Bo"),
        (["Ann", "Bo"], 5, "👍 reacted by Ann, Bo, and 3 others"),
        (["Ann", "Bo", "Cy"], 3, "👍 reacted by Ann, Bo, and Cy"),
        (["Ann", "Bo", "Cy"], 4, "👍 reacted by Ann, Bo, Cy, and 1 other"),
        (["Ann", "Bo", "Cy", "Di"], 9, "👍 reacted by Ann, Bo, Cy, and 6 others"),
    ] as [([String], UInt32, String?)])
    func theTooltipNamesUpToThreeAndCountsTheRest(names: [String], count: UInt32, text: String?) {
        #expect(ReactionWords.tooltip(emoji: "👍", names: names, count: count) == text)
    }

    @Test
    func theBarShowsAsManyPillsAsTheOfficialClient() {
        #expect(PostBar.pillsShown(width: 200) == 0)
        #expect(PostBar.pillsShown(width: 337) == 0)
        #expect(PostBar.pillsShown(width: 338) == 1)
        #expect(PostBar.pillsShown(width: 700) == 7)
    }
}

// Channel 11 in guild 1 with message 100.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReactionsModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(11)
    let it: MessageId = id(100)
    var failures: [MessageActionsModel.Failure] = []
    var refusals = 0
    var tooltips: [String] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.allow(reacting(), message: 100, in: 11)
        }
    }

    func model(delay: Duration = .zero) -> ReactionsModel {
        let model = ReactionsModel(store: store, account: account, tooltipDelay: delay)
        model.failed = { [unowned self] in failures.append($0) }
        model.refused = { [unowned self] in refusals += 1 }
        model.tooltipChanged = { [unowned self] message, pill in
            tooltips.append("\(message.rawValue) \(pill.unicode ?? "custom")")
        }
        return model
    }

    func row(_ raw: UInt64 = 100) -> MessageListModel.Row {
        MessageListModel.Row(id: id(raw), message: message(raw, in: 11))
    }

    func react(_ reactions: Reaction..., on raw: UInt64 = 100) {
        store.update { $0.reactions[id(raw)] = reactions }
    }

    func pill(_ emoji: String) throws -> ReactionPill {
        try #require(
            ReactionPill.merged(store.reactions(channelId: here, messageId: it)).first {
                $0.name == emoji
            })
    }

    var calls: [CallLog.Call] { store.log.actions }

    @Test
    func aRowHasItsPillsAndWhatTheCoreAllows() {
        react(reaction("👍", 2), reaction("🎉", 1, me: true))
        store.update { $0.allow(reacting(react: false), message: 100, in: 11) }

        let row = model().row(channelId: here, messageId: it)

        #expect(row?.pills.map(\.name) == ["👍", "🎉"])
        #expect(row?.adds == false)
        #expect(row?.toggles == true)
    }

    @Test
    func aMessageWithoutReactionsHasNoRow() {
        #expect(model().row(channelId: here, messageId: it) == nil)
    }

    @Test
    func aClickOnAPillThatIsntMineReactsFromTheInlineButton() async throws {
        react(reaction("👍", 2))

        await model().toggle(try pill("👍"), on: row())

        #expect(calls == [.react(here, it, "👍", .inlineButton)])
        #expect(try pill("👍").mine)
    }

    @Test
    func aClickOnMyPillTakesTheReactionBack() async throws {
        react(reaction("👍", 2, me: true))

        await model().toggle(try pill("👍"), on: row())

        #expect(calls == [.unreact(here, it, "👍", .inlineButton)])
        #expect(try !pill("👍").mine)
    }

    @Test
    func aPillThatIsMineOnlyAsASuperReactionIsTakenBack() async throws {
        react(reaction("👍", 1, burst: 1, meBurst: true))

        await model().toggle(try pill("👍"), on: row())

        #expect(calls == [.unreact(here, it, "👍", .inlineButton)])
    }

    @Test
    func aCustomEmojiGoesWithItsNameAndID() async throws {
        store.update { $0.reactions[id(100)] = [Reaction(emoji: custom(7), normal: 1, burst: 0, me: false, meBurst: false)] }
        let pill = try #require(model().row(channelId: here, messageId: it)?.pills.first)

        await model().toggle(pill, on: row(), from: .forumToolbar)

        #expect(calls == [.react(here, it, "wave:7", .forumToolbar)])
    }

    @Test
    func pillsThatOnlyShowTakeNoClick() async throws {
        react(reaction("👍", 2))
        store.update { $0.allow(reacting(toggle: false), message: 100, in: 11) }

        await model().toggle(try pill("👍"), on: row())

        #expect(calls.isEmpty)
    }

    @Test
    func aNewEmojiNeedsThePermissionToReact() async {
        store.update { $0.allow(reacting(react: false), message: 100, in: 11) }
        let model = model()

        await model.toggle(.unicode("💯"), on: row(), from: .hoverBar)
        #expect(calls.isEmpty)

        store.update { $0.allow(reacting(), message: 100, in: 11) }
        await model.toggle(.unicode("💯"), on: row(), from: .hoverBar)
        #expect(calls == [.react(here, it, "💯", .hoverBar)])
    }

    @Test
    func aQuickReactionThatIsMineIsTakenBack() async {
        react(reaction("💯", 1, me: true))

        await model().toggle(.unicode("💯"), on: row(), from: .contextMenu)

        #expect(calls == [.unreact(here, it, "💯", .contextMenu)])
    }

    @Test
    func theQuickReactionsAreTheDefaultsAndSayWhichAreMine() {
        react(reaction("😆", 3, me: true))

        let quick = model().quick(for: row())

        #expect(quick.map(\.name) == ["💯", "😆", "💖"])
        #expect(quick.map(\.mine) == [false, true, false])
        #expect(quick.map(\.hint) == ["Click to react", "Click to remove", "Click to react"])
        #expect(quick[1].accessibilityLabel == "Click to remove 😆")
        #expect(quick[0].accessibilityLabel == "Click to react with 💯")
    }

    @Test
    func noQuickReactionsWhereTheUserMayNotReactOrTheMessageIsntSent() {
        let model = model()
        store.update { $0.allow(reacting(react: false), message: 100, in: 11) }
        #expect(model.quick(for: row()).isEmpty)

        store.update { $0.allow(reacting(), message: 101, in: 11) }
        let failed = MessageListModel.Row(
            id: id(101), message: message(101, in: 11, delivery: .failed))
        #expect(model.quick(for: failed).isEmpty)
    }

    @Test
    func tooManyReactionsShowTheOfficialAlert() async {
        account.reactionError.withLock { $0 = .TooMany }

        await model().toggle(.unicode("💯"), on: row(), from: .hoverBar)

        #expect(
            failures == [
                .init(
                    title: "We appreciate the enthusiasm, but...",
                    message:
                        "Your reaction was not added because there are too many reactions on this message."
                )
            ])
        #expect(refusals == 0)
        #expect(store.reactions(channelId: here, messageId: it).isEmpty)
    }

    @Test(arguments: [ReactionError.Blocked, .Forbidden])
    func aRefusalWithoutWordsIsFelt(error: ReactionError) async {
        account.reactionError.withLock { $0 = error }

        await model().toggle(.unicode("💯"), on: row(), from: .hoverBar)

        #expect(refusals == 1)
        #expect(failures.isEmpty)
    }

    @Test(arguments: [
        ReactionError.UnknownEmoji, .RateLimited(retryAfter: nil),
        .Request(error: .Network(kind: .other)), .Request(error: .InvalidRequest),
    ])
    func otherFailuresAreQuietAsInTheOfficialClient(error: ReactionError) async {
        account.reactionError.withLock { $0 = error }

        await model().toggle(.unicode("💯"), on: row(), from: .hoverBar)

        #expect(refusals == 0)
        #expect(failures.isEmpty)
    }

    @Test
    func withTheSettingOffNothingShowsAndNothingReacts() async throws {
        react(reaction("👍", 2))
        let shown = try pill("👍")
        store.update { $0.displaySettings.renderReactions = false }
        let model = model()

        #expect(!model.shows)
        #expect(model.row(channelId: here, messageId: it) == nil)
        #expect(model.quick(for: row()).isEmpty)
        await model.toggle(shown, on: row())
        #expect(calls.isEmpty)
    }

    @Test
    func theSettingIsReadAgainWhenItChanges() {
        react(reaction("👍", 2))
        let model = model()
        store.update { $0.displaySettings.renderReactions = false }

        model.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: it)]))
        #expect(model.shows)

        model.apply(EventBatch([.displaySettingsChanged]))
        #expect(!model.shows)
    }

    // MARK: The tooltip

    func people(_ names: String..., emoji: String = "👍", kind: ReactionKind = .normal) {
        let users = names.enumerated().map { user(UInt64($0 + 2), name: $1) }
        account.reactorLists.withLock {
            $0[FakeAccount.reactorsKey(.unicode(emoji), kind)] = users
        }
    }

    @Test
    func theTooltipCountsPeopleUntilTheNamesArrive() async throws {
        react(reaction("👍", 5))
        people("Ann", "Bo", "Cy", "Di", "Ed")
        let model = model()
        let shown = try pill("👍")
        #expect(model.tooltip(of: shown, on: row()) == "👍 reacted by 5 people")

        model.pointerEntered(shown, on: row())
        try await eventually { self.tooltips == ["100 👍"] }

        #expect(model.tooltip(of: shown, on: row()) == "👍 reacted by Ann, Bo, Cy, and 2 others")
        #expect(calls == [.reactors(here, it, "👍", .normal, after: nil, limit: 3)])
    }

    @Test
    func theNamesAreAskedOncePerMessageAndEmoji() async throws {
        react(reaction("👍", 2), reaction("🎉", 1))
        people("Ann", "Bo")
        people("Cy", emoji: "🎉")
        let model = model()

        model.pointerEntered(try pill("👍"), on: row())
        try await eventually { self.tooltips.count == 1 }
        model.pointerLeft()
        model.pointerEntered(try pill("👍"), on: row())
        model.pointerEntered(try pill("🎉"), on: row())
        try await eventually { self.tooltips.count == 2 }
        model.pointerEntered(try pill("👍"), on: row())
        await Task.yield()

        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 3),
                .reactors(here, it, "🎉", .normal, after: nil, limit: 3),
            ])
    }

    @Test
    func superReactionsFillTheNamesUp() async throws {
        react(reaction("👍", 1, burst: 2))
        people("Ann")
        people("Bo", "Cy", kind: .burst)
        let model = model()

        model.pointerEntered(try pill("👍"), on: row())
        try await eventually { !self.tooltips.isEmpty }

        #expect(model.tooltip(of: try pill("👍"), on: row()) == "👍 reacted by Ann, Bo, and Cy")
        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 3),
                .reactors(here, it, "👍", .burst, after: nil, limit: 3),
            ])
    }

    @Test
    func aChangeOfTheMessagesReactionsDropsItsNamesOnly() async throws {
        react(reaction("👍", 1))
        react(reaction("👍", 1), on: 101)
        people("Ann")
        let model = model()
        let other = MessageListModel.Row(id: id(101), message: message(101, in: 11))
        model.pointerEntered(try pill("👍"), on: row())
        try await eventually { self.tooltips.count == 1 }
        model.pointerEntered(try pill("👍"), on: other)
        try await eventually { self.tooltips.count == 2 }

        model.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: it)]))

        #expect(model.tooltip(of: try pill("👍"), on: row()) == "👍 reacted by 1 person")
        #expect(model.tooltip(of: try pill("👍"), on: other) == "👍 reacted by Ann")

        model.pointerEntered(try pill("👍"), on: row())
        try await eventually { self.tooltips.count == 3 }
        #expect(model.tooltip(of: try pill("👍"), on: row()) == "👍 reacted by Ann")
    }

    @Test
    func aNewSessionDropsEveryName() async throws {
        react(reaction("👍", 1))
        people("Ann")
        let model = model()
        model.pointerEntered(try pill("👍"), on: row())
        try await eventually { self.tooltips.count == 1 }

        model.apply(EventBatch([.ready]))

        #expect(model.tooltip(of: try pill("👍"), on: row()) == "👍 reacted by 1 person")
    }

    @Test
    func aPointerThatLeavesBeforeTheDelayAsksNothing() async throws {
        react(reaction("👍", 1))
        people("Ann")
        let model = model(delay: .seconds(30))

        model.pointerEntered(try pill("👍"), on: row())
        model.pointerLeft()
        try await Task.sleep(for: .milliseconds(20))

        #expect(calls.isEmpty)
    }

    @Test
    func namesThatArriveAfterTheReactionsChangedArentKept() async throws {
        react(reaction("👍", 1))
        people("Ann")
        account.holdReactors.withLock { $0 = true }
        let model = model()
        model.pointerEntered(try pill("👍"), on: row())
        await account.reactorReplies.pulled(1)

        model.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: it)]))
        account.reactorReplies.send(())
        try await Task.sleep(for: .milliseconds(20))

        #expect(tooltips.isEmpty)
        #expect(model.tooltip(of: try pill("👍"), on: row()) == "👍 reacted by 1 person")
    }

    @Test
    func aStarterRowsNamesAreKeptUnderTheMessageItShows() async throws {
        react(reaction("👍", 1), on: 7)
        store.update { $0.reactions[id(31)] = [reaction("👍", 1)] }
        people("Ann")
        let model = model()
        let starter = MessageListModel.Row(
            id: id(31), message: message(31, in: 31, kind: .threadStarterMessage),
            starter: .loaded(message(7, in: 11)))
        let shown = ReactionPill(reaction("👍", 1))

        model.pointerEntered(shown, on: starter)
        try await eventually { self.tooltips == ["7 👍"] }
        #expect(calls == [.reactors(id(31), id(31), "👍", .normal, after: nil, limit: 3)])

        model.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(7))]))
        #expect(model.tooltip(of: shown, on: starter) == "👍 reacted by 1 person")
    }
}
