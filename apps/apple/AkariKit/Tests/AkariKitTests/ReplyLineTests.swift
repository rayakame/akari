import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct ReplyLineContentTests {
    func line(_ target: ReplyTarget, mentions: Bool = false, name: String? = "Ray") -> ReplyLine {
        ReplyLine(
            channelId: id(10), messageId: id(5), target: target, mentionsAuthor: mentions,
            authorName: name)
    }

    @Test
    func aLoadedTargetShowsItsAuthorAndItsText() {
        let line = line(preview(content: "**hi** there"))

        #expect(line.name == "Ray")
        #expect(line.author?.id == id(2))
        guard case .text(let text, let edited) = line.content else {
            Issue.record("the text was expected")
            return
        }
        #expect(text.text == "**hi** there")
        #expect(!edited)
        #expect(line.leadingSymbol == nil && line.trailingSymbol == nil)
        #expect(line.jumps)
    }

    @Test
    func aReplyThatMentionsTheAuthorPrefixesTheName() {
        #expect(line(preview(), mentions: true).name == "@Ray")
    }

    @Test
    func anEditedTargetSaysSo() {
        guard case .text(_, let edited) = line(preview(edited: true)).content else {
            Issue.record("the text was expected")
            return
        }
        #expect(edited)
    }

    @Test
    func aDeletedTargetHasItsWordsAndNoJump() {
        let line = line(.deleted, name: nil)

        #expect(line.content == .placeholder("Original message was deleted"))
        #expect(line.name == nil && line.author == nil)
        #expect(!line.jumps)
    }

    @Test
    func aTargetThatIsNotLoadedHasItsWordsAndJumps() {
        let line = line(.notLoaded, name: nil)

        #expect(line.content == .placeholder("Message could not be loaded"))
        #expect(line.name == nil)
        #expect(line.jumps)
    }

    @Test
    func aTargetWithoutTextNamesWhatItHas() {
        let cases: [(ReplyTarget, String, String?)] = [
            (preview(content: "", sticker: true), "Click to see sticker", "face.smiling"),
            (preview(content: "", attachments: true, sticker: true), "Click to see sticker", "face.smiling"),
            (preview(content: "", kind: .chatInputCommand), "Click to see command", "terminal"),
            (preview(content: "", kind: .contextMenuCommand), "Click to see command", "terminal"),
            (preview(content: "", voice: true, attachments: true), "Click to see voice message", "mic.fill"),
            (preview(content: "", attachments: true), "Click to see attachment", "photo"),
            (preview(content: "", embeds: true), "Click to see attachment", "photo"),
            (preview(content: ""), "Click to see attachment", "photo"),
            (preview(content: "", kind: .channelPinnedMessage), "Message pinned.", nil),
            (preview(content: "", kind: .userJoin), "Click to see message", nil),
        ]
        for (target, words, symbol) in cases {
            let line = line(target)
            #expect(line.content == .placeholder(words))
            #expect(line.trailingSymbol == symbol, "\(words)")
        }
    }

    @Test
    func aTargetWithTextStillNamesItsMedia() {
        #expect(line(preview(sticker: true)).trailingSymbol == "face.smiling")
        #expect(line(preview(kind: .chatInputCommand)).trailingSymbol == "terminal")
        #expect(line(preview(voice: true)).trailingSymbol == "mic.fill")
        #expect(line(preview(attachments: true)).trailingSymbol == "photo")
        #expect(line(preview(embeds: true)).trailingSymbol == "photo")
        #expect(line(preview()).trailingSymbol == nil)
    }

    @Test
    func aForwardHasItsLeadingArrow() {
        #expect(line(preview(content: "", forwarded: true)).leadingSymbol == "arrowshape.turn.up.right")
        #expect(line(preview()).leadingSymbol == nil)
    }

    @Test
    func theLineReadsAsOneSentence() {
        #expect(line(preview(content: "hi"), mentions: true).accessibilityLabel == "Replying to Ray: hi")
        #expect(line(.deleted, name: nil).accessibilityLabel == "Original message was deleted")
    }
}

