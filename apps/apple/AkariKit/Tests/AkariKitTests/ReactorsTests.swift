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

        #expect(model.tabs.map(\.name) == ["🎉", "👀", "👍"])
        #expect(model.tabs.map(\.count) == [4, 3, 1])
        #expect(model.selected == ReactionPill.ID(.unicode("🎉")))
        #expect(ReactionWords.tab(model.tabs[2]) == "👍 1 reaction")
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

        #expect(calls == [.removeReaction(here, it, "👍", .burst, id(3))])
        #expect(ids(model) == [2])
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

    @Test
    func aRefreshKeepsWhatWasLoadedPastTheFirstPage() async throws {
        react(reaction("👍", 150))
        people(2...151)
        let model = window()
        await model.open()
        await model.loadMore()
        #expect(model.reactors.count == 150)
        people(3...152)
        react(reaction("👍", 149))
        store.log.forget()

        model.apply(changed())

        try await eventually { self.ids(model).first == 3 }
        #expect(ids(model) == Array(3...151))
        #expect(calls == [.reactors(here, it, "👍", .normal, after: nil, limit: 100)])
        #expect(!model.hasMore)
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
        await Task.yield()

        #expect(calls.isEmpty)
        #expect(model.tabs.map(\.count) == [2, 1])
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
    func aMessageWithoutReactionsOpensNoWindow() {
        session.viewReactions(channelId: id(11), messageId: id(101))

        #expect(session.reactors == nil)
        session.close()
    }

    @Test
    func theQuestionIsAnsweredOnceAndYesRemoves() async throws {
        let list = try #require(session.messages)
        let model = session.messageActions(for: list) { _ in }

        model.perform(.removeAllReactions, on: try row())
        #expect(session.messageConfirmation?.title == "Remove All Reactions")
        session.answerMessageConfirmation(false)
        #expect(session.messageConfirmation == nil)
        #expect(!store.log.actions.contains(.removeReactions(id(11), id(100), nil)))

        model.perform(.removeAllReactions, on: try row())
        session.answerMessageConfirmation(true)
        try await eventually {
            self.store.log.actions.contains(.removeReactions(id(11), id(100), nil))
        }
        #expect(session.messageConfirmation == nil)
        session.close()
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
