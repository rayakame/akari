import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1 with messages 100 (another's) and 101 (the user's, editable).
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReplyActionTests {
    let store = FakeStore()
    let account: FakeAccount
    var replies: [String] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)], in: 1)
        }
    }

    func model() -> MessageActionsModel {
        MessageActionsModel(
            store: store, account: account, copy: { _ in }, retry: { _ in }, discard: { _ in },
            createThread: { _, _ in }, viewThread: { _, _ in },
            reply: { [unowned self] row, shift in
                replies.append("\(row.message.id.rawValue) \(shift ? "shift" : "plain")")
            },
            failed: { _ in })
    }

    func row(_ raw: UInt64, _ actions: MessageActions, delivery: Delivery = .sent) -> MessageListModel.Row {
        store.update { $0.allow(actions, message: raw, in: 11) }
        return MessageListModel.Row(id: id(raw), message: message(raw, in: 11, delivery: delivery))
    }

    func bar(_ row: MessageListModel.Row, shift: Bool = false) -> [MessageAction] {
        model().bar(for: row, shift: shift).map(\.action)
    }

    @Test
    func anothersMessageHasReplyInTheBar() {
        let message = row(100, allowed(reply: true, createThread: true))

        #expect(bar(message) == [.reply, .createThread, .more])
        #expect(bar(message, shift: true) == [.copyLink, .markUnread, .reply, .createThread, .more])
    }

    @Test
    func theUsersOwnEditableMessageHasReplyOnlyWithShift() {
        let own = row(101, allowed(reply: true, edit: true, createThread: true))

        #expect(bar(own) == [.createThread, .more])
        #expect(bar(own, shift: true) == [.copyLink, .markUnread, .reply, .createThread, .more])
    }

    // The core's edit flag alone places Reply; what else the row allows moves nothing.
    @Test(arguments: [false, true], [false, true])
    func replyStandsWhereTheCoresEditFlagSays(edit: Bool, delete: Bool) {
        let message = row(100, allowed(reply: true, edit: edit, delete: delete, createThread: true))

        #expect(bar(message) == (edit ? [.createThread, .more] : [.reply, .createThread, .more]))
        #expect(bar(message, shift: true) == [.copyLink, .markUnread, .reply, .createThread, .more])
    }

    @Test
    func replyIsOnceInEitherBarWhateverTheRules() {
        typealias When = MessageActionsModel.When
        for (editable, deletable) in [(false, false), (true, false), (false, true), (true, true)] {
            func replies(_ slots: [MessageActionsModel.Slot]) -> Int {
                slots.count { $0.action == .reply && $0.when.fits(editable: editable, deletable: deletable) }
            }
            #expect(replies(MessageActionsModel.barOrder) == (editable ? 0 : 1))
            #expect(replies(MessageActionsModel.shiftBarOrder) == 1)
        }
    }

    @Test
    func aMessageThatCantBeAnsweredHasNoReply() {
        let message = row(100, allowed())

        #expect(bar(message) == [.more])
        #expect(!model().menu(for: message).joined().contains { $0.action == .reply })
    }

    @Test
    func theMenuHasReplyBeforeCreateThread() {
        let groups = model().menu(for: row(100, allowed(reply: true, createThread: true)))

        #expect(groups.first?.map(\.action) == [.reply, .createThread])
        let item = groups.first?.first
        #expect(item?.title == "Reply")
        #expect(item?.symbol == "arrowshape.turn.up.left")
        #expect(item?.destructive == false)
    }

    @Test
    func aFailedSendOffersNoReply() {
        let failed = row(100, allowed(reply: false, delete: true), delivery: .failed)

        #expect(bar(failed) == [.retry, .delete])
    }

    @Test(arguments: [false, true])
    func replyHandsTheRowAndShiftOn(shift: Bool) {
        let message = row(100, allowed(reply: true))

        model().perform(.reply, on: message, shift: shift)

        #expect(replies == ["100 \(shift ? "shift" : "plain")"])
    }
}
