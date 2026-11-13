import Foundation
import Testing

@testable import AkariKit

func replying(pings: Bool = true, toggle: Bool = true, edit: Bool = false) -> MessageActions {
    MessageActions(
        reply: true, replyPings: pings, mentionToggle: toggle, edit: edit, delete: false,
        pin: false, unpin: false, react: false, toggleReaction: false, removeReactions: false,
        markUnread: true, copyLink: true, createThread: false, viewThread: false)
}

// Channel 10 in guild 1 with messages 100 and 102 (Ray's) and 101 (the user's); DM 20 with
// message 200.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct PendingReplyTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let drafts = Drafts()

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            state.channels[id(20)] = dm(20, with: 2)
            state.allow(replying(), message: 100, in: 10)
            state.allow(replying(pings: false, toggle: false), message: 101, in: 10)
            state.allow(replying(), message: 102, in: 10)
            state.allow(replying(toggle: false), message: 200, in: 20)
        }
    }

    func composer(_ channel: UInt64 = 10) -> ComposerModel {
        ComposerModel(channelId: id(channel), account: account, store: store, drafts: drafts)
    }

    func row(_ raw: UInt64, in channel: UInt64 = 10, name: String = "Ray") -> MessageListModel.Row {
        MessageListModel.Row(
            id: id(raw), message: message(raw, in: channel, by: user(2, name: name)))
    }

    var replies: [CallLog.Call] {
        store.log.actions.filter { call in
            if case .reply = call { true } else { false }
        }
    }

    @Test
    func aReplyStartsWithThePingAndTheToggleAndAsksForTheFocus() {
        let composer = composer()

        composer.startReply(to: row(100), shift: false)

        #expect(
            composer.reply
                == PendingReply(messageId: id(100), authorName: "Ray", mention: true, showsToggle: true))
        #expect(composer.focusRequests == 1)
    }

    @Test
    func shiftStartsItWithoutThePing() {
        let composer = composer()

        composer.startReply(to: row(100), shift: true)

        #expect(composer.reply?.mention == false)
        #expect(composer.reply?.showsToggle == true)
    }

    @Test
    func aReplyToTheUsersOwnMessageNeverPingsAndHasNoToggle() {
        let composer = composer()

        composer.startReply(to: row(101), shift: false)

        #expect(composer.reply?.mention == false)
        #expect(composer.reply?.showsToggle == false)
        composer.toggleReplyMention()
        #expect(composer.reply?.mention == false)
    }

    @Test(arguments: [false, true])
    func aReplyInADMPingsUnlessShiftAndHasNoToggle(shift: Bool) {
        let composer = composer(20)

        composer.startReply(to: row(200, in: 20), shift: shift)

        #expect(composer.reply?.mention == !shift)
        #expect(composer.reply?.showsToggle == false)
    }

    @Test
    func aMessageThatCantBeAnsweredStartsNothing() {
        let composer = composer()
        store.update { $0.allow(allowed(), message: 100, in: 10) }

        composer.startReply(to: row(100), shift: false)

        #expect(composer.reply == nil)
        #expect(composer.focusRequests == 0)
    }

    @Test
    func anotherReplyReplacesThePendingOne() {
        let composer = composer()
        composer.startReply(to: row(100), shift: true)

        composer.startReply(to: row(102, name: "Kim"), shift: false)

        #expect(
            composer.reply
                == PendingReply(messageId: id(102), authorName: "Kim", mention: true, showsToggle: true))
        #expect(composer.focusRequests == 2)
    }

    @Test
    func theToggleFlipsThePing() {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)

        composer.toggleReplyMention()
        #expect(composer.reply?.mention == false)
        composer.toggleReplyMention()
        #expect(composer.reply?.mention == true)
    }

    @Test
    func cancellingEndsIt() {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)

        composer.cancelReply()

        #expect(composer.reply == nil)
    }

    @Test
    func itIsKeptPerChannelAcrossAChannelSwitch() {
        let first = composer()
        first.startReply(to: row(100), shift: false)
        first.toggleReplyMention()

        #expect(composer(20).reply == nil)
        #expect(
            composer().reply
                == PendingReply(messageId: id(100), authorName: "Ray", mention: false, showsToggle: true))

        first.cancelReply()
        #expect(composer().reply == nil)
    }

    @Test
    func aSendTakesTheReplyAndClearsItWhenItStarts() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: true)
        composer.draft = "hi"
        store.log.forget()
        account.holdSends.withLock { $0 = true }

        async let submitted: Void = composer.submit()
        await account.sendReplies.pulled(1)
        #expect(composer.reply == nil)
        account.sendReplies.send(.ServerError(status: 502))
        await submitted

        #expect(
            store.log.actions.prefix(2) == [.reply(here, id(100), mention: false), .send(here, "hi")])
        #expect(composer.reply == nil, "a failed send doesn't bring the bar back")
        #expect(self.composer().reply == nil)
    }

    @Test
    func aSendWithThePingOnSaysSo() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)
        composer.draft = "hi"

        await composer.submit()

        #expect(replies == [.reply(here, id(100), mention: true)])
    }

    @Test
    func aSendWithoutAReplyNamesNone() async {
        let composer = composer()
        composer.draft = "hi"

        await composer.submit()

        #expect(replies.isEmpty)
    }

    @Test
    func aSubmitThatSendsNothingKeepsTheReply() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)

        composer.draft = "  "
        await composer.submit()
        #expect(composer.reply != nil)

        composer.draft = String(repeating: "a", count: 2001)
        await composer.submit()
        #expect(composer.reply != nil)
        #expect(replies.isEmpty)
    }

    @Test
    func aTargetThatVanishedClearsTheReplyKeepsTheTextAndSaysWhy() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)
        composer.draft = " hi "
        account.queueError.withLock { $0 = .InvalidRequest }

        await composer.submit()

        #expect(composer.reply == nil)
        #expect(composer.draft == " hi ")
        #expect(composer.problem == .replyTargetGone)
        #expect(
            composer.problem?.text
                == "The message you were replying to isn't available any more. Your message wasn't sent.")

        account.queueError.withLock { $0 = nil }
        await composer.submit()
        #expect(composer.problem == nil)
        #expect(replies.isEmpty)
    }

    @Test
    func anotherRefusalKeepsTheReply() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)
        composer.draft = "hi"
        account.queueError.withLock { $0 = .Closed }

        await composer.submit()

        #expect(composer.reply?.messageId == id(100))
        #expect(composer.problem == .failed(.Closed))
    }

    @Test
    func anInvalidRequestWithoutAReplyIsNotAboutOne() async {
        let composer = composer()
        composer.draft = "hi"
        account.queueError.withLock { $0 = .InvalidRequest }

        await composer.submit()

        #expect(composer.problem == .failed(.InvalidRequest))
    }

    @Test
    func aReplyDiscordRefusedForItsDeletedTargetSaysSo() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)
        composer.draft = "hi"
        account.sendError.withLock { $0 = .ReplyTargetDeleted }

        await composer.submit()

        #expect(composer.problem == .failed(.ReplyTargetDeleted))
        #expect(composer.problem?.text == "The message you replied to was deleted.")
    }

    @Test
    func theTargetsDeletionEndsTheReply() {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)

        composer.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(102))]))
        #expect(composer.reply != nil)
        composer.apply(EventBatch([.messageDeleted(channelId: id(20), messageId: id(100))]))
        #expect(composer.reply != nil)

        composer.apply(
            EventBatch([
                .messageDeleted(channelId: here, messageId: id(102)),
                .messageDeleted(channelId: here, messageId: id(100)),
            ]))
        #expect(composer.reply == nil)
        #expect(self.composer().reply == nil)
    }

    @Test
    func aTargetTheStoreCallsDeletedEndsTheReply() {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)

        composer.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(100))]))
        #expect(composer.reply != nil, "edited or loaded")

        store.update { $0.replyTargets[id(100)] = .deleted }
        composer.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(100))]))
        #expect(composer.reply == nil)
    }
}

