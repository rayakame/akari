import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1; message 100 with two reactions.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReactionActionsTests {
    let store = FakeStore()
    let account: FakeAccount
    let reactions: ReactionsModel
    let here: ChannelId = id(11)
    let it: MessageId = id(100)
    var viewed: [MessageId] = []
    var asked: [MessageActionsModel.Confirmation] = []
    var failures: [MessageActionsModel.Failure] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.reactions[id(100)] = [reaction("👍", 2), reaction("🎉", 1, me: true)]
        }
        reactions = ReactionsModel(store: store, account: account)
    }

    func model(reactions: ReactionsModel? = nil, without: Bool = false) -> MessageActionsModel {
        MessageActionsModel(
            store: store, account: account, copy: { _ in }, retry: { _ in }, discard: { _ in },
            createThread: { _, _ in }, viewThread: { _, _ in },
            reactions: without ? nil : reactions ?? self.reactions,
            viewReactions: { [unowned self] in viewed.append($0.message.id) },
            confirm: { [unowned self] in asked.append($0) },
            failed: { [unowned self] in failures.append($0) })
    }

    func row(
        _ actions: MessageActions = reacting(), _ raw: UInt64 = 100, delivery: Delivery = .sent
    ) -> MessageListModel.Row {
        store.update { $0.allow(actions, message: raw, in: 11) }
        return MessageListModel.Row(id: id(raw), message: message(raw, in: 11, delivery: delivery))
    }

    func bar(_ row: MessageListModel.Row, shift: Bool = false) -> [MessageAction] {
        model().bar(for: row, shift: shift).map(\.action)
    }

    func menu(_ row: MessageListModel.Row) -> [[MessageAction]] {
        model().menu(for: row).map { $0.map(\.action) }
    }

    var calls: [CallLog.Call] { store.log.actions }

    @Test
    func theBarStartsWithAddReactionWhereTheUserMayReact() {
        #expect(bar(row()) == [.addReaction, .more])
        #expect(bar(row(reacting(react: false))) == [.more])
        #expect(bar(row(), shift: true) == [.copyLink, .markUnread, .addReaction, .more])
    }

    @Test
    func theBarsQuickReactionsGoWithAddReactionAndLeaveWithShift() {
        #expect(model().quickReactions(for: row()).map(\.name) == ["💯", "😆", "💖"])
        #expect(model().quickReactions(for: row(), shift: true).isEmpty)
        #expect(model().quickReactions(for: row(reacting(react: false))).isEmpty)
        #expect(model(without: true).quickReactions(for: row()).isEmpty)
    }

    @Test
    func theMenuStartsWithTheReactionsGroup() {
        #expect(
            menu(row()) == [[.addReaction, .viewReactions], [.copyText, .markUnread, .copyLink]])
        #expect(
            menu(row(reacting(react: false, toggle: false))) == [
                [.viewReactions], [.copyText, .markUnread, .copyLink],
            ])
        #expect(menu(row(reacting(), 101)) == [[.addReaction], [.copyText, .markUnread, .copyLink]])
    }

    @Test
    func moderatorsHaveTheRemoveItems() {
        #expect(
            menu(row(reacting(remove: true))).last == [.removeReactions, .removeAllReactions])
        #expect(menu(row(reacting(remove: true), 101)).last == [.copyText, .markUnread, .copyLink])
        #expect(
            model().menu(for: row(reacting(remove: true))).last?.map(\.destructive) == [true, true])
    }

    @Test
    func withoutTheListsReactionsOrWithTheSettingOffNothingIsOffered() {
        let allowed = row(reacting(remove: true))
        #expect(model(without: true).menu(for: allowed).map { $0.map(\.action) }
            == [[.copyText, .markUnread, .copyLink]])
        #expect(model(without: true).bar(for: allowed, shift: false).map(\.action) == [.more])

        store.update { $0.displaySettings.renderReactions = false }
        let hidden = ReactionsModel(store: store, account: account)
        #expect(model(reactions: hidden).menu(for: allowed).map { $0.map(\.action) }
            == [[.copyText, .markUnread, .copyLink]])
        #expect(model(reactions: hidden).quickReactions(for: allowed).isEmpty)
    }

    @Test
    func aFailedSendOffersNoReaction() {
        let failed = row(reacting(), 101, delivery: .failed)

        #expect(bar(failed) == [.retry])
        #expect(model().quickReactions(for: failed).isEmpty)
        #expect(!menu(failed).joined().contains(.addReaction))
    }

    @Test
    func aQuickReactionReactsFromWhereItWasClicked() async throws {
        let message = row()
        let quick = try #require(model().quickReactions(for: message).first)

        await model().react(quick, on: message, from: .hoverBar)

        #expect(calls == [.react(here, it, "💯", .hoverBar)])
    }

    @Test
    func viewReactionsOpensTheWindow() {
        model().perform(.viewReactions, on: row())

        #expect(viewed == [it])
    }

    @Test
    func removeAllReactionsAsksFirst() async throws {
        model().perform(.removeAllReactions, on: row(reacting(remove: true)))

        #expect(calls.isEmpty)
        let question = try #require(asked.first)
        #expect(question.title == "Remove All Reactions")
        #expect(question.message == "Are you sure you want to remove all reactions from this message?")
        #expect(question.confirm == "Yes")
        #expect(question.cancel == "No")

        question.run()
        try await eventually { self.calls == [.removeReactions(self.here, self.it, nil)] }
    }

    @Test
    func shiftSkipsTheQuestion() async throws {
        model().perform(.removeAllReactions, on: row(reacting(remove: true)), shift: true)

        try await eventually { self.calls == [.removeReactions(self.here, self.it, nil)] }
        #expect(asked.isEmpty)
    }

    @Test
    func removeReactionsListsTheMessagesEmojiAndRemovesOne() async throws {
        let message = row(reacting(remove: true))
        let choices = model().removableReactions(for: message)
        #expect(choices.map(\.name) == ["👍", "🎉"])
        #expect(model().removableReactions(for: row(reacting())).isEmpty)

        model().removeReactions(of: choices[1], on: message, shift: false)
        let question = try #require(asked.first)
        #expect(question.title == "Remove Reactions for Emoji")
        #expect(
            question.message
                == "Are you sure you want to remove all of the reactions for this emoji from this message?")
        question.run()

        try await eventually { self.calls == [.removeReactions(self.here, self.it, "🎉")] }
    }

    @Test
    func aRefusedRemovalSaysSo() async {
        account.reactionError.withLock { $0 = .Forbidden }

        await MessageActionsModel.removeReactions(
            of: nil, in: here, from: it, account: account, failed: { failures.append($0) })

        #expect(
            failures == [
                .init(
                    title: "Couldn't Remove Reactions",
                    message: "You don't have permission to do that here.")
            ])
    }
}
