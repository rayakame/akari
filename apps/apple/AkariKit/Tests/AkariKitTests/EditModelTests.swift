import Foundation
import Testing

@testable import AkariKit

func editing(delete: Bool = true) -> MessageActions {
    allowed(edit: true, delete: delete)
}

// Channel 10 in guild 1: Ray's 100, the user's 101 and 102, and 103 with only a picture;
// channel 20 beside it.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class EditModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let drafts = Drafts()
    let here: ChannelId = id(10)
    var deleteAsked: [MessageId] = []

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10, guild: 1)
            state.channels[id(20)] = channel(20, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            var picture = message(103, content: "")
            picture.attachments = [
                Attachment(
                    id: id(1), filename: "a.png", title: nil, description: nil, contentType: nil,
                    size: 1, kind: .image, width: 4, height: 4, placeholder: nil, spoiler: false,
                    animated: false,
                    key: .attachment(channelId: id(10), messageId: id(103), attachmentId: id(1)),
                    display: .media, cover: nil)
            ]
            state.show(
                message(100, by: user(2, name: "Ray"), content: "hello"),
                message(101, content: "my first"), message(102, content: "my second"), picture)
            state.windows[id(10)] = window([100, 101, 102, 103])
            state.allow(allowed(), message: 100, in: 10)
            state.allow(editing(), message: 101, in: 10)
            state.allow(editing(), message: 102, in: 10)
            state.allow(editing(), message: 103, in: 10)
        }
    }

    func list(_ channel: UInt64 = 10) -> MessageListModel {
        let list = MessageListModel(
            channelId: id(channel), userId: id(1), account: account, store: store, drafts: drafts)
        list.editor.askDelete = { [unowned self] row in deleteAsked.append(row.message.id) }
        return list
    }

    func row(_ raw: UInt64, in list: MessageListModel) throws -> MessageListModel.Row {
        try #require(list.rows.first { $0.message.id == id(raw) })
    }

    var edits: [CallLog.Call] {
        store.log.actions.filter { call in
            switch call {
            case .edit, .dismissEdit, .delete: true
            default: false
            }
        }
    }

    func updated(_ raw: UInt64, in channel: UInt64 = 10) -> EventBatch {
        EventBatch([.messageUpdated(channelId: id(channel), messageId: id(raw))])
    }

    @Test
    func anEditStartsWithTheMessagesTextAndNoProblem() throws {
        let list = list()

        list.editor.start(try row(101, in: list))

        #expect(list.editor.editing?.messageId == id(101))
        #expect(list.editor.editing?.problem == nil)
        #expect(list.editor.text == "my first")
    }

    @Test
    func aMessageTheUserMayNotEditStartsNothing() throws {
        let list = list()

        list.editor.start(try row(100, in: list))

        #expect(list.editor.editing == nil)
        #expect(list.editor.text == "")
    }

    @Test
    func anotherEditInTheChannelReplacesTheFirstAndItsText() throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "typed"
        let first = try #require(list.editor.editing?.opening)

        list.editor.start(try row(102, in: list))

        #expect(list.editor.editing?.messageId == id(102))
        #expect(list.editor.text == "my second")
        #expect(try #require(list.editor.editing?.opening) > first, "a new opening fills the editor")
    }

    @Test
    func theEditAndItsTextAreKeptPerChannelAcrossAChannelSwitch() throws {
        let first = list()
        first.editor.start(try row(101, in: first))
        first.editor.text = "half typed"

        #expect(list(20).editor.editing == nil)
        let back = list()
        #expect(back.editor.editing?.messageId == id(101))
        #expect(back.editor.text == "half typed")
        #expect(back.editor.editing?.opening != first.editor.editing?.opening, "it opens again")
    }

    @Test
    func cancellingEndsItLeavesTheMessageAndGivesTheComposerTheFocus() throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        let focus = list.composer.focusRequests

        list.editor.cancel()

        #expect(list.editor.editing == nil)
        #expect(self.list().editor.editing == nil, "gone for the channel")
        #expect(list.composer.focusRequests == focus + 1)
        #expect(edits.isEmpty)
    }

    @Test(arguments: ["my first", "  my first\n"])
    func anUnchangedSaveSendsNothingAndCloses(text: String) async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = text

        list.editor.save()
        await finished(list.editor.saving)

        #expect(list.editor.editing == nil)
        #expect(edits.isEmpty)
        #expect(deleteAsked.isEmpty)
    }

    @Test
    func anEmptiedSaveAsksToDeleteTheMessageAndCloses() throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = " \n "

        list.editor.save()

        #expect(list.editor.editing == nil)
        #expect(deleteAsked == [id(101)])
        #expect(edits.isEmpty)
    }

    @Test
    func anEmptiedSaveOfAMessageWithAPictureSavesNoText() async throws {
        store.update { $0.messages[id(103)]?.content = "caption" }
        let list = list()
        list.editor.start(try row(103, in: list))
        list.editor.text = ""

        list.editor.save()
        await finished(list.editor.saving)

        #expect(edits == [.edit(here, id(103), "")])
        #expect(deleteAsked.isEmpty)
    }

    @Test
    func aSaveTooLongStaysOpenWithTheReasonAndSendsNothing() throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        let opening = list.editor.editing?.opening
        list.editor.text = String(repeating: "a", count: 2001)

        list.editor.save()

        #expect(list.editor.editing?.problem == .tooLong(by: 1, limit: 2000))
        #expect(list.editor.editing?.opening == opening, "the text stays as typed")
        #expect(list.editor.text.count == 2001)
        #expect(edits.isEmpty)
        list.editor.text = String(repeating: "a", count: 2002)
        #expect(list.editor.editing?.problem == .tooLong(by: 1, limit: 2000), "until the next save")
        list.editor.text = String(repeating: "a", count: 2000)
        #expect(list.editor.editing?.problem == nil, "gone once it fits")
        #expect(list.editor.editing?.opening == opening)
        #expect(
            EditProblem.tooLong(by: 1, limit: 2000).text(locale: Locale(identifier: "en_US"))
                == "This message is 1 character too long. The limit is 2,000.")
    }

    @Test
    func nitrosLimitIsTheCoresAndFollowsTheUser() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = String(repeating: "a", count: 2001)
        store.update { $0.lengthLimit = 4000 }
        list.apply(EventBatch([.currentUserUpdated]))

        list.editor.save()

        #expect(list.editor.editing == nil)
        await finished(list.editor.saving)
        #expect(edits.count == 1)
    }

    @Test
    func theCounterShowsFromATenthOfTheLimit() throws {
        let list = list()
        list.editor.start(try row(101, in: list))

        list.editor.text = String(repeating: "a", count: 1799)
        #expect(list.editor.remaining == nil)
        list.editor.text = String(repeating: "a", count: 1800)
        #expect(list.editor.remaining == 200)
        list.editor.text = String(repeating: "a", count: 2001)
        #expect(list.editor.remaining == -1)

        store.update { $0.lengthLimit = 4000 }
        list.apply(EventBatch([.currentUserUpdated]))
        list.editor.text = String(repeating: "a", count: 3599)
        #expect(list.editor.remaining == nil)
        list.editor.text = String(repeating: "a", count: 3600)
        #expect(list.editor.remaining == 400)
    }

    @Test
    func aSaveClosesTheEditorAtOnceAndSendsTheTrimmedTextWithTheRowsIds() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "  my first, edited \n"
        account.holdEdits.withLock { $0 = true }

        list.editor.save()

        #expect(list.editor.editing == nil)
        try await eventually { self.edits == [.edit(self.here, id(101), "my first, edited")] }
        account.editReplies.send(())
        try await eventually { self.store.messages(channelId: self.here, ids: [id(101)]).first?.edit == nil }
        list.apply(updated(101))
        #expect(list.editor.editing == nil, "nothing opens after Discord took it")
    }

    @Test
    func aRefusedSaveOpensAgainWithTheSavedTextAndWhyOnce() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "rude words"
        account.editError.withLock { $0 = .MessageBlocked(reason: "Blocked by AutoMod.") }

        list.editor.save()
        await finished(list.editor.saving)
        list.apply(updated(101))

        #expect(list.editor.editing?.messageId == id(101))
        #expect(list.editor.editing?.problem == .refused(.MessageBlocked(reason: "Blocked by AutoMod.")))
        #expect(list.editor.text == "rude words")
        #expect(
            edits == [.edit(here, id(101), "rude words"), .dismissEdit(here, id(101))],
            "the core forgets it once the editor has it")
    }

    @Test
    func theMessagesRefusalOpensTheEditorAndTheAnswerAfterItDoesNothing() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "rude words"
        account.editError.withLock { $0 = .Forbidden }
        account.holdEdits.withLock { $0 = true }
        list.editor.save()
        try await eventually { !self.edits.isEmpty }

        // The core's event comes before its answer.
        store.update { state in
            state.messages[id(101)]?.edit = .failed(content: "rude words", error: .Forbidden)
        }
        list.apply(updated(101))
        let opening = try #require(list.editor.editing?.opening)
        list.editor.text = "kinder words"
        account.editReplies.send(())
        await finished(list.editor.saving)

        #expect(list.editor.editing?.opening == opening, "opened once")
        #expect(list.editor.text == "kinder words")
        #expect(list.editor.editing?.problem == .refused(.Forbidden))
        #expect(edits.filter { $0 == .dismissEdit(here, id(101)) }.count == 1)
    }

    @Test
    func aRefusalGivenUpBeforeDiscordsAnswerStaysGivenUp() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "rude words"
        account.editError.withLock { $0 = .Forbidden }
        account.holdEdits.withLock { $0 = true }
        list.editor.save()
        try await eventually { !self.edits.isEmpty }
        store.update { state in
            state.messages[id(101)]?.edit = .failed(content: "rude words", error: .Forbidden)
        }
        list.apply(updated(101))
        #expect(list.editor.editing != nil)

        list.editor.cancel()
        account.editReplies.send(())
        await finished(list.editor.saving)

        #expect(list.editor.editing == nil)
    }

    @Test
    func aChannelOpenedAgainKeepsItsEditOverARefusal() throws {
        let first = list()
        first.editor.start(try row(101, in: first))
        first.editor.text = "half typed"
        store.update { state in
            state.messages[id(102)]?.edit = .failed(content: "saved", error: .Forbidden)
        }

        let back = list()

        #expect(back.editor.editing?.messageId == id(101))
        #expect(back.editor.text == "half typed")
        #expect(!edits.contains(.dismissEdit(here, id(102))))
    }

    @Test
    func theAnswerOpensTheEditorAndTheMessagesRefusalAfterItDoesNothing() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "rude words"
        account.editError.withLock { $0 = .Forbidden }

        list.editor.save()
        await finished(list.editor.saving)
        let opening = try #require(list.editor.editing?.opening)
        list.apply(updated(101))

        #expect(list.editor.editing?.opening == opening)
        #expect(list.editor.text == "rude words")
        #expect(edits == [.edit(here, id(101), "rude words"), .dismissEdit(here, id(101))])
    }

    @Test
    func aRefusalBeforeAskingOpensTheEditorWithTheThrownReason() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "new"
        account.editRefusedEarly.withLock { $0 = .Request(error: .InvalidRequest) }

        list.editor.save()
        await finished(list.editor.saving)

        #expect(list.editor.editing?.problem == .refused(.Request(error: .InvalidRequest)))
        #expect(list.editor.text == "new")
    }

    @Test
    func aRefusalWaitsWhileAnotherEditIsOpenAndOpensWhenItEnds() throws {
        let list = list()
        list.editor.start(try row(102, in: list))
        store.update { state in
            state.messages[id(101)]?.edit = .failed(content: "saved", error: .Forbidden)
        }
        list.apply(updated(101))
        #expect(list.editor.editing?.messageId == id(102))

        list.editor.cancel()

        #expect(list.editor.editing?.messageId == id(101))
        #expect(list.editor.text == "saved")
    }

    @Test
    func aRefusalOpensTheEditorWhenTheChannelOpensAgain() {
        store.update { state in
            state.messages[id(102)]?.edit = .failed(content: "saved", error: .Forbidden)
        }

        let list = list()

        #expect(list.editor.editing?.messageId == id(102))
        #expect(list.editor.editing?.problem == .refused(.Forbidden))
        #expect(edits == [.dismissEdit(here, id(102))])
    }

    @Test
    func anEditOfARefusedMessageTakesTheSavedText() throws {
        let list = list()
        list.editor.start(try row(102, in: list))
        store.update { state in
            state.messages[id(101)]?.edit = .failed(content: "saved", error: .Forbidden)
        }
        list.apply(updated(101))

        list.editor.start(try row(101, in: list))

        #expect(list.editor.text == "saved")
        #expect(list.editor.editing?.problem == .refused(.Forbidden))
    }

    @Test
    func theEditEndsWhenItsMessageIsDeletedAlsoInABulk() throws {
        let list = list()
        list.editor.start(try row(101, in: list))

        list.apply(
            EventBatch([
                .messageDeleted(channelId: here, messageId: id(100)),
                .messageDeleted(channelId: here, messageId: id(101)),
            ]))

        #expect(list.editor.editing == nil)
        #expect(self.list().editor.editing == nil)
    }

    @Test
    func anotherMessagesDeleteLeavesTheEdit() throws {
        let list = list()
        list.editor.start(try row(101, in: list))

        list.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(100))]))

        #expect(list.editor.editing?.messageId == id(101))
    }

    @Test(arguments: [EditError.Deleted, .Request(error: .Network(kind: .timeout))])
    func aSaveAnsweredAfterItsMessageWentOpensNothing(error: EditError) async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "new"
        account.editError.withLock { $0 = error }
        account.holdEdits.withLock { $0 = true }
        list.editor.save()
        try await eventually { !self.edits.isEmpty }

        store.update { state in
            state.messages[id(101)] = nil
            state.windows[id(10)] = window([100, 102, 103])
        }
        list.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(101))]))
        account.editReplies.send(())
        await finished(list.editor.saving)

        #expect(list.editor.editing == nil)
    }

    // Nothing is left to save to; the row's own deletion says the rest.
    @Test
    func aRefusalForADeletedMessageOpensNothingWhileItsRowStillShows() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "new"
        account.editError.withLock { $0 = .Deleted }

        list.editor.save()
        await finished(list.editor.saving)
        list.apply(updated(101))

        #expect(list.editor.editing == nil)
        list.editor.start(try row(101, in: list))
        #expect(list.editor.text == "my first")
        #expect(list.editor.editing?.problem == nil)
    }

    @Test
    func aKeptEditWhoseMessageWentWhileTheChannelWasClosedIsGone() throws {
        let first = list()
        first.editor.start(try row(101, in: first))
        first.editor.text = "half typed"
        store.update { state in
            state.messages[id(101)] = nil
            state.windows[id(10)] = window([100, 102, 103])
        }

        let back = list()

        #expect(back.editor.editing == nil)
        #expect(back.composer.focusRequests == 0)
        #expect(self.list().editor.editing == nil, "gone for the channel")
    }

    @Test
    func aKeptEditOfTheNewestMessageThatWentIsGoneToo() throws {
        let first = list()
        first.editor.start(try row(103, in: first))
        store.update { state in
            state.messages[id(103)] = nil
            state.windows[id(10)] = window([100, 101, 102])
        }

        #expect(list().editor.editing == nil)
    }

    @Test
    func aKeptEditOfAMessageBeyondTheLoadedRowsStays() throws {
        let first = list()
        first.editor.start(try row(101, in: first))
        first.editor.text = "half typed"
        store.update { $0.windows[id(10)] = window([102, 103], latest: false) }

        let back = list()

        #expect(back.editor.editing?.messageId == id(101))
        #expect(back.editor.text == "half typed")
    }

    @Test
    func aKeptEditWhoseChannelLoadsWithoutItsMessageEnds() throws {
        let first = list()
        first.editor.start(try row(101, in: first))
        store.update { $0.windows[id(10)] = nil }
        let back = list()
        #expect(back.editor.editing?.messageId == id(101), "nothing to tell yet")

        store.update { state in
            state.messages[id(101)] = nil
            state.windows[id(10)] = window([100, 102, 103])
        }
        back.apply(EventBatch([.messagesLoaded(channelId: here, first: id(100), last: id(103))]))

        #expect(back.editor.editing == nil)
        #expect(back.composer.focusRequests == 0)
    }

    @Test
    func anEditItsMessagesDeletionEndsAsksTheFocusOnlyWhenTheEditorHadIt() throws {
        let list = list()
        list.editor.start(try row(101, in: list))

        list.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(101))]))
        #expect(list.editor.editing == nil)
        #expect(list.composer.focusRequests == 0, "the user may be typing elsewhere")

        list.editor.start(try row(102, in: list))
        list.editor.editorFocused = true
        list.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(102))]))
        #expect(list.composer.focusRequests == 1)
    }

    @Test
    func aStarterRowsEditEndsWhenTheMessageItShowsGoes() throws {
        store.update { state in
            state.channels[id(31)] = thread(31, in: 10)
            state.show(message(31, in: 31, kind: .threadStarterMessage))
            state.windows[id(31)] = window([31], oldest: true)
            state.starters[id(31)] = .loaded(message: message(7, in: 10, content: "the start"))
            state.allow(editing(), message: 31, in: 31)
        }
        let list = list(31)
        list.editor.start(try row(31, in: list))
        #expect(list.editor.editing?.messageId == id(31))

        store.update { $0.starters[id(31)] = .unavailable }
        list.apply(updated(31, in: 31))

        #expect(list.editor.editing == nil)
    }

    @Test
    func aRefusalBeforeAskingWaitsWhileAnotherEditIsOpenAndOpensWhenItEnds() async throws {
        let list = list()
        list.editor.start(try row(101, in: list))
        list.editor.text = "new"
        account.editRefusedEarly.withLock { $0 = .Request(error: .InvalidRequest) }
        list.editor.save()
        list.editor.start(try row(102, in: list))

        await finished(list.editor.saving)
        #expect(list.editor.editing?.messageId == id(102))
        list.editor.cancel()

        #expect(list.editor.editing?.messageId == id(101))
        #expect(list.editor.text == "new")
        #expect(list.editor.editing?.problem == .refused(.Request(error: .InvalidRequest)))
    }

    // The earlier save's answer, after the later save started, isn't the later one's.
    @Test
    func aRefusalBeforeAskingAfterAnEarlierSavesAnswerStillOpensTheEditor() async throws {
        let list = list()
        account.holdEdits.withLock { $0 = true }
        list.editor.start(try row(101, in: list))
        list.editor.text = "first"
        list.editor.save()
        let first = list.editor.saving
        await account.editReplies.pulled(1)
        list.editor.start(try row(101, in: list))
        list.editor.text = "second"
        account.editRefusedEarly.withLock { $0 = .Request(error: .InvalidRequest) }
        list.editor.save()
        await account.editReplies.pulled(2)

        account.editReplies.send(())
        await finished(first)
        account.editReplies.send(())
        await finished(list.editor.saving)

        #expect(list.editor.editing?.messageId == id(101))
        #expect(list.editor.text == "second")
        #expect(list.editor.editing?.problem == .refused(.Request(error: .InvalidRequest)))
    }

    @Test
    func upArrowEditsTheNewestLoadedMessageTheUserMayEdit() {
        let list = list()

        #expect(list.editNewest())
        #expect(list.editor.editing?.messageId == id(103))

        store.update { $0.allow(allowed(), message: 103, in: 10) }
        #expect(list.editNewest())
        #expect(list.editor.editing?.messageId == id(102))
    }

    @Test
    func upArrowWithNothingToEditDoesNothing() {
        store.update { state in
            for raw: UInt64 in [101, 102, 103] {
                state.allow(allowed(), message: raw, in: 10)
            }
        }
        let list = list()

        #expect(!list.editNewest())
        #expect(list.editor.editing == nil)
    }

    @Test
    func aStarterRowsRefusalIsForgottenByTheMessageItShows() {
        store.update { state in
            state.channels[id(31)] = thread(31, in: 10)
            state.show(message(31, in: 31, kind: .threadStarterMessage))
            state.windows[id(31)] = window([31], oldest: true)
            var original = message(7, in: 10, content: "the start")
            original.edit = .failed(content: "the new start", error: .Forbidden)
            state.starters[id(31)] = .loaded(message: original)
            state.allow(editing(), message: 31, in: 31)
        }

        let list = list(31)

        #expect(list.editor.editing?.messageId == id(31))
        #expect(list.editor.text == "the new start")
        #expect(edits == [.dismissEdit(here, id(7))])
    }

    @Test
    func escapeEndsTheEditBeforeAPendingReplyAndMarkingRead() async throws {
        store.update { $0.allow(replying(edit: true), message: 100, in: 10) }
        let list = list()
        list.composer.startReply(to: try row(100, in: list), shift: false)
        list.editor.start(try row(101, in: list))

        await list.escape()
        #expect(list.editor.editing == nil)
        #expect(list.composer.reply != nil)

        await list.escape()
        #expect(list.composer.reply == nil)
        #expect(account.markedRead.current.isEmpty)

        await list.escape()
        #expect(account.markedRead.current == [here])
    }
}

