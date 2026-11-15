import Foundation
import Testing

@testable import AkariKit

// Pin and Unpin in the bar and the menu: channel 11 in guild 1 (#general) with Ray's message
// 100; DM 20 with Ray; thread 31 under 11 starting from message 7.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PinActionsTests {
    let store = FakeStore()
    let account: FakeAccount
    var questions: [MessageActionsModel.PinQuestion] = []
    var failures: [MessageActionsModel.Failure] = []
    var confirmations: [MessageActionsModel.Confirmation] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11, name: "general"), thread(31, in: 11, name: "plans")], in: 1)
            state.channels[id(20)] = dm(20, with: 2)
        }
    }

    func model() -> MessageActionsModel {
        MessageActionsModel(
            store: store, account: account, copy: { _ in }, retry: { _ in }, discard: { _ in },
            createThread: { _, _ in }, viewThread: { _, _ in },
            confirm: { [unowned self] in confirmations.append($0) },
            askPin: { [unowned self] in questions.append($0) },
            failed: { [unowned self] in failures.append($0) })
    }

    func row(
        _ raw: UInt64, in channel: UInt64 = 11, _ actions: MessageActions? = allowed(pin: true),
        content: String? = "hello", delivery: Delivery = .sent
    ) -> MessageListModel.Row {
        let message = message(raw, in: channel, by: user(2, name: "Ray"), content: content, delivery: delivery)
        store.update { $0.allow(actions, message: raw, in: channel) }
        return MessageListModel.Row(id: id(raw), message: message)
    }

    func bar(_ row: MessageListModel.Row, shift: Bool) -> [MessageAction] {
        model().bar(for: row, shift: shift).map(\.action)
    }

    func menu(_ row: MessageListModel.Row) -> [[MessageAction]] {
        model().menu(for: row).map { $0.map(\.action) }
    }

    // MARK: Where they are

    @Test
    func pinStandsInTheShiftBarAfterMarkUnreadAndInTheMenuAfterCopyText() {
        let message = row(100)

        #expect(bar(message, shift: false) == [.more])
        #expect(bar(message, shift: true) == [.copyLink, .markUnread, .pin, .more])
        #expect(menu(message) == [[.copyText, .pin, .markUnread, .copyLink]])
    }

    @Test
    func aPinnedMessageHasUnpinInTheSamePlaces() {
        let message = row(100, allowed(unpin: true))

        #expect(bar(message, shift: true) == [.copyLink, .markUnread, .unpin, .more])
        #expect(menu(message) == [[.copyText, .unpin, .markUnread, .copyLink]])
    }

    @Test
    func theirWordsAndSymbolsAreTheOfficialClients() {
        let pin = model().bar(for: row(100), shift: true)[2]
        let unpin = model().menu(for: row(101, allowed(unpin: true)))[0][1]

        #expect([pin.title, pin.symbol] == ["Pin Message", "pin"])
        #expect([unpin.title, unpin.symbol] == ["Unpin Message", "pin.slash"])
        #expect(pin.enabled && unpin.enabled)
        #expect(!pin.destructive && !unpin.destructive)
    }

    @Test
    func neitherIsWhereTheCoreSaysNoOrOnAFailedSend() {
        #expect(menu(row(100, allowed())) == [[.copyText, .markUnread, .copyLink]])
        let failed = row(101, allowed(pin: true, markUnread: false, copyLink: false), delivery: .failed)
        #expect(menu(failed) == [[.copyText, .retry]])
        #expect(bar(failed, shift: true) == [.retry])
        let failedPinned = row(104, allowed(unpin: true, markUnread: false, copyLink: false), delivery: .failed)
        #expect(menu(failedPinned) == [[.copyText, .retry]])

        model().perform(.pin, on: row(102, allowed()))
        model().perform(.unpin, on: row(103, allowed(pin: true)))
        #expect(questions.isEmpty)
        #expect(!store.log.calls.contains { if case .pin = $0 { true } else { false } })
    }

    // MARK: The questions

    @Test(arguments: [MessageActionsModel.Surface.bar, .menu])
    func pinAsksTheOfficialQuestionAndOnlyItsYesPins(from surface: MessageActionsModel.Surface)
        async throws
    {
        model().perform(.pin, on: row(100), shift: false, from: surface)

        let question = try #require(questions.first)
        #expect(question.pins)
        #expect(question.row.message.id == id(100))
        #expect(question.title == "Pin It. Pin It Good.")
        #expect(
            question.message
                == "Hey, just double checking that you want to pin this message to #general for "
                + "posterity and greatness?")
        #expect(question.tip == nil)
        #expect(question.confirm == "Oh yeah. Pin it")
        #expect(question.cancel == "Cancel")
        #expect(!question.destructive)
        #expect(!store.log.calls.contains(.pin(id(11), id(100))), "nothing before the answer")

        question.run()
        try await eventually { self.store.log.calls.contains(.pin(id(11), id(100))) }
    }

    @Test
    func aConversationsQuestionSpeaksOfTheCurrentChannel() throws {
        model().perform(.pin, on: row(100, in: 20))

        #expect(
            try #require(questions.first).message
                == "Hey, just double checking that you want to pin this message to the current "
                + "channel for posterity and greatness?")
    }

    @Test
    func unpinAsksWithTheProtipAndARedYes() async throws {
        model().perform(.unpin, on: row(100, allowed(unpin: true)), shift: false, from: .bar)

        let question = try #require(questions.first)
        #expect(!question.pins)
        #expect(question.title == "Unpin Message")
        #expect(question.message == "You sure you want to remove this pinned message?")
        #expect(
            question.tip
                == MessageTip(
                    label: "Protip:",
                    text: "You can hold down shift when clicking unpin message to bypass this "
                        + "confirmation entirely.",
                    bold: "unpin message"))
        #expect(question.confirm == "Remove it please!")
        #expect(question.destructive)

        question.run()
        try await eventually { self.store.log.calls.contains(.unpin(id(11), id(100))) }
    }

    @Test(arguments: [MessageAction.pin, .unpin])
    func shiftSkipsTheQuestion(_ action: MessageAction) async throws {
        let message = row(100, action == .pin ? allowed(pin: true) : allowed(unpin: true))

        model().perform(action, on: message, shift: true, from: .menu)

        let call: CallLog.Call = action == .pin ? .pin(id(11), id(100)) : .unpin(id(11), id(100))
        try await eventually { self.store.log.calls.contains(call) }
        #expect(questions.isEmpty)
    }

    // The core resolves a starter row's IDs; the question names the channel the message is in.
    @Test
    func aStarterRowPinsWithItsOwnIDsAndNamesTheMessagesChannel() async throws {
        let original = message(7, in: 11, by: user(2, name: "Ray"))
        let starter = MessageListModel.Row(
            id: id(31), message: message(31, in: 31, kind: .threadStarterMessage),
            starter: .loaded(original))
        store.update { $0.allow(allowed(pin: true), message: 31, in: 31) }

        model().perform(.pin, on: starter)
        let question = try #require(questions.first)
        #expect(question.message.contains("#general"))
        question.run()

        try await eventually { self.store.log.calls.contains(.pin(id(31), id(31))) }
    }

    // MARK: Discord's refusals

    @Test(arguments: [
        (PinError.Forbidden, "No Access", "You do not have permission to pin messages in this channel."),
        (.WrongChannel, "Invalid Channel Type", "This message cannot be pinned in this channel."),
        (.Archived, "Invalid Thread Archive State", "This message cannot be pinned in an archived thread."),
        (.SystemMessage, "Invalid Message Type", "This message is a system message and cannot be pinned."),
        (.Gone, "Unknown Message", "The message you are trying to pin does not exist."),
        (
            .Request(error: .Discord(status: 400, code: 50035, message: "Invalid Form Body")),
            "The Pin Broke", "Invalid Form Body"
        ),
        (
            .Request(error: .Discord(status: 400, code: 50035, message: "")), "The Pin Broke",
            "An error occurred, please try again."
        ),
        (.Request(error: .Network(kind: .connect)), "Error", "An error occurred, please try again."),
        (.Request(error: .ServerError(status: 502)), "Error", "An error occurred, please try again."),
    ])
    func aRefusedPinShowsTheOfficialAlert(error: PinError, title: String, text: String) async throws {
        account.pinError.withLock { $0 = error }

        model().perform(.pin, on: row(100), shift: true)

        try await eventually { !self.failures.isEmpty }
        #expect(failures == [.init(title: title, message: text)])
    }

    @Test
    func tooManyPinsNamesTheChannelOrSpeaksOfThisChannel() async throws {
        account.pinError.withLock { $0 = .TooMany(limit: 250) }

        model().perform(.pin, on: row(100), shift: true)
        model().perform(.pin, on: row(101, in: 20), shift: true)

        try await eventually { self.failures.count == 2 }
        #expect(Set(failures.map(\.title)) == ["The Pin Broke"])
        #expect(
            Set(failures.map(\.message)) == [
                "Discord is unable to pin that message. You may have hit the limit of 250 pins in "
                    + "channel #general.",
                "Discord is unable to pin that message. You may have hit the limit of 250 pins in "
                    + "this channel.",
            ])
    }

    @Test
    func aRefusedUnpinHasOneAlertWithTryAgain() async throws {
        account.pinError.withLock { $0 = .Request(error: .Network(kind: .connect)) }

        model().perform(.unpin, on: row(100, allowed(unpin: true)), shift: true)

        try await eventually { !self.confirmations.isEmpty }
        let alert = try #require(confirmations.first)
        #expect(alert.title == "The Pin Is Stuck!")
        #expect(alert.message == "Discord failed to unpin that message for some reason. Sorry about that.")
        #expect(alert.confirm == "Try Again")
        #expect(alert.cancel == "Cancel")
        #expect(!alert.destructive)
        #expect(failures.isEmpty)

        account.pinError.withLock { $0 = nil }
        alert.run()
        try await eventually { self.store.log.calls.filter { $0 == .unpin(id(11), id(100)) }.count == 2 }
        #expect(confirmations.count == 1)
    }

    // The question may be answered after the host let the model go.
    @Test
    func theQuestionPinsWithoutItsModel() async throws {
        model().perform(.pin, on: row(100))

        try #require(questions.first).run()

        try await eventually { self.store.log.calls.contains(.pin(id(11), id(100))) }
    }
}