// Channel 10: messages 5 (Ray's) and 6...8; 7 answers 5, 8 answers 2, which isn't loaded.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ReplyLineTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let ray = user(2, name: "Ray")

    init() {
        account = FakeAccount(store: store)
        store.update { [ray] state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            state.show(
                message(5, by: ray), message(6, by: ray), reply(7, to: 5, mentionsAuthor: true),
                reply(8, to: 2))
            state.windows[id(10)] = window([5, 6, 7, 8])
            state.replyTargets[id(5)] = preview(by: ray, content: "message 5")
        }
    }

    func makeModel() -> MessageListModel {
        let model = MessageListModel(channelId: here, userId: id(1), account: account, store: store)
        store.log.forget()
        return model
    }

    func line(_ raw: UInt64, in model: MessageListModel) -> ReplyLine? {
        model.rows.first { $0.id == id(raw) }?.reply
    }

    @Test
    func aReplysRowHasItsLine() {
        let model = makeModel()

        #expect(line(5, in: model) == nil)
        #expect(
            line(7, in: model)
                == ReplyLine(
                    channelId: here, messageId: id(5), target: preview(by: ray, content: "message 5"),
                    mentionsAuthor: true, authorName: "Ray"))
        #expect(line(8, in: model)?.target == .notLoaded)
    }

    @Test
    func aContextMenuCommandsResultHasItsLineToo() {
        store.update { state in
            state.show(reply(7, to: 5, kind: .contextMenuCommand))
        }

        #expect(line(7, in: makeModel())?.messageId == id(5))
    }

    @Test
    func aChangedTargetReadsItsRepliesAgainAndNothingElse() {
        let model = makeModel()
        store.update { [ray] state in
            state.replyTargets[id(5)] = preview(by: ray, content: "edited", edited: true)
        }

        model.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(5))]))

        #expect(line(7, in: model)?.target == preview(by: ray, content: "edited", edited: true))
        #expect(
            store.log.reads.filter { if case .messages = $0 { true } else { false } }
                == [.messages(here, [id(7)])])
    }

    @Test
    func aTargetDeletedSinceShowsAsDeleted() {
        let model = makeModel()
        store.update { $0.replyTargets[id(5)] = .deleted }

        model.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(5))]))

        #expect(line(7, in: model)?.target == .deleted)
        #expect(line(7, in: model)?.authorName == nil)
    }

    @Test
    func aTargetALoadBroughtShows() {
        let model = makeModel()
        store.update { [ray] in $0.replyTargets[id(2)] = preview(by: ray, content: "old") }

        model.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(2))]))

        #expect(line(8, in: model)?.authorName == "Ray")
    }

    @Test
    func aTargetInAnotherChannelChangesNothingHere() {
        let model = makeModel()

        model.apply(EventBatch([.replyTargetChanged(channelId: id(11), targetId: id(5))]))

        #expect(store.log.reads.isEmpty)
    }

    @Test
    func aWebhooksTargetIsNamedAsItPosted() {
        store.update { state in
            state.replyTargets[id(5)] = preview(by: user(9, name: "Hook"), webhook: true)
        }

        #expect(line(7, in: makeModel())?.authorName == "Hook")
    }

    // A thread's first row shows its parent's message: a reply there answers in the parent.
    @Test
    func aStarterRowsOriginalHasItsLine() {
        store.update { [ray] state in
            state.channels[id(30)] = thread(30, in: 10)
            state.show(message(30, in: 30, by: ray, kind: .threadStarterMessage))
            state.windows[id(30)] = window([30], oldest: true)
            state.starters[id(30)] = .loaded(message: reply(7, to: 5, mentionsAuthor: true))
        }
        let model = MessageListModel(channelId: id(30), userId: id(1), account: account, store: store)

        let line = model.rows.first?.reply
        #expect(line?.channelId == here)
        #expect(line?.messageId == id(5))
        #expect(line?.name == "@Ray")

        store.update { $0.replyTargets[id(5)] = .deleted }
        model.apply(EventBatch([.replyTargetChanged(channelId: here, targetId: id(5))]))
        #expect(model.rows.first?.reply?.target == .deleted)
    }
}

