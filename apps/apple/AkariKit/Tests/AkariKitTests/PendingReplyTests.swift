import Foundation
import Observation
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
    func aRowOfAnotherChannelStartsNothing() {
        let composer = composer()
        store.update { $0.allow(replying(), message: 300, in: 11) }

        composer.startReply(to: row(300, in: 11), shift: false)

        #expect(composer.reply == nil)
        #expect(composer.focusRequests == 0)
        #expect(account.replyHolds.current.isEmpty)
    }

    @Test
    func theBarNamesTheAuthorAsTheRowDoes() {
        let composer = composer()
        let row = MessageListModel.Row(
            id: id(100), message: message(100, by: user(2, name: "Ray")), authorName: "Nick")

        composer.startReply(to: row, shift: false)

        #expect(composer.reply?.authorName == "Nick")
    }

    @Test
    func aNewReplyClearsTheGoneNoticeAndNoOther() async {
        let composer = composer()
        composer.startReply(to: row(100), shift: false)
        composer.draft = "hi"
        account.queueError.withLock { $0 = .InvalidRequest }
        await composer.submit()
        #expect(composer.problem == .replyTargetGone)

        composer.startReply(to: row(102), shift: false)
        #expect(composer.problem == nil)

        account.queueError.withLock { $0 = .Closed }
        await composer.submit()
        #expect(composer.problem == .failed(.Closed))

        composer.startReply(to: row(100), shift: false)
        #expect(composer.problem == .failed(.Closed))
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

        store.update { $0.answer(.deleted, message: 100, in: 10) }
        composer.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(100))]))
        #expect(composer.reply == nil)
    }
}