// The session asks and answers Pin's and Unpin's questions.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionPinQuestionTests {
    let store = FakeStore()
    let account: FakeAccount
    let session: SessionModel

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.show(message(100, in: 11), message(101, in: 11))
            state.windows[id(11)] = window([100, 101])
            state.allow(allowed(pin: true), message: 100, in: 11)
        }
        session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
    }

    func list() throws -> MessageListModel {
        let list = try #require(session.messages)
        list.apply(EventBatch([.messagesLoaded(channelId: id(11), first: id(100), last: id(101))]))
        return list
    }

    @Test
    func pinAsksTheSessionAndOnlyItsYesPins() async throws {
        let list = try list()
        let model = session.messageActions(for: list) { _ in }
        let row = try #require(list.rows.first)

        model.perform(.pin, on: row, from: .menu)
        #expect(session.pinQuestion?.row.message.id == id(100))
        session.answerPinQuestion(false)
        #expect(session.pinQuestion == nil)
        #expect(!store.log.calls.contains(.pin(id(11), id(100))))

        model.perform(.pin, on: row, from: .bar)
        session.answerPinQuestion(true)
        try await eventually { self.store.log.calls.contains(.pin(id(11), id(100))) }
        #expect(store.log.calls.filter { $0 == .pin(id(11), id(100)) }.count == 1)
        session.close()
    }

    @Test
    func aRefusedPinAlertsUntilDismissed() async throws {
        account.pinError.withLock { $0 = .Forbidden }
        let list = try list()

        session.messageActions(for: list) { _ in }.perform(.pin, on: try #require(list.rows.first), shift: true)

        try await eventually { self.session.messageActionFailure != nil }
        #expect(session.messageActionFailure?.title == "No Access")
        session.close()
    }

    @Test
    func aQuestionWhoseMessageGoesMeanwhileCloses() throws {
        let list = try list()
        session.messageActions(for: list) { _ in }.perform(.pin, on: try #require(list.rows.first))
        #expect(session.pinQuestion != nil)

        session.apply(EventBatch([.messageDeleted(channelId: id(11), messageId: id(101))]))
        #expect(session.pinQuestion != nil, "another message's delete leaves it")
        session.apply(EventBatch([.messageDeleted(channelId: id(11), messageId: id(100))]))

        #expect(session.pinQuestion == nil)
        session.close()
    }

    @Test
    func aQuestionThatAPinFromElsewhereAnsweredCloses() throws {
        let list = try list()
        session.messageActions(for: list) { _ in }.perform(.pin, on: try #require(list.rows.first))

        session.apply(EventBatch([.messageUpdated(channelId: id(11), messageId: id(100))]))
        #expect(session.pinQuestion != nil, "an edit leaves it")
        store.update { $0.allow(allowed(unpin: true), message: 100, in: 11) }
        session.apply(EventBatch([.messageUpdated(channelId: id(11), messageId: id(100))]))

        #expect(session.pinQuestion == nil)
        session.close()
    }
}