// Messages 1...10 in channel 10; the list shows 5...8. 7 answers 6 and 8 answers 2.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct ReplyJumpTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            for raw in UInt64(1)...10 {
                state.show(message(raw, by: user(2)))
            }
            state.show(reply(7, to: 6), reply(8, to: 2))
            state.windows[id(10)] = window([5, 6, 7, 8])
            state.replyTargets[id(6)] = preview()
        }
        account.windowAfter.withLock {
            $0 = { load in
                switch load {
                case .around(let around, _):
                    let raw = around.rawValue
                    return window(Array(max(3, raw) - 2...min(10, raw + 2)), latest: raw + 2 >= 10)
                case .latest: return window([7, 8, 9, 10])
                default: return nil
                }
            }
        }
    }

    func makeModel() -> MessageListModel {
        let model = MessageListModel(channelId: here, userId: id(1), account: account, store: store)
        store.log.forget()
        return model
    }

    func row(_ raw: UInt64, in model: MessageListModel) -> MessageListModel.Row {
        model.rows.first { $0.id == id(raw) } ?? MessageListModel.Row(id: id(raw), message: message(raw))
    }

    var loads: [CallLog.Call] {
        store.log.actions.filter { if case .load = $0 { true } else { false } }
    }

    @Test
    func aLoadedTargetIsJumpedToWithoutALoadAndWithoutAWayBack() async {
        let model = makeModel()

        await model.jumpToTarget(of: row(7, in: model))

        #expect(loads.isEmpty)
        #expect(model.messageJump == .init(messageId: id(6), number: 1))
        #expect(model.replyReturn == nil, "the list is at the present")
    }

    @Test
    func anOlderTargetIsLoadedAroundAndTheReplyIsTheWayBack() async {
        let model = makeModel()

        await model.jumpToTarget(of: row(8, in: model))

        #expect(loads == [.load(here, .around(messageId: id(2), limit: 50))])
        #expect(model.messageJump == .init(messageId: id(2), number: 1))
        #expect(!model.atPresent)
        #expect(model.replyReturn == id(8))
    }

    @Test
    func theWayBackJumpsToTheReplyAndEnds() async {
        let model = makeModel()
        await model.jumpToTarget(of: row(8, in: model))

        await model.returnToReply()

        #expect(loads.last == .load(here, .around(messageId: id(8), limit: 50)))
        #expect(model.messageJump == .init(messageId: id(8), number: 2))
        #expect(model.replyReturn == nil)
    }

    @Test
    func theJumpToThePresentEndsTheWayBack() async {
        let model = makeModel()
        await model.jumpToTarget(of: row(8, in: model))

        await model.jumpToPresent()

        #expect(model.atPresent)
        #expect(model.replyReturn == nil)
    }

    @Test
    func anotherJumpEndsTheWayBack() async {
        let model = makeModel()
        await model.jumpToTarget(of: row(8, in: model))

        await model.jump(to: id(3))

        #expect(model.replyReturn == nil)
    }

    @Test
    func aFailedLoadLeavesNoWayBack() async {
        account.loadError.withLock { $0 = .Network(kind: .timeout) }
        let model = makeModel()

        await model.jumpToTarget(of: row(8, in: model))

        #expect(model.messageJump == nil)
        #expect(model.replyReturn == nil)
    }

    @Test
    func aJumpThatFailsAfterOneThatLandedThereLeavesNoWayBack() async {
        let model = makeModel()
        let reply = row(8, in: model)
        await model.jump(to: id(2))
        account.loadError.withLock { $0 = .Network(kind: .timeout) }
        store.update { [here] in $0.windows[here] = window([7, 8, 9], latest: false) }
        let loaded = StoreEvent.messagesLoaded(channelId: here, first: id(7), last: id(9))
        model.apply(EventBatch([loaded]))

        await model.jumpToTarget(of: reply)

        #expect(model.messageJump == .init(messageId: id(2), number: 1))
        #expect(model.replyReturn == nil)
    }

    @Test
    func aDeletedTargetIsNotJumpedTo() async {
        store.update { $0.replyTargets[id(6)] = .deleted }
        let model = makeModel()

        await model.jumpToTarget(of: row(7, in: model))

        #expect(model.messageJump == nil)
        #expect(loads.isEmpty)
    }

    @Test
    func aRowWithoutALineJumpsNowhere() async {
        let model = makeModel()

        await model.jumpToTarget(of: row(5, in: model))

        #expect(model.messageJump == nil)
    }

    @Test
    func aTargetInAnotherChannelIsTheSessions() async {
        store.update { $0.show(reply(7, to: 6, targetChannel: 11)) }
        let model = makeModel()
        var asked: [String] = []
        model.jumpElsewhere = { asked.append("\($0.rawValue) \($1.rawValue)") }

        await model.jumpToTarget(of: row(7, in: model))

        #expect(asked == ["11 6"])
        #expect(model.messageJump == nil)
    }

    @Test
    func withoutAWayBackReturningDoesNothing() async {
        let model = makeModel()

        await model.returnToReply()

        #expect(loads.isEmpty && model.messageJump == nil)
    }
}