// Channel 10 with messages 1...60, the list at its newest; 58 can be answered. A load around
// 5 leaves 3...7, the latest page 57...60.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ReplyHoldTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let drafts = Drafts()

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.channels[id(20)] = dm(20, with: 2)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            for raw in UInt64(1)...60 {
                state.show(message(raw, by: user(2)))
            }
            state.windows[id(10)] = window([57, 58, 59, 60])
            state.allow(replying(), message: 58, in: 10)
            state.allow(replying(), message: 59, in: 10)
            state.allow(replying(toggle: false), message: 200, in: 20)
        }
        account.windowAfter.withLock {
            $0 = { load in
                switch load {
                case .around: return window([3, 4, 5, 6, 7], latest: false)
                case .latest: return window([57, 58, 59, 60])
                default: return nil
                }
            }
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(
            channelId: here, userId: id(1), account: account, store: store, drafts: drafts)
    }

    func answer(_ raw: UInt64, in model: MessageListModel) throws {
        let row = try #require(model.rows.first { $0.id == id(raw) })
        model.composer.startReply(to: row, shift: false)
        #expect(model.composer.reply?.messageId == id(raw))
    }

    var replies: [CallLog.Call] {
        store.log.actions.filter { call in
            if case .reply = call { true } else { false }
        }
    }

    func hold(_ raw: UInt64?, in channel: UInt64 = 10) -> FakeAccount.ReplyHold {
        FakeAccount.ReplyHold(channel: id(channel), message: raw.map { id($0) })
    }

    @Test
    func aReplyIsSentAfterAJumpTookItsMessageOutOfTheList() async throws {
        let model = makeModel()
        try answer(58, in: model)
        await model.jump(to: id(5))
        #expect(model.rows.map(\.id) == [3, 4, 5, 6, 7].map { id($0) })
        #expect(model.composer.reply?.messageId == id(58), "the bar stays")

        model.composer.draft = "hi"
        await model.composer.submit()

        #expect(replies == [.reply(here, id(58), mention: true)])
        #expect(model.composer.problem == nil)
        #expect(model.composer.reply == nil)
        #expect(model.composer.draft.isEmpty)
    }

    @Test
    func aReplyIsSentAfterTheJumpToThePresentReloadedTheList() async throws {
        let model = makeModel()
        try answer(58, in: model)
        await model.jump(to: id(5))
        account.windowAfter.withLock { $0 = { _ in window([59, 60]) } }

        await model.jumpToPresent()
        model.composer.draft = "hi"
        await model.composer.submit()

        #expect(replies == [.reply(here, id(58), mention: true)])
        #expect(model.composer.problem == nil)
    }

    @Test
    func aReplyKeptAcrossAChannelSwitchIsStillHeld() async throws {
        let first = makeModel()
        try answer(58, in: first)
        await first.jump(to: id(5))

        let again = makeModel()
        #expect(again.composer.reply?.messageId == id(58))
        again.composer.draft = "hi"
        await again.composer.submit()

        #expect(account.replyHolds.current == [hold(58)], "held once, by the core for the session")
        #expect(replies == [.reply(here, id(58), mention: true)])
    }

    @Test
    func startingAReplyHoldsItsMessageAndAnotherReplacesIt() throws {
        let model = makeModel()

        try answer(58, in: model)
        try answer(59, in: model)

        #expect(account.replyHolds.current == [hold(58), hold(59)])
        #expect(account.heldReplies.current == [here: id(59)])
    }

    @Test
    func theSwitchHoldsNothingAgain() throws {
        let model = makeModel()
        try answer(58, in: model)

        model.composer.toggleReplyMention()

        #expect(account.replyHolds.current == [hold(58)])
    }

    @Test
    func cancellingReleasesTheHold() throws {
        let model = makeModel()
        try answer(58, in: model)

        model.composer.cancelReply()
        model.composer.cancelReply()

        #expect(account.replyHolds.current == [hold(58), hold(nil)])
        #expect(account.heldReplies.current.isEmpty)
    }

    @Test
    func aSendLeavesTheReleaseToTheCore() async throws {
        let model = makeModel()
        try answer(58, in: model)
        model.composer.draft = "hi"

        await model.composer.submit()

        #expect(account.replyHolds.current == [hold(58)])
        #expect(account.heldReplies.current.isEmpty)
    }

    @Test
    func theMessagesDeletionReleasesTheHold() throws {
        let model = makeModel()
        try answer(58, in: model)

        model.composer.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(58))]))

        #expect(model.composer.reply == nil)
        #expect(account.replyHolds.current == [hold(58), hold(nil)])
    }

    @Test
    func aRefusedReplyReleasesTheHold() async throws {
        let model = makeModel()
        try answer(58, in: model)
        model.composer.draft = "hi"
        account.queueError.withLock { $0 = .InvalidRequest }

        await model.composer.submit()

        #expect(model.composer.problem == .replyTargetGone)
        #expect(account.replyHolds.current == [hold(58), hold(nil)])
    }

    @Test
    func theBarsNameFollowsARename() throws {
        let model = makeModel()
        try answer(58, in: model)
        #expect(model.composer.reply?.authorName == "someone")
        store.update { $0.show(message(58, by: user(2, name: "Renamed"))) }

        model.apply(EventBatch([.userUpdated(userId: id(2))]))

        #expect(model.composer.reply?.authorName == "Renamed")
        #expect(drafts.replies[here]?.authorName == "Renamed")
        #expect(account.replyHolds.current == [hold(58)], "the same reply")
    }

    @Test
    func theBarsNameFollowsANewSession() throws {
        let model = makeModel()
        try answer(58, in: model)
        store.update { $0.show(message(58, by: user(2, name: "Renamed"))) }

        model.apply(EventBatch([.ready]))

        #expect(model.composer.reply?.authorName == "Renamed")
    }

    @Test
    func theBarsNameFollowsAMessageThatLeftTheList() async throws {
        let model = makeModel()
        try answer(58, in: model)
        await model.jump(to: id(5))
        store.update { $0.answer(preview(by: user(2, name: "Renamed")), message: 58, in: 10) }

        model.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(58))]))

        #expect(model.composer.reply?.authorName == "Renamed")
    }

    @Test
    func anotherUsersRenameLeavesTheBarsName() throws {
        let model = makeModel()
        try answer(58, in: model)

        let changed = Locked(false)
        withObservationTracking {
            _ = model.composer.reply
        } onChange: {
            changed.withLock { $0 = true }
        }

        model.apply(EventBatch([.userUpdated(userId: id(7))]))

        #expect(model.composer.reply?.authorName == "someone")
        #expect(!changed.current, "the bar isn't drawn again for the same name")
    }

    // Pending row 900 answers 58, which was deleted after the send failed.
    func failedReply(with error: RequestError) async throws -> MessageListModel {
        let model = makeModel()
        try answer(58, in: model)
        store.update { [here] state in
            state.show(reply(900, to: 58, delivery: .failed))
            state.windows[here] = window([57, 58, 59, 60], pending: [900])
        }
        account.sendError.withLock { $0 = error }
        model.composer.draft = "hi"
        await model.composer.submit()
        store.update { $0.answer(.deleted, message: 58, in: 10) }
        model.apply(
            EventBatch([
                .messageInserted(channelId: here, messageId: id(900)),
                .replyTargetChanged(channelId: here, targetId: id(58)),
            ]))
        return model
    }

    func refused(_ model: MessageListModel) -> Bool? {
        model.rows.first { $0.id == id(900) }?.replyRefused
    }

    @Test
    func aReplyThatFailedForAnotherReasonIsNotRefusedForItsDeletedMessage() async throws {
        let model = try await failedReply(with: .Network(kind: .timeout))

        #expect(model.rows.first { $0.id == id(900) }?.reply?.target == .deleted)
        #expect(refused(model) == false)
    }

    @Test
    func aReplyDiscordRefusedForItsDeletedMessageSaysSoUntilItIsRetried() async throws {
        let model = try await failedReply(with: .ReplyTargetDeleted)
        #expect(refused(model) == true)
        #expect(makeModel().rows.first { $0.id == id(900) }?.replyRefused == true, "kept for the session")

        account.sendError.withLock { $0 = .Network(kind: .timeout) }
        await model.composer.retry(id(900))
        #expect(refused(model) == false)

        account.sendError.withLock { $0 = .ReplyTargetDeleted }
        await model.composer.retry(id(900))
        #expect(refused(model) == true)

        model.composer.discard(id(900))
        #expect(makeModel().rows.first { $0.id == id(900) }?.replyRefused == false)
    }

    @Test
    func aRetryThatGoesThroughForgetsTheRefusal() async throws {
        let model = try await failedReply(with: .ReplyTargetDeleted)
        #expect(model.composer.replyWasRefused(id(900)))

        account.sendError.withLock { $0 = nil }
        await model.composer.retry(id(900))

        #expect(!model.composer.replyWasRefused(id(900)))
        #expect(model.composer.problem == nil)
    }

    @Test
    func aRetryThatFailsAsBeforeReadsNoRowAgain() async throws {
        let model = try await failedReply(with: .Network(kind: .timeout))
        store.log.forget()

        await model.composer.retry(id(900))

        #expect(refused(model) == false)
        #expect(store.log.reads.allSatisfy { if case .messages = $0 { false } else { true } })
    }

    @Test
    func aMessageThatIsNeitherLoadedNorHeldIsStillRefused() async throws {
        let model = makeModel()
        try answer(58, in: model)
        await model.jump(to: id(5))
        account.heldReplies.withLock { $0 = [:] }

        model.composer.draft = "hi"
        await model.composer.submit()

        #expect(replies.isEmpty)
        #expect(model.composer.problem == .replyTargetGone)
        #expect(model.composer.draft == "hi")
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