@Suite
struct EditProblemTests {
    private func text(_ problem: EditProblem) -> String {
        problem.text(locale: Locale(identifier: "en_US"))
    }

    @Test
    func autoModsReasonIsDiscordsAndItsOwnWordsWithoutOne() {
        #expect(text(.refused(.MessageBlocked(reason: "Blocked words."))) == "Blocked words.")
        #expect(
            text(.refused(.MessageBlocked(reason: "")))
                == "This message can't be edited because it contains content blocked by this "
                + "community. The message may also be viewed by the community owners.")
    }

    @Test
    func theEditLimitSaysTheMinutesRoundedUp() {
        let start = "To keep announcements speedy, published messages can only be edited 3 times "
            + "per hour. Please try again in "
        #expect(text(.refused(.TooManyEdits(retryAfter: 61))) == start + "2 minutes.")
        #expect(text(.refused(.TooManyEdits(retryAfter: 30))) == start + "1 minute.")
        #expect(text(.refused(.TooManyEdits(retryAfter: nil))) == start + "1 minute.")
    }

    @Test
    func theOtherRefusalsSayWhy() {
        #expect(text(.refused(.Forbidden)) == "You don't have permission to edit this message.")
        #expect(text(.refused(.Deleted)) == "This message was deleted.")
        #expect(text(.refused(.RateLimited(retryAfter: 5))) == "You're editing too quickly. Try again in 5 seconds.")
        #expect(text(.refused(.RateLimited(retryAfter: nil))) == "You're editing too quickly. Try again in a moment.")
        #expect(
            text(.refused(.Request(error: .Network(kind: .timeout))))
                == "Akari couldn't reach Discord. Check your connection, then save again.")
        #expect(text(.refused(.Request(error: .ServerError(status: 502)))) == "Discord had a problem. Try again in a moment.")
        #expect(
            text(.refused(.Request(error: .Discord(status: 400, code: 50035, message: "Invalid"))))
                == "Discord refused this edit as too long or malformed.")
        #expect(text(.refused(.Request(error: .TooLong(limit: 2000)))) == "This message is longer than 2,000 characters.")
        #expect(text(.refused(.Request(error: .Closed))) == "Akari isn't connected. Reconnect to edit messages.")
    }
}