// Channel 11 in guild 1 with thread 31 beside it; message 1 in 11 and 4 in 31 can be answered.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReplyEscapeTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.channels[id(31)] = thread(31, in: 11, name: "t1")
            state.windows[id(11)] = window([1, 2])
            state.windows[id(31)] = window([4])
            state.show(
                message(1, in: 11, by: user(2)), message(2, in: 11, by: user(2)),
                message(4, in: 31, by: user(2)))
            state.channelBadges[id(11)] = badge(unread: true)
            state.allow(replying(), message: 1, in: 11)
            state.allow(replying(), message: 4, in: 31)
        }
    }

    func session() -> SessionModel {
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    func reply(in list: MessageListModel?) throws {
        let list = try #require(list)
        let row = try #require(list.rows.first)
        list.composer.startReply(to: row, shift: false)
        #expect(list.composer.reply != nil)
    }

    @Test
    func escapeCancelsAPendingReplyBeforeItMarksRead() async throws {
        let session = session()
        try reply(in: session.messages)

        #expect(!session.escape(), "nothing to scroll")
        await finished(session.escaping)

        #expect(session.messages?.composer.reply == nil)
        #expect(account.markedRead.current.isEmpty)

        #expect(session.escape())
        await finished(session.escaping)
        #expect(account.markedRead.current == [id(11)])
        session.close()
    }

    // Message 1 is being answered when the channel is marked unread from message 2.
    func replyingAndMarkedUnread(_ session: SessionModel) async throws -> MessageListModel {
        store.update { $0.allow(replying(), message: 2, in: 11) }
        let list = try #require(session.messages)
        try reply(in: list)
        let second = try #require(list.rows.first { $0.id == id(2) })
        await session.messageActions(for: list) { _ in }.markUnread(from: second)
        return list
    }

    @Test
    func aMarkUnreadOnAnotherMessageLeavesTheReply() async throws {
        let session = session()

        let list = try await replyingAndMarkedUnread(session)

        #expect(list.composer.reply?.messageId == id(1))
        #expect(list.replyingTo == id(1))
        #expect(list.newMessagesLine == id(2))
        #expect(account.held.current == [id(11)])
        session.close()
    }

    @Test
    func aReplyStartedAfterAMarkUnreadLeavesTheLine() async throws {
        let session = session()
        store.update { $0.allow(replying(), message: 2, in: 11) }
        let list = try #require(session.messages)
        let actions = session.messageActions(for: list) { _ in }
        await actions.markUnread(from: try #require(list.rows.first { $0.id == id(2) }))

        actions.perform(.reply, on: try #require(list.rows.first), shift: false)

        #expect(list.composer.reply?.messageId == id(1))
        #expect(list.newMessagesLine == id(2))
        #expect(account.held.current == [id(11)])
        session.close()
    }

    // The first Escape is the reply's: the channel stays unread and held until the second.
    @Test
    func escapeCancelsTheReplyAndLeavesTheChannelMarkedUnread() async throws {
        let session = session()
        let list = try await replyingAndMarkedUnread(session)
        let told = account.openCalls.current.count

        #expect(!session.escape(from: id(11)))
        await finished(session.escaping)

        #expect(list.composer.reply == nil)
        #expect(list.newMessagesLine == id(2))
        #expect(account.held.current == [id(11)])
        #expect(account.markedRead.current.isEmpty)

        #expect(session.escape(from: id(11)))
        await finished(session.escaping)

        #expect(account.markedRead.current == [id(11)])
        #expect(account.held.current.isEmpty)
        #expect(list.newMessagesLine == nil)
        #expect(account.openCalls.current.count == told, "the lists stay")
        session.close()
    }

    // An answer that comes after the reply was cancelled still moves only the line.
    @Test
    func aRefusedMarkTakesItsLineBackAndLeavesTheReply() async throws {
        let session = session()
        store.update { $0.allow(replying(), message: 2, in: 11) }
        let list = try #require(session.messages)
        try reply(in: list)
        let answers = account.holdMark(from: id(2))
        let second = try #require(list.rows.first { $0.id == id(2) })
        let mark = Task { await session.messageActions(for: list) { _ in }.markUnread(from: second) }
        await answers.pulled(1)
        #expect(list.newMessagesLine == id(2))

        answers.send(.success(.replaced))
        await finished(mark)

        #expect(list.newMessagesLine != id(2))
        #expect(list.composer.reply?.messageId == id(1))
        #expect(session.messageActionFailure == nil)
        session.close()
    }

    @Test
    func aListsOwnEscapeCancelsItsReplyFirst() async throws {
        let session = session()
        let list = try #require(session.messages)
        try reply(in: list)

        await list.escape()
        #expect(list.composer.reply == nil)
        #expect(account.markedRead.current.isEmpty)

        await list.escape()
        #expect(account.markedRead.current == [id(11)])
        session.close()
    }

    @Test
    func escapeCancelsThePaneItWasPressedInFirstAndKeepsTheSplit() async throws {
        let session = session()
        await session.openBeside(thread: id(31), from: id(11))
        try reply(in: session.messages)
        try reply(in: session.split)

        #expect(!session.escape(from: id(31)))
        #expect(session.split?.composer.reply == nil)
        #expect(session.messages?.composer.reply != nil)

        #expect(!session.escape(from: id(31)))
        #expect(session.messages?.composer.reply == nil)
        #expect(session.split != nil)
        #expect(account.markedRead.current.isEmpty)
        session.close()
    }

    @Test
    func withoutAPaneTheOpenChannelsReplyGoesFirst() async throws {
        let session = session()
        await session.openBeside(thread: id(31), from: id(11))
        try reply(in: session.messages)
        try reply(in: session.split)

        #expect(!session.escape())

        #expect(session.messages?.composer.reply == nil)
        #expect(session.split?.composer.reply != nil)
        session.close()
    }

    @Test
    func aReplyFromTheBarGoesToItsListsComposer() async throws {
        let session = session()
        let list = try #require(session.messages)
        let row = try #require(list.rows.first)
        let actions = session.messageActions(for: list) { _ in }

        actions.perform(.reply, on: row, shift: true)

        #expect(
            list.composer.reply
                == PendingReply(
                    messageId: id(1), authorName: "someone", mention: false, showsToggle: true))
        session.close()
    }

    @Test
    func theReplysRowIsNamedWhileItIsPending() async throws {
        let session = session()
        let list = try #require(session.messages)
        #expect(list.replyingTo == nil)

        try reply(in: list)
        #expect(list.replyingTo == id(1))

        list.composer.cancelReply()
        #expect(list.replyingTo == nil)
        session.close()
    }

    @Test
    func aDeletedTargetEndsTheReplyThroughTheSession() async throws {
        let session = session()
        try reply(in: session.messages)
        store.update { state in
            state.windows[id(11)] = window([2])
        }

        store.subscription.send(.messageDeleted(channelId: id(11), messageId: id(1)))
        await store.subscription.batches.pulled(2)

        #expect(session.messages?.composer.reply == nil)
        session.close()
    }
}
