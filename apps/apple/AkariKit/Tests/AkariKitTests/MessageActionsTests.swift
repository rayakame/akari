import Foundation
import Testing

@testable import AkariKit

func allowed(
    reply: Bool = false, edit: Bool = false, delete: Bool = false, pin: Bool = false,
    unpin: Bool = false, react: Bool = false, markUnread: Bool = true, copyLink: Bool = true,
    createThread: Bool = false, viewThread: Bool = false
) -> MessageActions {
    MessageActions(
        reply: reply, replyPings: false, mentionToggle: false, edit: edit, delete: delete, pin: pin,
        unpin: unpin, react: react, toggleReaction: react, removeReactions: false,
        markUnread: markUnread, copyLink: copyLink, createThread: createThread,
        viewThread: viewThread)
}

private enum MarkAlert {
    static let network = "Akari couldn't reach Discord. Check your connection, then try again."
    static let permission = "You don't have permission to do that here."
    static let slowDown = "Discord is asking Akari to slow down. Try again in a moment."
    static let notConnected = "Akari isn't connected. Reconnect, then try again."
    static let problem = "Discord had a problem. Try again in a moment."
}

// Channel 11 in guild 1 with messages 100 (another's) and 101 (the user's); DM 20.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class MessageActionsTests {
    let store = FakeStore()
    let account: FakeAccount
    var copied: [String] = []
    var threadsCreated: [String] = []
    var threadsViewed: [String] = []
    var failures: [MessageActionsModel.Failure] = []
    var marks: [String] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.channels[id(20)] = dm(20, with: 2)
        }
    }

    func model() -> MessageActionsModel {
        MessageActionsModel(
            store: store, account: account,
            copy: { [unowned self] in copied.append($0) },
            retry: { _ in }, discard: { _ in },
            createThread: { [unowned self] in threadsCreated.append("\($0.rawValue) \($1.rawValue)") },
            viewThread: { [unowned self] in threadsViewed.append("\($0.rawValue) \($1.rawValue)") },
            marking: { [unowned self] channel, message in
                marks.append("mark \(channel.rawValue) \(message.rawValue)")
                return { [unowned self] in marks.append("undo \(message.rawValue)") }
            },
            failed: { [unowned self] in failures.append($0) })
    }

    func row(
        _ raw: UInt64, in channel: UInt64 = 11, _ actions: MessageActions? = allowed(),
        content: String? = nil, delivery: Delivery = .sent, kind: MessageType = .default
    ) -> MessageListModel.Row {
        let message = message(raw, in: channel, content: content, delivery: delivery, kind: kind)
        store.update { $0.allow(actions, message: raw, in: channel) }
        return MessageListModel.Row(id: id(raw), message: message)
    }

    func bar(_ row: MessageListModel.Row, shift: Bool = false) -> [MessageAction] {
        model().bar(for: row, shift: shift).map(\.action)
    }

    func menu(_ row: MessageListModel.Row) -> [[MessageAction]] {
        model().menu(for: row).map { $0.map(\.action) }
    }

    @Test
    func theBarOfAnotherUsersMessageEndsWithMore() {
        #expect(bar(row(100)) == [.more])
        #expect(bar(row(100, allowed(createThread: true))) == [.createThread, .more])
        #expect(bar(row(100, allowed(viewThread: true))) == [.viewThread, .more])
    }

    @Test
    func shiftPutsCopyLinkAndMarkUnreadFirst() {
        let message = row(100, allowed(createThread: true))

        #expect(bar(message, shift: true) == [.copyLink, .markUnread, .createThread, .more])
        #expect(bar(row(101, allowed(markUnread: false)), shift: true) == [.copyLink, .more])
    }

    @Test
    func theBarsWordsAreTheOfficialClients() {
        let items = model().bar(for: row(100, allowed(createThread: true)), shift: true)

        #expect(items.map(\.title) == ["Copy Link", "Mark Unread", "Create Thread", "More"])
        #expect(items.map(\.symbol) == ["link", "message.badge", "text.bubble", "ellipsis"])
        #expect(items.allSatisfy { $0.enabled && !$0.destructive })
        #expect(model().bar(for: row(101, allowed(viewThread: true)), shift: false).first?.title == "View Thread")
    }

    @Test
    func aRowThatAllowsNothingHasNoBar() {
        let nothing = allowed(markUnread: false, copyLink: false)

        #expect(bar(row(100, nothing)).isEmpty)
        #expect(bar(row(100, nothing), shift: true).isEmpty)
        #expect(bar(row(100, nil, delivery: .pending)).isEmpty)
    }

    @Test
    func aFailedSendHasRetryAndDeleteWithOrWithoutShift() {
        let failed = row(100, allowed(delete: true, markUnread: false, copyLink: false), delivery: .failed)
        let items = model().bar(for: failed, shift: false)

        #expect(items.map(\.action) == [.retry, .delete])
        #expect(items.map(\.title) == ["Retry", "Delete"])
        #expect(items.map(\.symbol) == ["arrow.clockwise", "trash"])
        #expect(items.map(\.destructive) == [false, false])
        #expect(bar(failed, shift: true) == [.retry, .delete])
    }

    @Test
    func deleteIsOnlyWhereTheCoreAllowsIt() {
        let kept = row(
            100, allowed(markUnread: false, copyLink: false), content: "x", delivery: .failed)

        #expect(bar(kept) == [.retry])
        #expect(bar(kept, shift: true) == [.retry])
        #expect(menu(kept) == [[.copyText, .retry]])
    }

    // Deleting a sent message isn't built yet: only a failed send has it.
    @Test
    func aSentMessageHasNoDeleteYet() {
        let sent = row(100, allowed(delete: true), content: "x")

        #expect(bar(sent, shift: true) == [.copyLink, .markUnread, .more])
        #expect(menu(sent) == [[.copyText, .markUnread, .copyLink]])
    }

    @Test
    func aSystemLineHasTheSameBar() {
        let join = row(100, allowed(), kind: .userJoin)

        #expect(bar(join) == [.more])
        #expect(bar(join, shift: true) == [.copyLink, .markUnread, .more])
    }

    @Test
    func theMenuKeepsTheOfficialClientsGroups() {
        let message = row(100, allowed(createThread: true), content: "hello")
        let items = model().menu(for: message)

        #expect(items.map { $0.map(\.action) } == [[.createThread], [.copyText, .markUnread, .copyLink]])
        #expect(
            items.map { $0.map(\.title) }
                == [["Create Thread"], ["Copy Text", "Mark Unread", "Copy Message Link"]])
    }

    @Test
    func theMenuLeavesOutWhatDoesNotApply() {
        #expect(menu(row(100, allowed(viewThread: true), content: "")) == [[.viewThread], [.markUnread, .copyLink]])
        #expect(menu(row(100, allowed(markUnread: false), content: "x")) == [[.copyText, .copyLink]])
        #expect(menu(row(100, nil, content: "x", delivery: .pending)) == [[.copyText]])
        #expect(menu(row(100, nil, content: "", delivery: .pending)).isEmpty)
    }

    @Test
    func aFailedSendsMenuResendsAndDeletes() {
        let failed = row(
            100, allowed(delete: true, markUnread: false, copyLink: false), content: "x",
            delivery: .failed)
        let items = model().menu(for: failed)

        #expect(items.map { $0.map(\.title) } == [["Copy Text", "Resend message"], ["Delete Message"]])
        #expect(items[1][0].destructive)
    }

    // What later features add falls into these places.
    @Test
    func theOrdersNameEveryActionOnce() {
        #expect(
            MessageActionsModel.barOrder.map(\.action)
                == [.addReaction, .reply, .edit, .createThread, .viewThread, .more])
        #expect(
            MessageActionsModel.shiftBarOrder.map(\.action)
                == [
                    .copyId, .copyLink, .markUnread, .pin, .unpin, .reply, .addReaction, .reply,
                    .edit, .createThread, .viewThread, .delete, .more,
                ])
        #expect(
            MessageActionsModel.menuOrder
                == [
                    [.addReaction, .viewReactions],
                    [.edit, .reply, .createThread, .viewThread],
                    [.copyText, .pin, .unpin, .retry, .markUnread, .copyLink],
                    [.removeReactions, .removeAllReactions, .delete],
                    [.copyId],
                ])
        #expect(
            MessageActionsModel.barOrder.map(\.when)
                == [.always, .notEditable, .always, .always, .always, .always])
        #expect(
            MessageActionsModel.shiftBarOrder.map(\.when)
                == [
                    .always, .always, .always, .always, .always, .editable, .always, .notEditable,
                    .always, .always, .always, .always, .withoutDelete,
                ])
        let listed = Set(MessageActionsModel.menuOrder.joined())
            .union(MessageActionsModel.shiftBarOrder.map(\.action))
        #expect(listed == Set(MessageAction.allCases))
    }

    // Reply stands in the first row unless the message can be edited, then in the Shift row;
    // More gives way to Delete.
    @Test
    func aSlotsRuleSaysWhenItFits() {
        typealias When = MessageActionsModel.When
        func fits(_ when: When) -> [Bool] {
            [(false, false), (true, false), (false, true), (true, true)].map {
                when.fits(editable: $0.0, deletable: $0.1)
            }
        }

        #expect(fits(.always) == [true, true, true, true])
        #expect(fits(.editable) == [false, true, false, true])
        #expect(fits(.notEditable) == [true, false, true, false])
        #expect(fits(.withoutDelete) == [true, true, false, false])
    }

    @Test
    func theBarAsksTheStoreEachTime() {
        let message = row(100, allowed(createThread: true))
        #expect(bar(message) == [.createThread, .more])

        store.update { $0.allow(allowed(viewThread: true), message: 100, in: 11) }

        #expect(bar(message) == [.viewThread, .more])
        #expect(store.reads.filter { $0 == .messageActions(id(11), id(100)) }.count == 2)
    }

    // The link is the core's, asked with the row's IDs.
    @Test
    func copyTextAndTheLinkGoToTheHost() {
        let message = row(100, content: "hello **there**")
        let model = model()

        model.perform(.copyText, on: message)
        model.perform(.copyLink, on: message)
        model.perform(.copyLink, on: row(200, in: 20))

        #expect(copied == ["hello **there**", "link 11 100", "link 20 200"])
        #expect(store.reads.contains(.messageLink(id(11), id(100))))
        #expect(store.reads.contains(.messageLink(id(20), id(200))))
    }

    @Test
    func theThreadActionsNameTheMessagesChannel() {
        let model = model()

        model.perform(.createThread, on: row(100, allowed(createThread: true)))
        model.perform(.viewThread, on: row(101, allowed(viewThread: true)))

        #expect(threadsCreated == ["11 100"])
        #expect(threadsViewed == ["101 11"])
    }

    @Test
    func aStarterRowsThreadActionsNameTheOriginal() {
        let model = model()

        model.perform(.createThread, on: starterRow())
        model.perform(.viewThread, on: starterRow())

        #expect(threadsCreated == ["11 100"])
        #expect(threadsViewed == ["100 11"])
    }

    @Test
    func theMarkUnreadItemMarks() async throws {
        model().perform(.markUnread, on: row(100))

        try await eventually { self.store.log.calls.contains(.markUnread(id(11), id(100))) }
        #expect(marks == ["mark 11 100"])
        #expect(failures.isEmpty)
    }

    @Test
    func markUnreadAsksTheAccount() async {
        let model = model()

        await model.markUnread(from: row(100))

        #expect(store.log.calls.contains(.markUnread(id(11), id(100))))
        #expect(failures.isEmpty)
        #expect(marks == ["mark 11 100"])
    }

    @Test
    func aMessageThatCannotBeMarkedIsNotAsked() async {
        await model().markUnread(from: row(100, allowed(markUnread: false)))

        #expect(!store.log.calls.contains(.markUnread(id(11), id(100))))
    }

    @Test(arguments: [
        (RequestError.Network(kind: .connect), MarkAlert.network),
        (.Discord(status: 403, code: 50001, message: "Missing Access"), MarkAlert.permission),
        (.Discord(status: 400, code: 50013, message: "Missing Permissions"), MarkAlert.permission),
        (.Discord(status: 404, code: 10008, message: "Unknown Message"), MarkAlert.problem),
        (.RateLimited(retryAfter: 2), MarkAlert.slowDown),
        (.Closed, MarkAlert.notConnected),
        (.ServerError(status: 502), MarkAlert.problem),
    ])
    func aRefusedMarkSaysSoOnce(error: RequestError, text: String) async {
        account.markUnreadError.withLock { $0 = error }

        await model().markUnread(from: row(100))

        #expect(marks == ["mark 11 100", "undo 100"])
        #expect(failures == [.init(title: "Couldn't Mark as Unread", message: text)])
    }

    // An ack, another mark, another device or a READY took its place: nothing for the user to do.
    @Test
    func aReplacedMarkIsTakenBackWithoutAnAlert() async {
        account.markUnreadOutcome.withLock { $0 = .replaced }

        await model().markUnread(from: row(100))

        #expect(marks == ["mark 11 100", "undo 100"])
        #expect(failures.isEmpty)
    }

    // As Discord sends it, the row's own message has no text.
    func starterRow(showing text: String = "the start", own: String = "") -> MessageListModel.Row {
        let original = message(100, in: 11, content: text)
        let starter = message(300, in: 100, content: own, kind: .threadStarterMessage)
        return MessageListModel.Row(id: id(300), message: starter, starter: .loaded(original))
    }

    // The core answers for the row, asked with the row's own IDs; the actions take the original's.
    @Test
    func aStarterRowAsksAsItselfAndActsOnTheOriginal() async {
        store.update { $0.allow(allowed(), message: 300, in: 100) }
        let row = starterRow()
        let model = model()

        #expect(model.bar(for: row, shift: true).map(\.action) == [.copyLink, .markUnread, .more])
        #expect(model.menu(for: row).map { $0.map(\.action) } == [[.copyText, .markUnread, .copyLink]])
        model.perform(.copyText, on: row)
        model.perform(.copyLink, on: row)
        await model.markUnread(from: row)

        #expect(copied == ["the start", "link 100 300"], "the core resolves the row to its message")
        #expect(store.log.calls.contains(.markUnread(id(100), id(300))))
        #expect(marks == ["mark 100 300"])
        let asked = store.reads.filter { if case .messageActions = $0 { true } else { false } }
        #expect(!asked.isEmpty)
        #expect(asked.allSatisfy { $0 == .messageActions(id(100), id(300)) })
    }

    @Test
    func aStarterRowCopiesTheTextItShows() {
        store.update { $0.allow(allowed(), message: 300, in: 100) }
        let empty = starterRow(showing: "", own: "never shown")
        let model = model()

        #expect(model.menu(for: empty).map { $0.map(\.action) } == [[.markUnread, .copyLink]])
        #expect(model.menu(for: starterRow()).first?.first?.action == .copyText)
    }

    @Test
    func aStarterRowOffersOnlyWhatTheCoreSaysForIt() async {
        store.update { state in
            state.allow(allowed(createThread: true, viewThread: true), message: 100, in: 11)
            state.allow(allowed(markUnread: false), message: 300, in: 100)
        }
        let row = starterRow()
        let model = model()

        #expect(model.bar(for: row, shift: true).map(\.action) == [.copyLink, .more])
        await model.markUnread(from: row)

        #expect(marks.isEmpty)
        #expect(!store.log.calls.contains(.markUnread(id(100), id(300))))
    }
}

