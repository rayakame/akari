import Foundation
import Testing

@testable import AkariKit

// Message 100 in channel 11; the user is 1.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReactorsTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(11)
    let it: MessageId = id(100)
    var refusals = 0

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.show(message(100, in: 11, by: user(2)))
            state.allow(reacting(), message: 100, in: 11)
        }
    }

    func react(_ reactions: Reaction...) {
        store.update { $0.reactions[id(100)] = reactions }
    }

    func people(_ ids: ClosedRange<UInt64>, emoji: String = "👍", kind: ReactionKind = .normal) {
        account.reactorLists.withLock {
            $0[FakeAccount.reactorsKey(.unicode(emoji), kind)] = ids.map { user($0, name: "user \($0)") }
        }
    }

    func window(on emoji: String? = nil) -> ReactorsModel {
        let model = ReactorsModel(
            channelId: here, messageId: it,
            emoji: emoji.map { ReactionPill.ID(.unicode($0)) }, userId: id(1), store: store,
            account: account)
        model.refused = { [unowned self] in refusals += 1 }
        return model
    }

    func changed() -> EventBatch {
        EventBatch([.messageReactionsChanged(channelId: here, messageId: it)])
    }

    var calls: [CallLog.Call] { store.log.actions }
    func ids(_ model: ReactorsModel) -> [UInt64] { model.reactors.map(\.id.rawValue) }

    @Test
    func theEmojiAreListedByCountAndTheFirstIsChosen() {
        react(reaction("👍", 1), reaction("🎉", 2, burst: 2), reaction("👀", 3))

        let model = window()

        #expect(model.tabs.map(\.emoji.name) == ["🎉", "👀", "👍"])
        #expect(model.tabs.map(\.count) == [4, 3, 1])
        #expect(model.selected == ReactionPill.ID(.unicode("🎉")))
        #expect(ReactionWords.tab(model.tabs[2]) == "thumbsup 1 reaction")
    }

    @Test
    func theWindowOpensOnTheEmojiItWasAskedFor() {
        react(reaction("👍", 1), reaction("🎉", 2))

        #expect(window(on: "👍").selected == ReactionPill.ID(.unicode("👍")))
        #expect(window(on: "🦊").selected == ReactionPill.ID(.unicode("🎉")))
    }

    @Test
    func openingLoadsTheFirstHundred() async {
        react(reaction("👍", 3))
        people(2...4)
        let model = window()

        await model.open()

        #expect(calls == [.reactors(here, it, "👍", .normal, after: nil, limit: 100)])
        #expect(ids(model) == [2, 3, 4])
        #expect(!model.hasMore)
        #expect(!model.loading)
    }

    @Test
    func openingAgainKeepsTheListAndAsksNothing() async {
        react(reaction("👍", 250))
        people(2...251)
        let model = window()
        await model.open()
        store.log.forget()

        await model.open()

        #expect(calls.isEmpty)
        #expect(ids(model) == Array(2...101))
    }

    @Test
    func morePagesAreAskedWhileDiscordHasMore() async {
        react(reaction("👍", 250))
        people(2...251)
        let model = window()

        await model.open()
        #expect(model.hasMore)
        await model.loadMore()
        await model.loadMore()
        await model.loadMore()

        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 100),
                .reactors(here, it, "👍", .normal, after: id(101), limit: 100),
                .reactors(here, it, "👍", .normal, after: id(201), limit: 100),
            ])
        #expect(ids(model) == Array(2...251))
        #expect(!model.hasMore)
    }

    @Test
    func superReactionsFollowThePlainOnes() async {
        react(reaction("👍", 2, burst: 2))
        people(2...3)
        people(3...5, kind: .burst)
        let model = window()

        await model.open()
        #expect(model.hasMore)
        await model.loadMore()

        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 100),
                .reactors(here, it, "👍", .burst, after: nil, limit: 100),
            ])
        #expect(ids(model) == [2, 3, 4, 5])
        #expect(model.reactors.map(\.kind) == [.normal, .normal, .burst, .burst])
        #expect(!model.hasMore)
    }

    @Test
    func anEmojiWithSuperReactionsOnlyAsksForThem() async {
        react(reaction("👍", 0, burst: 1))
        people(2...2, kind: .burst)
        let model = window()

        await model.open()

        #expect(calls == [.reactors(here, it, "👍", .burst, after: nil, limit: 100)])
        #expect(ids(model) == [2])
    }

    @Test
    func anotherEmojiLoadsItsOwnUsers() async {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        people(7...7, emoji: "🎉")
        let model = window()
        await model.open()

        await model.select(ReactionPill.ID(.unicode("🎉")))

        #expect(ids(model) == [7])
        #expect(model.selected == ReactionPill.ID(.unicode("🎉")))
    }

    @Test
    func anotherEmojiShowsNoneOfTheFirstOnesUsersWhileItsPageComes() async {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        people(7...7, emoji: "🎉")
        let model = window()
        await model.open()
        account.holdReactors.withLock { $0 = true }

        let chosen = Task { await model.select(ReactionPill.ID(.unicode("🎉"))) }
        await account.reactorReplies.pulled(1)
        #expect(model.reactors.isEmpty)
        #expect(model.loading)
        account.reactorReplies.send(())
        await chosen.value

        #expect(ids(model) == [7])
    }

    @Test
    func choosingTheChosenEmojiAgainAsksNothing() async {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        let model = window()
        await model.open()
        store.log.forget()

        await model.select(ReactionPill.ID(.unicode("👍")))

        #expect(calls.isEmpty)
        #expect(ids(model) == [2, 3])
    }

    @Test
    func anEmojiTheMessageDoesntHaveIsntChosen() async {
        react(reaction("👍", 2))
        people(2...3)
        let model = window()
        await model.open()
        store.log.forget()

        await model.select(ReactionPill.ID(.unicode("🦊")))

        #expect(model.selected == ReactionPill.ID(.unicode("👍")))
        #expect(calls.isEmpty)
        #expect(ids(model) == [2, 3])
    }

    @Test
    func aPageOfAnEmojiTheWindowLeftIsDropped() async {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        people(7...7, emoji: "🎉")
        account.holdReactors.withLock { $0 = true }
        let model = window()
        let first = Task { await model.open() }
        await account.reactorReplies.pulled(1)

        let second = Task { await model.select(ReactionPill.ID(.unicode("🎉"))) }
        await account.reactorReplies.pulled(2)
        account.reactorReplies.send(())
        account.reactorReplies.send(())
        await first.value
        await second.value

        #expect(ids(model) == [7])
        #expect(!model.loading)
    }

    @Test
    func aFailedPageSaysSoAndIsAskedAgain() async {
        react(reaction("👍", 2))
        people(2...3)
        account.reactorsError.withLock { $0 = .Network(kind: .other) }
        let model = window()

        await model.open()
        #expect(model.failed)
        #expect(model.hasMore)
        #expect(model.reactors.isEmpty)

        account.reactorsError.withLock { $0 = nil }
        await model.loadMore()
        #expect(!model.failed)
        #expect(ids(model) == [2, 3])
    }

    @Test
    func theUsersOwnRowAlwaysHasItsRemoveAndOthersOnlyForModerators() async {
        react(reaction("👍", 2, me: true))
        people(1...2)
        let model = window()
        await model.open()
        #expect(model.reactors.map(\.removable) == [true, false])

        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let moderator = window()
        await moderator.open()
        #expect(moderator.reactors.map(\.removable) == [true, true])
    }

    @Test
    func aRemovedRowGoesAtOnceAndNamesItsKind() async throws {
        react(reaction("👍", 1, burst: 1))
        people(2...2)
        people(3...3, kind: .burst)
        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let model = window()
        await model.open()
        await model.loadMore()
        account.holdReactions.withLock { $0 = true }
        store.log.forget()

        let removal = Task { await model.remove(model.reactors[1]) }
        await account.reactionReplies.pulled(1)
        #expect(ids(model) == [2])
        account.reactionReplies.send(())
        await removal.value

        #expect(ids(model) == [2])
        await model.loadMore()
        #expect(calls == [.removeReaction(here, it, "👍", .burst, id(3))], "nothing asked again")
    }

    @Test
    func aRefusedRemovalPutsTheRowBack() async {
        react(reaction("👍", 2))
        people(2...3)
        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let model = window()
        await model.open()
        account.reactionError.withLock { $0 = .Forbidden }

        await model.remove(model.reactors[0])

        #expect(ids(model) == [2, 3])
        #expect(refusals == 1)
    }

    @Test(arguments: [
        (ReactionError.Blocked, true), (.Forbidden, true), (.TooMany, false), (.UnknownEmoji, false),
        (.RateLimited(retryAfter: nil), false), (.Request(error: .Network(kind: .other)), false),
    ])
    func onlyARefusalWithoutWordsIsFeltWhenARemovalFails(error: ReactionError, felt: Bool) async {
        react(reaction("👍", 2))
        people(2...3)
        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let model = window()
        await model.open()
        account.reactionError.withLock { $0 = error }

        await model.remove(model.reactors[0])

        #expect(ids(model) == [2, 3])
        #expect(refusals == (felt ? 1 : 0))
    }

    @Test
    func aRowThatMayNotBeRemovedAsksNothing() async {
        react(reaction("👍", 1))
        people(2...2)
        let model = window()
        await model.open()
        store.log.forget()

        await model.remove(model.reactors[0])

        #expect(calls.isEmpty)
    }

    @Test
    func aRowsRemoveClickedAgainBeforeTheAnswerAsksNothing() async {
        react(reaction("👍", 2))
        people(2...3)
        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let model = window()
        await model.open()
        let row = model.reactors[0]
        account.holdReactions.withLock { $0 = true }
        store.log.forget()

        let removal = Task { await model.remove(row) }
        await account.reactionReplies.pulled(1)
        account.holdReactions.withLock { $0 = false }
        await model.remove(row)
        account.reactionReplies.send(())
        await removal.value

        #expect(calls == [.removeReaction(here, it, "👍", .normal, id(2))])
        #expect(ids(model) == [3])
    }

    @Test
    func aChangeAsksTheFirstPageAgainAndKeepsTheListMeanwhile() async throws {
        react(reaction("👍", 3))
        people(2...4)
        let model = window()
        await model.open()
        account.holdReactors.withLock { $0 = true }
        people(3...5)
        react(reaction("👍", 3, me: true))

        model.apply(changed())
        await account.reactorReplies.pulled(1)
        #expect(ids(model) == [2, 3, 4])
        account.reactorReplies.send(())

        try await eventually { self.ids(model) == [3, 4, 5] }
        #expect(model.tabs.map(\.mine) == [true])
    }

    // A new first page says nothing of who is past it.
    @Test
    func aRefreshAsksEveryLoadedPageAgain() async throws {
        react(reaction("👍", 250))
        people(2...251)
        let model = window()
        await model.open()
        await model.loadMore()
        #expect(model.reactors.count == 200)
        account.reactorLists.withLock {
            $0[FakeAccount.reactorsKey(.unicode("👍"), .normal)]?.removeAll { $0.id == id(150) }
        }
        react(reaction("👍", 249))
        store.log.forget()

        model.apply(changed())

        try await eventually { !self.ids(model).contains(150) }
        #expect(ids(model) == Array(2...149) + Array(151...202))
        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 100),
                .reactors(here, it, "👍", .normal, after: id(101), limit: 100),
            ])
        #expect(model.hasMore)
    }

    @Test
    func aRefreshThatFailsKeepsTheList() async throws {
        react(reaction("👍", 3))
        people(2...4)
        let model = window()
        await model.open()
        account.reactorsError.withLock { $0 = .Network(kind: .other) }
        react(reaction("👍", 2))
        store.log.forget()

        model.apply(changed())

        try await eventually { self.calls.count == 1 && !model.loading }
        #expect(ids(model) == [2, 3, 4])
        #expect(!model.failed)
    }

    // Reaction events name no user: one leaving and one coming leave the count as it was.
    @Test
    func aReactionEventThatLeavesEveryCountAsksAgain() async throws {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        let model = window(on: "👍")
        await model.open()
        people(3...4)

        model.apply(changed())

        try await eventually { self.ids(model) == [3, 4] }
    }

    @Test
    func aPageAskedForDuringARefreshIsLoadedAfterIt() async throws {
        react(reaction("👍", 150))
        people(2...151)
        let model = window()
        await model.open()
        account.holdReactors.withLock { $0 = true }
        react(reaction("👍", 151))
        people(2...152)

        model.apply(changed())
        await account.reactorReplies.pulled(1)
        await model.loadMore()
        #expect(model.reactors.count == 100)
        account.reactorReplies.send(())
        account.reactorReplies.send(())

        try await eventually { self.ids(model) == Array(2...152) }
        #expect(!model.hasMore)
        #expect(!model.loading)
    }

    // The first page's answer comes after the second asked: a page asked twice would show.
    @Test
    func morePagesAskedForWhileOneLoadsAskNothing() async throws {
        react(reaction("👍", 250))
        people(2...251)
        account.holdReactors.withLock { $0 = true }
        let model = window()
        let first = Task { await model.open() }
        await account.reactorReplies.pulled(1)

        let second = Task { await model.loadMore() }
        account.reactorReplies.send(())
        await first.value
        account.reactorReplies.send(())
        await second.value

        #expect(calls == [.reactors(here, it, "👍", .normal, after: nil, limit: 100)])
        #expect(ids(model) == Array(2...101))
    }

    @Test
    func aChangeDuringAPageIsAskedAfterIt() async throws {
        react(reaction("👍", 3))
        people(2...4)
        account.holdReactors.withLock { $0 = true }
        let model = window()
        let opening = Task { await model.open() }
        await account.reactorReplies.pulled(1)
        people(3...5)
        react(reaction("👍", 3, me: true))

        model.apply(changed())
        account.reactorReplies.send(())
        await account.reactorReplies.pulled(2)
        account.reactorReplies.send(())
        await opening.value

        #expect(ids(model) == [3, 4, 5])
    }

    @Test
    func aRefreshOfAnEmojiTheWindowLeftIsDropped() async throws {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        people(7...7, emoji: "🎉")
        let model = window(on: "👍")
        await model.open()
        account.holdReactors.withLock { $0 = true }
        people(2...4)
        react(reaction("👍", 3), reaction("🎉", 1))

        model.apply(changed())
        await account.reactorReplies.pulled(1)
        let chosen = Task { await model.select(ReactionPill.ID(.unicode("🎉"))) }
        await account.reactorReplies.pulled(2)
        account.reactorReplies.send(())
        account.reactorReplies.send(())
        await chosen.value

        #expect(ids(model) == [7])
    }

    @Test
    func aKindWhoseCountWentIsDroppedByTheRefresh() async throws {
        react(reaction("👍", 1, burst: 1))
        people(2...2)
        people(3...3, kind: .burst)
        let model = window()
        await model.open()
        await model.loadMore()
        #expect(ids(model) == [2, 3])
        react(reaction("👍", 1))

        model.apply(changed())

        try await eventually { self.ids(model) == [2] }
    }

    @Test
    func aMessageThatArrivesWholeAgainRereadsTheTabs() {
        react(reaction("👍", 1))
        let model = window()
        react(reaction("👍", 1), reaction("🎉", 2))

        model.apply(EventBatch([.messageUpdated(channelId: here, messageId: it)]))

        #expect(model.tabs.map(\.name) == ["🎉", "👍"])
    }

    @Test
    func aNewSessionRereadsTheTabs() {
        react(reaction("👍", 1))
        let model = window()
        react(reaction("👍", 1), reaction("🎉", 2))

        model.apply(EventBatch([.ready]))

        #expect(model.tabs.map(\.name) == ["🎉", "👍"])
    }

    @Test
    func aPageThatShowsNoOneNewGoesOnToTheNext() async {
        react(reaction("👍", 100, burst: 2))
        people(2...101)
        people(200...201, kind: .burst)
        let model = window()
        await model.open()
        #expect(model.hasMore)

        await model.loadMore()

        #expect(ids(model) == Array(2...101) + [200, 201])
        #expect(!model.hasMore)
        #expect(
            calls.suffix(2) == [
                .reactors(here, it, "👍", .normal, after: id(101), limit: 100),
                .reactors(here, it, "👍", .burst, after: nil, limit: 100),
            ])
    }

    @Test
    func aFullPageWithNoOneOnItEndsTheList() async {
        react(reaction("👍", 2))
        people(2...3)
        account.reactorsFull.withLock { $0 = true }
        let model = window()
        await model.open()
        #expect(model.hasMore)
        store.log.forget()

        await model.loadMore()

        #expect(calls == [.reactors(here, it, "👍", .normal, after: id(3), limit: 100)])
        #expect(ids(model) == [2, 3])
        #expect(!model.hasMore)
        #expect(!model.loading)
    }

    // A Discord that answered every `after` with the first page would be asked forever.
    @Test
    func aPageThatEndsOnTheUserItWasAskedAfterEndsTheList() async {
        react(reaction("👍", 150))
        people(2...101)
        let model = window()
        await model.open()
        #expect(model.hasMore)
        account.reactorsIgnoringAfter.withLock { $0 = 1 }
        store.log.forget()

        await model.loadMore()

        #expect(calls == [.reactors(here, it, "👍", .normal, after: id(101), limit: 100)])
        #expect(ids(model) == Array(2...101))
        #expect(!model.hasMore)
        #expect(!model.loading)
    }

    @Test
    func aRefreshStopsAtAPageThatEndsOnTheUserItWasAskedAfter() async throws {
        react(reaction("👍", 150))
        people(2...151)
        let model = window()
        await model.open()
        await model.loadMore()
        account.reactorsIgnoringAfter.withLock { $0 = 1 }
        react(reaction("👍", 151))
        store.log.forget()

        model.apply(changed())

        try await eventually { self.calls.count >= 2 && !model.loading }
        #expect(
            calls == [
                .reactors(here, it, "👍", .normal, after: nil, limit: 100),
                .reactors(here, it, "👍", .normal, after: id(101), limit: 100),
            ])
    }

    // The user's own ✕ changes the store at once: the list is asked again before Discord has
    // removed anything.
    @Test
    func aRefreshAskedBeforeARemovalWasAnsweredNeverBringsTheRowBack() async throws {
        react(reaction("👍", 3, me: true))
        people(1...3)
        let model = window()
        await model.open()
        account.holdReactions.withLock { $0 = true }
        account.holdReactors.withLock { $0 = true }
        store.log.forget()

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        react(reaction("👍", 2))
        model.apply(changed())
        await account.reactorReplies.pulled(1)
        account.reactionReplies.send(())
        await removal.value
        account.reactorReplies.send(())

        await account.reactorReplies.pulled(2)
        #expect(ids(model) == [2, 3], "the list from before the removal, without its row")
        people(2...3)
        account.reactorReplies.send(())
        try await eventually { !model.loading }
        #expect(ids(model) == [2, 3])

        react(reaction("👍", 3, me: true))
        people(1...3)
        account.holdReactors.withLock { $0 = false }
        model.apply(changed())
        try await eventually { self.ids(model) == [1, 2, 3] }
    }

    @Test
    func aRemovedRowStaysOutWhileNoListAskedAfterTheAnswerHasCome() async throws {
        react(reaction("👍", 3, me: true))
        people(1...3)
        let model = window()
        await model.open()
        account.holdReactions.withLock { $0 = true }
        account.holdReactors.withLock { $0 = true }

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        react(reaction("👍", 2))
        model.apply(changed())
        await account.reactorReplies.pulled(1)
        account.reactionReplies.send(())
        await removal.value
        account.reactorReplies.send(())
        await account.reactorReplies.pulled(2)
        account.reactorsError.withLock { $0 = .Network(kind: .other) }
        account.reactorReplies.send(())

        try await eventually { !model.loading }
        #expect(ids(model) == [2, 3])
    }

    // The user reacted again meanwhile: Discord's list has them.
    @Test
    func aRemovalAnsweredDuringARefreshHidesNothingOnceTheEmojiIsChosenAgain() async throws {
        react(reaction("👍", 3, me: true), reaction("🎉", 1))
        people(1...3)
        people(7...7, emoji: "🎉")
        let model = window(on: "👍")
        await model.open()
        account.holdReactions.withLock { $0 = true }
        account.holdReactors.withLock { $0 = true }

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        model.apply(changed())
        await account.reactorReplies.pulled(1)
        account.reactionReplies.send(())
        await removal.value
        account.holdReactors.withLock { $0 = false }
        await model.select(ReactionPill.ID(.unicode("🎉")))
        account.reactorReplies.send(())
        await model.select(ReactionPill.ID(.unicode("👍")))

        #expect(ids(model) == [1, 2, 3])
    }

    @Test
    func aRemovalHidesItsRowUnderItsEmojiOnly() async throws {
        react(reaction("👍", 2, me: true), reaction("🎉", 2, me: true))
        people(1...2)
        people(1...1, emoji: "🎉")
        account.reactorLists.withLock {
            $0[FakeAccount.reactorsKey(.unicode("🎉"), .normal)]?.append(user(3, name: "user 3"))
        }
        let model = window(on: "👍")
        await model.open()
        account.holdReactions.withLock { $0 = true }

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        await model.select(ReactionPill.ID(.unicode("🎉")))
        #expect(ids(model) == [1, 3])
        account.reactionReplies.send(())
        await removal.value

        #expect(ids(model) == [1, 3], "another emoji's list")
        await model.select(ReactionPill.ID(.unicode("👍")))
        #expect(ids(model) == [1, 2], "asked after the answer: as Discord has it")
    }

    @Test
    func aRowWhoseRemovalIsOnItsWayStaysOutWhenItsEmojiIsChosenAgain() async {
        react(reaction("👍", 2, me: true), reaction("🎉", 1))
        people(1...2)
        people(7...7, emoji: "🎉")
        let model = window(on: "👍")
        await model.open()
        account.holdReactions.withLock { $0 = true }

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        await model.select(ReactionPill.ID(.unicode("🎉")))
        await model.select(ReactionPill.ID(.unicode("👍")))
        #expect(ids(model) == [2])
        account.reactionReplies.send(())
        await removal.value

        #expect(ids(model) == [2])
    }

    @Test
    func aRemovalTakesOneKindAndLeavesTheUsersOther() async throws {
        react(reaction("👍", 2, burst: 1))
        people(2...3)
        people(2...2, kind: .burst)
        store.update { $0.allow(reacting(remove: true), message: 100, in: 11) }
        let model = window()
        await model.open()
        await model.loadMore()
        #expect(ids(model) == [2, 3])
        account.holdReactions.withLock { $0 = true }

        let removal = Task { await model.remove(model.reactors[0]) }
        await account.reactionReplies.pulled(1)
        #expect(ids(model) == [3, 2], "the super reaction is theirs still")
        #expect(model.reactors.map(\.kind) == [.normal, .burst])
        account.reactionReplies.send(())
        await removal.value

        #expect(ids(model) == [3, 2])
        #expect(model.reactors.map(\.kind) == [.normal, .burst])
    }

    @Test
    func theSettingTurnedOffEmptiesTheWindow() {
        react(reaction("👍", 1))
        let model = window()
        store.update { $0.displaySettings.renderReactions = false }

        model.apply(EventBatch([.displaySettingsChanged]))

        #expect(model.isEmpty)
    }

    // A reload after a reconnect may let go of the message for a moment.
    @Test
    func aMessageTheStoreLetGoOfKeepsItsWindow() async throws {
        react(reaction("👍", 2), reaction("🎉", 1))
        people(2...3)
        let model = window()
        await model.open()
        let held = store.messages(channelId: here, ids: [it])
        store.update { state in
            state.messages[id(100)] = nil
            state.reactions[id(100)] = nil
        }
        store.log.forget()

        model.apply(EventBatch([.ready]))

        #expect(model.tabs.map(\.emoji.name) == ["👍", "🎉"])
        #expect(ids(model) == [2, 3])
        #expect(calls.isEmpty)

        store.update { state in
            state.messages[id(100)] = held.first
            state.reactions[id(100)] = [reaction("👍", 3)]
        }
        people(2...4)
        model.apply(EventBatch([.messageInserted(channelId: here, messageId: it)]))

        #expect(model.tabs.map(\.count) == [3])
        try await eventually { self.ids(model) == [2, 3, 4] }
    }

    @Test
    func aDeletedMessageEmptiesTheWindow() {
        react(reaction("👍", 2))
        let model = window()
        store.update { state in
            state.messages[id(100)] = nil
            state.reactions[id(100)] = nil
        }

        model.apply(EventBatch([.messageDeleted(channelId: here, messageId: it)]))

        #expect(model.isEmpty)
    }

    @Test
    func anUnchangedEmojiAsksNothingWhenAnotherChanges() async {
        react(reaction("👍", 1), reaction("🎉", 5))
        people(2...2)
        let model = window(on: "👍")
        await model.open()
        store.log.forget()
        react(reaction("👍", 1), reaction("🎉", 2))

        model.apply(changed())
        #expect(model.tabs.map(\.count) == [2, 1])
        // A refresh the change had begun would be asked before this returns.
        people(7...8, emoji: "🎉")
        await model.select(ReactionPill.ID(.unicode("🎉")))

        #expect(calls == [.reactors(here, it, "🎉", .normal, after: nil, limit: 100)])
    }

    @Test
    func whenTheChosenEmojiLeavesTheFirstOneIsChosen() async throws {
        react(reaction("👍", 1), reaction("🎉", 5))
        people(2...2)
        people(7...8, emoji: "🎉")
        let model = window(on: "👍")
        await model.open()
        react(reaction("🎉", 5))

        model.apply(changed())

        #expect(model.selected == ReactionPill.ID(.unicode("🎉")))
        #expect(model.hasMore, "its users are still to come")
        try await eventually { self.ids(model) == [7, 8] }
    }

    @Test
    func theWindowIsEmptyOnceTheMessageHasNoReactions() {
        react(reaction("👍", 1))
        let model = window()
        #expect(!model.isEmpty)
        store.update { $0.reactions[id(100)] = [] }

        model.apply(changed())

        #expect(model.isEmpty)
        #expect(model.reactors.isEmpty)
    }

    @Test
    func eventsForOtherMessagesReadNothing() {
        react(reaction("👍", 1))
        let model = window()
        store.log.forget()

        model.apply(EventBatch([.messageReactionsChanged(channelId: here, messageId: id(7))]))

        #expect(store.reads.isEmpty)
        _ = model
    }
}