// Channel 11 with thread 31 beside it, and channel 12.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionMessageJumpTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
            state.channels[id(31)] = thread(31, in: 11, name: "t1")
            state.windows[id(11)] = window([1, 2])
            state.windows[id(12)] = window([3])
            state.windows[id(31)] = window([31], oldest: true)
            state.show(
                message(1, in: 11), message(2, in: 11), message(3, in: 12),
                message(31, in: 31, kind: .threadStarterMessage))
            state.starters[id(31)] = .loaded(message: reply(2, to: 1, in: 11))
            state.replyTargets[id(1)] = preview()
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

    @Test
    func aStarterRowsReplyJumpsInTheParentBesideIt() async throws {
        let session = session()
        await session.openBeside(thread: id(31), from: id(11))
        let split = try #require(session.split)
        let row = try #require(split.rows.first)

        await split.jumpToTarget(of: row)
        await finished(session.jumping)

        #expect(session.messages?.messageJump == .init(messageId: id(1), number: 1))
        #expect(split.messageJump == nil)
        #expect(session.split != nil)
        session.close()
    }

    // The New Thread form shows its source message as the list does, the line included.
    @Test
    func aThreadFormsSourceHasItsLineAndFollowsItsTarget() async throws {
        store.update { state in
            state.show(reply(2, to: 1, in: 11, mentionsAuthor: true))
            state.threadCreations[id(11)] = ThreadCreation(public: true, private: true)
            state.startable = [id(2)]
        }
        let session = session()

        session.openThreadForm(in: id(11), from: id(2), location: .message)

        let form = try #require(session.threadForm)
        #expect(form.sourceReply?.messageId == id(1))
        #expect(form.sourceReply?.name == "@Ray")

        store.update { $0.replyTargets[id(1)] = .deleted }
        session.apply(EventBatch([.replyTargetChanged(channelId: id(11), targetId: id(1))]))
        #expect(form.sourceReply?.target == .deleted)
        session.close()
    }

    @Test
    func aMessageInAChannelThatIsNotOpenOpensItAndJumps() async throws {
        let session = session()

        session.jump(to: id(3), in: id(12))
        await session.messages?.open()

        #expect(session.messages?.channelId == id(12))
        #expect(session.messages?.messageJump == .init(messageId: id(3), number: 1))
        session.close()
    }
}