// A session with channel 11 open: what its lists' menus and bars do.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionMessageActionsTests {
    let store = FakeStore()
    let account: FakeAccount
    let session: SessionModel

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.allow(allowed(), message: 100, in: 11)
        }
        session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
    }

    @Test
    func aFailedMarkAlertsUntilDismissed() async throws {
        account.markUnreadError.withLock { $0 = .ServerError(status: 502) }
        let list = try #require(session.messages)
        let model = session.messageActions(for: list) { _ in }

        await model.markUnread(from: MessageListModel.Row(id: id(100), message: message(100, in: 11)))

        #expect(session.messageActionFailure?.title == "Couldn't Mark as Unread")
        #expect(session.messageActionFailure?.message == "Discord had a problem. Try again in a moment.")
        session.dismissMessageActionFailure()
        #expect(session.messageActionFailure == nil)
        session.close()
    }

    @Test
    func aMarkMovesTheOpenListsLine() async throws {
        store.update { state in
            state.currentUser = user(1)
            for raw in UInt64(100)...102 {
                state.show(message(raw, in: 11, by: user(2)))
            }
            state.windows[id(11)] = window([100, 101, 102])
            state.readStates[id(11)] = ReadState(acked: id(102), mentions: 0, estimated: false)
            state.allow(allowed(), message: 101, in: 11)
            state.allow(allowed(), message: 100, in: 11)
        }
        let list = try #require(session.messages)
        list.apply(EventBatch([.messagesLoaded(channelId: id(11), first: id(100), last: id(102))]))
        let row = try #require(list.rows.first { $0.id == id(101) })
        #expect(list.newMessagesLine == nil)

        await session.messageActions(for: list) { _ in }.markUnread(from: row)
        #expect(list.newMessagesLine == id(101))

        account.markUnreadError.withLock { $0 = .ServerError(status: 502) }
        await session.messageActions(for: list) { _ in }.markUnread(from: try #require(list.rows.first))
        #expect(list.newMessagesLine == id(101))
        session.close()
    }

    func unreadFrom101() throws -> MessageListModel {
        store.update { state in
            state.currentUser = user(1)
            for raw in UInt64(100)...102 {
                state.show(message(raw, in: 11, by: user(2)))
            }
            state.windows[id(11)] = window([100, 101, 102])
            state.readStates[id(11)] = ReadState(acked: id(102), mentions: 0, estimated: false)
            state.allow(allowed(), message: 101, in: 11)
        }
        let list = try #require(session.messages)
        list.apply(EventBatch([.messagesLoaded(channelId: id(11), first: id(100), last: id(102))]))
        return list
    }

    @Test
    func aReplacedMarkTakesItsLineBack() async throws {
        let list = try unreadFrom101()
        let row = try #require(list.rows.first { $0.id == id(101) })
        account.markUnreadOutcome.withLock { $0 = .replaced }

        await session.messageActions(for: list) { _ in }.markUnread(from: row)

        #expect(list.newMessagesLine == nil)
        #expect(session.messageActionFailure == nil)
        session.close()
    }

    // Marked twice from the same message: the first mark's answer isn't the second one's.
    @Test
    func anEarlierMarksAnswerLeavesALaterMarksLine() throws {
        let list = try unreadFrom101()

        let first = list.markUnread(from: id(101))
        _ = list.markUnread(from: id(101))
        first()

        #expect(list.newMessagesLine == id(101))
        session.close()
    }

    @Test
    func aFailedSendsButtonsReachTheComposer() async throws {
        let list = try #require(session.messages)
        let model = session.messageActions(for: list) { _ in }
        let failed = MessageListModel.Row(id: id(100), message: message(100, in: 11, delivery: .failed))

        model.perform(.delete, on: MessageListModel.Row(id: id(101), message: message(101, in: 11)))
        #expect(!store.log.calls.contains(.discard(id(11), id(101))), "a sent message stays")
        model.perform(.delete, on: failed)
        model.perform(.retry, on: failed)
        try await eventually { self.store.log.calls.contains(.retry(id(11), id(100))) }

        #expect(store.log.calls.contains(.discard(id(11), id(100))))
        session.close()
    }

    @Test
    func theThreadItemsReachTheSession() async throws {
        store.update { state in
            state.channels[id(101)] = thread(101, in: 11)
            state.threadCreations[id(11)] = ThreadCreation(public: true, private: false)
            state.startable = [id(100)]
            state.show(message(100, in: 11), message(101, in: 11, hasThread: true))
        }
        let list = try #require(session.messages)
        let model = session.messageActions(for: list) { _ in }

        model.perform(.createThread, on: MessageListModel.Row(id: id(100), message: message(100, in: 11)))
        #expect(session.threadForm?.parentId == id(11))
        #expect(session.threadForm?.source?.id == id(100))
        #expect(session.threadForm?.location == .message)

        let parent = message(101, in: 11, hasThread: true)
        model.perform(.viewThread, on: MessageListModel.Row(id: id(101), message: parent))
        try await eventually { self.session.split?.channelId == id(101) }
        session.close()
    }

    // A row of another channel, as a list still showing while the session moved on.
    @Test
    func aMarkInAnotherChannelLeavesTheLine() async throws {
        let list = try unreadFrom101()
        store.update { $0.allow(allowed(), message: 101, in: 12) }

        await session.messageActions(for: list) { _ in }
            .markUnread(from: MessageListModel.Row(id: id(101), message: message(101, in: 12)))

        #expect(store.log.calls.contains(.markUnread(id(12), id(101))))
        #expect(list.newMessagesLine == nil)
        session.close()
    }
}