@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionReactionsTests {
    let store = FakeStore()
    let account: FakeAccount
    let session: SessionModel

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.show(message(100, in: 11, by: user(2)))
            state.windows[id(11)] = window([100])
            state.allow(reacting(remove: true), message: 100, in: 11)
            state.reactions[id(100)] = [reaction("👍", 2)]
        }
        session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
    }

    func row() throws -> MessageListModel.Row {
        try #require(session.messages?.rows.first)
    }

    @Test
    func viewReactionsOpensTheWindowAndClosingEndsIt() throws {
        let list = try #require(session.messages)

        session.messageActions(for: list) { _ in }.perform(.viewReactions, on: try row())
        #expect(session.reactors?.messageId == id(100))

        session.closeReactors()
        #expect(session.reactors == nil)
        session.close()
    }

    @Test
    func theWindowClosesItselfWhenTheLastReactionGoes() throws {
        session.viewReactions(of: try row())
        store.update { $0.reactions[id(100)] = [] }

        session.apply(EventBatch([.messageReactionsChanged(channelId: id(11), messageId: id(100))]))

        #expect(session.reactors == nil)
        session.close()
    }

    @Test
    func theWindowStaysOverAReconnect() throws {
        session.viewReactions(of: try row())
        store.update { state in
            state.messages[id(100)] = nil
            state.reactions[id(100)] = nil
        }

        session.apply(EventBatch([.ready]))
        #expect(session.reactors?.tabs.map(\.count) == [2])

        session.apply(EventBatch([.messageDeleted(channelId: id(11), messageId: id(100))]))
        #expect(session.reactors == nil)
        session.close()
    }

    // The store answers for a starter row under the row's own IDs.
    @Test
    func aStarterRowsWindowIsOpenedWithTheRowsOwnIDs() {
        store.update { $0.reactions[id(31)] = [reaction("👀", 1)] }
        let starter = MessageListModel.Row(
            id: id(31), message: message(31, in: 31, kind: .threadStarterMessage),
            starter: .loaded(message(100, in: 11)))

        session.viewReactions(of: starter)

        #expect(session.reactors?.channelId == id(31))
        #expect(session.reactors?.messageId == id(31))
        #expect(session.reactors?.tabs.map(\.name) == ["👀"])
        session.close()
    }

    @Test
    func aMessageWithoutReactionsOpensNoWindow() {
        session.viewReactions(channelId: id(11), messageId: id(101))

        #expect(session.reactors == nil)
        session.close()
    }

    @Test
    func theQuestionIsAnsweredOnceAndYesRemoves() async throws {
        let list = try #require(session.messages)
        let model = session.messageActions(for: list) { _ in }

        let pill = try #require(try row().reactions?.pills.first)

        model.perform(.removeAllReactions, on: try row())
        #expect(session.messageConfirmation?.title == "Remove All Reactions")
        session.answerMessageConfirmation(false)
        #expect(session.messageConfirmation == nil)

        // A removal "No" had begun would be asked before this one.
        model.removeReactions(of: pill, on: try row(), shift: false)
        #expect(session.messageConfirmation?.title == "Remove Reactions for Emoji")
        session.answerMessageConfirmation(true)
        try await eventually { self.removals.contains(.removeReactions(id(11), id(100), "👍")) }
        #expect(removals == [.removeReactions(id(11), id(100), "👍")])
        #expect(session.messageConfirmation == nil)
        session.close()
    }

    var removals: [CallLog.Call] {
        store.log.actions.filter { if case .removeReactions = $0 { true } else { false } }
    }

    @Test
    func aRefusedReactionAlertsOrIsCounted() async throws {
        let list = try #require(session.messages)
        account.reactionError.withLock { $0 = .TooMany }
        await list.reactions.toggle(.unicode("💯"), on: try row(), from: .hoverBar)
        #expect(session.messageActionFailure?.title == "We appreciate the enthusiasm, but...")

        account.reactionError.withLock { $0 = .Blocked }
        await list.reactions.toggle(.unicode("💯"), on: try row(), from: .hoverBar)
        #expect(session.refusedReactions == 1)
        session.close()
    }
}
