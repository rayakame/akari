import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1 (messages 1 and 2, both can start a thread) with thread 31 under it, and
// channel 12. The user may create both kinds of thread in 11 and is there, online.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ThreadFormTests {
    let store = FakeStore()
    let account: FakeAccount
    let memory = AccountMemory(defaults: MemoryDefaults())

    init() {
        account = FakeAccount(store: store)
    }

    var calls: [CallLog.Call] { store.log.calls }

    var creates: [CallLog.Call] {
        calls.filter {
            if case .createThread = $0 { return true }
            return false
        }
    }

    func inChannel11(
        creation: ThreadCreation = ThreadCreation(public: true, private: true),
        sourceContent: String = "das ist ein ziemlich langer satz zum testen von threads"
    ) -> SessionModel {
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
            state.channels[id(31)] = thread(31, in: 11, name: "t1")
            state.windows[id(11)] = window([1, 2])
            state.windows[id(12)] = window([3])
            state.windows[id(31)] = window([4])
            state.show(
                message(1, in: 11, by: user(2)), message(2, in: 11, by: user(2), content: sourceContent),
                message(3, in: 12), message(4, in: 31))
            state.threadCreations[id(11)] = creation
            state.startable = [id(1), id(2)]
        }
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    func created(_ raw: UInt64) {
        account.createReply.withLock { $0 = .success(thread(raw, in: 11, name: "new")) }
    }

    func added(_ session: SessionModel, _ thread: UInt64) {
        session.apply(EventBatch([.channelAdded(channelId: id(thread), guildId: id(1))]))
    }

    @Test
    func theFormTakesTheSplitsPlaceAndASplitTakesItBack() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        #expect(session.threadForm?.parentId == id(11))
        #expect(session.split == nil)
        #expect(memory.splits(of: id(1))[id(11)] == nil)

        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.threadForm == nil)
        #expect(session.split?.channelId == id(31))
        session.close()
    }

    @Test
    func eachChannelKeepsItsFormAndWhatsTypedAcrossSwitches() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        session.open(channel: id(12))
        #expect(session.threadForm == nil)
        session.open(channel: id(11))

        #expect(session.threadForm?.name == "plans")
        #expect(session.threadForm?.message == "first")
        session.close()
    }

    @Test
    func theComposersTextMovesIntoAnEmptyFormsMessage() {
        let session = inChannel11()
        session.messages?.composer.draft = "hello"

        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        #expect(session.threadForm?.message == "hello")
        #expect(session.messages?.composer.draft == "")

        session.closeThreadForm()
        session.messages?.composer.draft = "more"
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.message == "hello")
        #expect(session.messages?.composer.draft == "more")
        session.close()
    }

    @Test
    func escapeClosesTheFormFirstAndKeepsWhatsTyped() async {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.message = "first"

        #expect(session.escape() == false)
        await finished(session.escaping)

        #expect(session.threadForm == nil)
        #expect(account.markedRead.current.isEmpty)
        #expect(session.focusChannel)
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.message == "first")
        session.close()
    }

    @Test
    func discardingTheFormForgetsWhatsTyped() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.isPrivate = true
        session.threadForm?.message = "first"
        #expect(session.threadForm?.hasText == true)

        session.discardThreadForm()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        #expect(session.threadForm?.name == "")
        #expect(session.threadForm?.isPrivate == false)
        #expect(session.threadForm?.message == "")
        #expect(session.threadForm?.hasText == false)
        session.close()
    }

    @Test
    func nothingOpensWhereTheUserMayCreateNoThread() {
        let session = inChannel11(creation: ThreadCreation(public: false, private: false))

        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm == nil)

        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: true, private: false) }
        store.update { $0.startable = [] }
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        #expect(session.threadForm == nil)
        session.close()
    }

    @Test
    func theBrowsersCreateClosesTheBrowser() {
        let session = inChannel11()
        session.openThreadBrowser(of: id(11), as: .popout)

        session.openThreadForm(in: id(11), location: .threadBrowserEmptyState)

        #expect(session.threadBrowser == nil)
        #expect(session.threadForm?.location == .threadBrowserEmptyState)
        session.close()
    }

    @Test
    func theShortcutOpensTheFormOfTheOpenChannelOrOfTheThreadsParent() {
        let session = inChannel11()
        session.go(to: id(31))

        session.openThreadFormHere()

        #expect(session.messages?.channelId == id(11))
        #expect(session.threadForm?.parentId == id(11))
        #expect(session.threadForm?.location == .keyboardShortcut)
        #expect(session.threadForm?.source == nil)
        session.close()
    }

    @Test
    func reopenedFromAnotherPlaceItTellsDiscordThatPlace() async {
        let session = inChannel11()
        created(41)
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        session.openThreadFormHere()
        await session.submitThreadForm()

        #expect(
            creates == [.createThread(id(11), nil, "plans", isPrivate: false, .keyboardShortcut)])
        session.close()
    }

    @Test
    func theOpenChannelsRowClosesItsForm() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        session.open(channel: id(11))

        #expect(session.threadForm == nil)
        session.close()
    }

    @Test
    func aMessageFormNamesItselfFromTheMessageAndHasNoCheckbox() {
        let session = inChannel11()

        session.openThreadForm(in: id(11), from: id(2), location: .message)

        let form = session.threadForm
        #expect(form?.source?.id == id(2))
        #expect(form?.nameLabel == "Thread Name (Optional)")
        #expect(form?.namePlaceholder == "das ist ein ziemlich langer satz zum ")
        #expect(form?.privateMode == .disabled)
        session.close()
    }

    @Test
    func aFormWithoutAMessageAsksForAName() {
        let session = inChannel11()

        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        #expect(session.threadForm?.nameLabel == "Thread Name")
        #expect(session.threadForm?.namePlaceholder == "New Thread")
        session.close()
    }

    @Test(arguments: [
        (true, true, ThreadFormModel.PrivateMode.enabled),
        (false, true, .privateOnly),
        (true, false, .disabled),
    ])
    func theCheckboxFollowsThePermissions(public: Bool, private: Bool, mode: ThreadFormModel.PrivateMode) {
        let session = inChannel11(creation: ThreadCreation(public: `public`, private: `private`))

        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        #expect(session.threadForm?.privateMode == mode)
        #expect(session.threadForm?.createsPrivate == (mode == .privateOnly))
        session.close()
    }

    @Test
    func anotherMessageStartsTheNameOver() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.name = "about two"
        session.threadForm?.message = "first"
        session.closeThreadForm()

        session.openThreadForm(in: id(11), from: id(1), location: .message)

        #expect(session.threadForm?.name == "")
        #expect(session.threadForm?.message == "first")
        session.close()
    }

    @Test
    func theNameFieldTakesNoLineBreaksAndAtMost100Characters() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        session.threadForm?.name = "two\nlines\r\nhere"
        #expect(session.threadForm?.name == "two lines here")
        session.threadForm?.name = String(repeating: "é", count: 120)
        #expect(session.threadForm?.name == String(repeating: "é", count: 100))
        session.close()
    }

    @Test
    func bothFieldsSayWhatsMissingAndTypingClearsIt() async {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        await session.submitThreadForm()

        #expect(session.threadForm?.nameError == "Thread Name is required")
        #expect(session.threadForm?.messageError == "Starter Message is required.")
        #expect(creates.isEmpty)
        session.threadForm?.name = "x"
        session.threadForm?.message = "y"
        #expect(session.threadForm?.nameError == nil)
        #expect(session.threadForm?.messageError == nil)
        session.close()
    }

    @Test
    func aMessageFormNeedsOnlyTheFirstMessage() async {
        let session = inChannel11()
        session.openThreadForm(in: id(11), from: id(2), location: .message)

        await session.submitThreadForm()

        #expect(session.threadForm?.nameError == nil)
        #expect(session.threadForm?.messageError == "Starter Message is required.")
        session.close()
    }

    @Test
    func aTooLongFirstMessageSaysSoBeforeAnythingIsCreated() async {
        let session = inChannel11()
        store.update { $0.lengthLimit = 10 }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "t"
        session.threadForm?.message = "eleven char"

        await session.submitThreadForm()

        #expect(session.threadForm?.messageError?.contains("too long") == true)
        #expect(creates.isEmpty)
        session.close()
    }

    @Test
    func aThreadFromAMessageSendsTheDerivedName() async {
        let session = inChannel11()
        created(2)
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.message = "first"

        await session.submitThreadForm()

        #expect(
            creates == [
                .createThread(
                    id(11), id(2), "das ist ein ziemlich langer satz zum ", isPrivate: false,
                    .message)
            ])
        session.close()
    }

    @Test
    func aMessageWithoutTextNamesItsThreadThread() async {
        let session = inChannel11(sourceContent: "")
        created(2)
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        #expect(session.threadForm?.namePlaceholder == "New Thread")
        session.threadForm?.message = "first"

        await session.submitThreadForm()

        #expect(creates == [.createThread(id(11), id(2), "Thread", isPrivate: false, .message)])
        session.close()
    }

    @Test
    func aPrivateThreadIsCreatedAsTyped() async {
        let session = inChannel11()
        created(41)
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "  f4 "
        session.threadForm?.isPrivate = true
        session.threadForm?.message = "first"

        await session.submitThreadForm()

        #expect(
            creates == [
                .createThread(id(11), nil, "f4", isPrivate: true, .threadBrowserToolbar)
            ])
        session.close()
    }

    @Test
    func aCreatedThreadOpensBesideItsChannelAndGetsTheFirstMessage() async {
        let session = inChannel11()
        created(41)
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "  first  "

        await session.submitThreadForm()

        #expect(session.threadForm == nil)
        #expect(session.split?.channelId == id(41))
        #expect(session.focusSplit)
        #expect(memory.splits(of: id(1))[id(11)]?.thread == id(41))
        #expect(calls.contains(.send(id(41), "first")))
        #expect(calls.contains(.deliver(id(41), id(900))))
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.hasText == false)
        session.close()
    }

    @Test
    func elsewhereByThenTheFormJustClosesAndTheMessageStillGoes() async {
        let session = inChannel11()
        created(41)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        session.open(channel: id(12))
        account.createReplies.send(())
        await submitting.value

        #expect(memory.splits(of: id(1))[id(11)] == nil)
        session.open(channel: id(11))
        #expect(session.threadForm == nil)
        #expect(session.split == nil)
        #expect(calls.contains(.send(id(41), "first")))
        session.close()
    }

    @Test
    func aFirstMessageTheCoreRefusesBecomesTheThreadsDraft() async {
        let session = inChannel11()
        created(41)
        account.queueError.withLock { $0 = .Closed }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        await session.submitThreadForm()

        #expect(session.split?.composer.draft == "first")
        session.close()
    }

    @Test
    func aSecondEnterWhileCreatingCreatesNothing() async {
        let session = inChannel11()
        created(41)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        let first = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        await session.submitThreadForm()
        account.createReplies.send(())
        await first.value

        #expect(creates.count == 1)
        session.close()
    }

    func refused(_ error: CreateThreadError) async -> ThreadFormModel? {
        let session = inChannel11()
        account.createReply.withLock { $0 = .failure(error) }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"
        await session.submitThreadForm()
        defer { session.close() }
        return session.threadForm
    }

    @Test
    func aRefusalKeepsEverythingAndSaysWhyInDiscordsWords() async {
        let full = await refused(.TooManyThreads(announcement: false))
        #expect(full?.alert?.title == "Cannot create thread")
        #expect(
            full?.alert?.message
                == "This server has reached the maximum number of active threads. Free up space by archiving a thread."
        )
        #expect(full?.name == "plans")
        #expect(full?.message == "first")

        let announcements = await refused(.TooManyThreads(announcement: true))
        #expect(
            announcements?.alert?.message
                == "This server has reached the maximum number of active announcement threads. Free up space by archiving a thread."
        )
        let limited = await refused(.Request(error: .RateLimited(retryAfter: 3)))
        #expect(limited?.alert == .init(title: "Cannot create thread", message: "You are being rate limited."))
        let other = await refused(.Request(error: .Network(kind: .connect)))
        #expect(other?.alert == .init(title: "Error", message: "An error occurred, please try again."))
        let captcha = await refused(.Request(error: .InvalidRequest))
        #expect(captcha?.alert?.title == "Error")
    }

    @Test
    func nameRefusalsShowUnderTheName() async {
        let required = await refused(.NameRequired)
        #expect(required?.nameError == "Thread Name is required")
        #expect(required?.alert == nil)

        let blocked = await refused(.NameBlocked(reason: "Not here."))
        #expect(blocked?.nameError == "Not here.")
        blocked?.name = "other"
        #expect(blocked?.nameError == "Not here.", "AutoMod's stays until the next try")

        let silent = await refused(.NameBlocked(reason: ""))
        #expect(silent?.nameError?.hasPrefix("This thread cannot be sent") == true)
    }

    @Test
    func slowmodeShowsNoDialog() async {
        let slowed = await refused(.Slowmode)
        #expect(slowed?.alert == nil)
        #expect(slowed?.nameError == nil)
    }

    @Test
    func someoneElsesThreadFromTheMessageTakesTheFormOver() async {
        let session = inChannel11()
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.message = "mine"
        session.threadForm?.name = "a name"

        store.update { $0.channels[id(2)] = thread(2, in: 11, name: "theirs") }
        added(session, 2)

        #expect(session.threadForm == nil)
        #expect(session.split?.channelId == id(2))
        #expect(session.split?.composer.draft == "mine")
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.hasText == false)
        session.close()
    }

    @Test
    func theUsersOwnNewThreadIsntTakenOver() async {
        let session = inChannel11()
        created(2)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.message = "first"

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        store.update { $0.channels[id(2)] = thread(2, in: 11, name: "new") }
        added(session, 2)
        account.createReplies.send(())
        await submitting.value

        #expect(calls.contains(.send(id(2), "first")))
        #expect(session.split?.channelId == id(2))
        #expect(session.split?.composer.draft == "")
        session.close()
    }

    @Test
    func theCooldownShowsAndEndsOnItsOwn() async {
        let session = inChannel11()
        store.update {
            $0.threadSlowmodes[id(11)] = Slowmode(
                interval: 30, exempt: false, until: Date().addingTimeInterval(0.2))
        }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.slowmode?.until != nil)

        store.update {
            $0.threadSlowmodes[id(11)] = Slowmode(interval: 30, exempt: false, until: nil)
        }
        await waitFor { session.threadForm?.slowmode?.until == nil }

        #expect(session.threadForm?.slowmode?.interval == 30)
        session.close()
    }

    @Test
    func aDeletedChannelTakesItsFormAlong() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)

        store.update { $0.channelLists[id(1)] = [id(12)] }
        session.apply(EventBatch([.channelRemoved(channelId: id(11), guildId: id(1))]))

        #expect(session.threadForm == nil)
        session.close()
    }

    @Test
    func aMessageFormKeepsItsMessageWhenTheWindowLetsItGo() async {
        let session = inChannel11()
        created(2)
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        let form = session.threadForm
        session.threadForm?.name = "about two"
        session.threadForm?.message = "first"

        store.update { state in
            state.windows[id(11)] = window([1])
            state.messages[id(2)] = nil
        }
        session.apply(EventBatch([.messagesTrimmed(channelId: id(11), first: id(1), last: id(1))]))
        #expect(session.threadForm === form)
        session.open(channel: id(12))
        session.open(channel: id(11))
        #expect(session.threadForm?.source?.id == id(2))
        #expect(session.threadForm?.name == "about two")
        await session.submitThreadForm()

        #expect(creates == [.createThread(id(11), id(2), "about two", isPrivate: false, .message)])
        session.close()
    }

    @Test
    func aFormClosedWhileItsThreadIsCreatedOpensNothing() async {
        let session = inChannel11()
        created(41)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        session.closeThreadForm()
        await session.openBeside(thread: id(31), from: id(11))
        account.createReplies.send(())
        await submitting.value

        #expect(session.split?.channelId == id(31))
        #expect(memory.splits(of: id(1))[id(11)]?.thread == id(31))
        #expect(session.threadForm == nil)
        #expect(calls.contains(.send(id(41), "first")))
        session.close()
    }

    @Test
    func theFirstMessageIsWhatWasTypedAtEnter() async {
        let session = inChannel11()
        created(41)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"
        let form = session.threadForm

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        session.threadForm?.message = "changed"
        session.openThreadFormHere()
        #expect(session.threadForm === form, "a request keeps its form")
        account.createReplies.send(())
        await submitting.value

        #expect(calls.contains(.send(id(41), "first")))
        session.close()
    }

    @Test
    func someoneElsesThreadIsntReadUntilTheUserActs() async {
        let session = inChannel11()
        session.apply(EventBatch([.ready]))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        session.openThreadForm(in: id(11), from: id(2), location: .message)

        store.update { state in
            state.channels[id(2)] = thread(2, in: 11, name: "theirs")
            state.windows[id(2)] = window([])
        }
        added(session, 2)
        await finished(session.pendingReading)

        #expect(session.split?.channelId == id(2))
        #expect(!session.reading.contains(id(2)))
        session.userInteracted()
        #expect(session.reading.contains(id(2)))
        session.close()
    }

    @Test
    func anEditedSourceMessageShowsAndNamesTheFormLive() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        let form = session.threadForm

        store.update { $0.show(message(2, in: 11, by: user(2), content: "ganz neuer text")) }
        session.apply(EventBatch([.messageUpdated(channelId: id(11), messageId: id(2))]))

        #expect(session.threadForm === form)
        #expect(session.threadForm?.source?.content == "ganz neuer text")
        #expect(session.threadForm?.namePlaceholder == "ganz neuer text")
        session.close()
    }

    // The user's work stays: both texts, and a thread without the message.
    @Test
    func aDeletedSourceMessageLeavesAFormWithoutItKeepingBothTexts() async {
        let session = inChannel11()
        created(41)
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"

        store.update { state in
            state.windows[id(11)] = window([1])
            state.messages[id(2)] = nil
        }
        session.apply(EventBatch([.messageDeleted(channelId: id(11), messageId: id(2))]))

        #expect(session.threadForm?.source == nil)
        #expect(session.threadForm?.nameLabel == "Thread Name")
        #expect(session.threadForm?.name == "plans")
        #expect(session.threadForm?.message == "first")
        #expect(session.threadForm?.privateMode == .enabled)
        await session.submitThreadForm()
        #expect(creates == [.createThread(id(11), nil, "plans", isPrivate: false, .message)])
        session.close()
    }

    @Test
    func aSourceDiscordNoLongerKnowsLeavesAFormWithoutIt() async {
        let session = inChannel11()
        account.createReply.withLock {
            $0 = .failure(.Request(error: .Discord(status: 404, code: 10008, message: "Unknown Message")))
        }
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.message = "first"

        await session.submitThreadForm()

        #expect(session.threadForm?.source == nil)
        #expect(session.threadForm?.message == "first")
        #expect(session.threadForm?.nameError == "Thread Name is required")
        #expect(session.threadForm?.alert == nil)
        session.close()
    }

    @Test
    func losingThePermissionClosesTheFormKeepingWhatsTyped() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.message = "first"

        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: false, private: false) }
        session.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))
        #expect(session.threadForm == nil)

        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: true, private: true) }
        session.apply(EventBatch([.channelUpdated(channelId: id(11), guildId: id(1))]))
        #expect(session.threadForm == nil, "it doesn't come back on its own")
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        #expect(session.threadForm?.message == "first")
        session.close()
    }

    @Test
    func aMessageFormClosesWhenOnlyPrivateThreadsAreLeft() {
        let session = inChannel11()
        session.openThreadForm(in: id(11), from: id(2), location: .message)

        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: false, private: true) }
        session.apply(EventBatch([.channelUpdated(channelId: id(11), guildId: id(1))]))

        #expect(session.threadForm == nil)
        session.close()
    }

    @Test
    func aRefusalAfterAChannelSwitchWaitsForTheUsersReturn() async {
        let session = inChannel11()
        account.createReply.withLock { $0 = .failure(.TooManyThreads(announcement: false)) }
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), location: .threadBrowserToolbar)
        session.threadForm?.name = "plans"
        session.threadForm?.message = "first"
        let form = session.threadForm

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        session.open(channel: id(12))
        account.createReplies.send(())
        await submitting.value
        session.open(channel: id(11))

        #expect(session.threadForm === form)
        #expect(session.threadForm?.alert?.title == "Cannot create thread")
        #expect(session.threadForm?.message == "first")
        session.close()
    }

    // The later action wins: the form follows the new message, the first message still goes.
    @Test
    func reopeningFromAnotherMessageDuringACreateRetargetsTheForm() async {
        let session = inChannel11()
        created(2)
        account.holdCreates.withLock { $0 = true }
        session.openThreadForm(in: id(11), from: id(2), location: .message)
        session.threadForm?.message = "first"
        let form = session.threadForm

        let submitting = Task { await session.submitThreadForm() }
        await waitFor { !self.creates.isEmpty }
        session.openThreadForm(in: id(11), from: id(1), location: .message)
        #expect(session.threadForm === form)
        #expect(session.threadForm?.source?.id == id(1))
        account.createReplies.send(())
        await submitting.value

        #expect(session.threadForm?.source?.id == id(1))
        #expect(session.threadForm?.message == "", "it went out as the first message")
        #expect(session.split == nil)
        #expect(calls.contains(.send(id(2), "first")))
        session.close()
    }

    @Test
    func aLateBoxFetchDoesntReplaceAFormOpenedMeanwhile() async {
        let session = inChannel11()
        account.holdThreadFetches.withLock { $0 = true }
        account.fetchedThreads.withLock { $0[id(77)] = thread(77, in: 11, name: "late") }

        let clicking = Task { await session.openBeside(thread: id(77), from: id(11)) }
        await waitFor { self.calls.contains(.fetchThread(id(11), id(77))) }
        session.openThreadFormHere()
        account.threadFetches.send(())
        await clicking.value

        #expect(session.threadForm != nil)
        #expect(session.split == nil)
        session.close()
    }

    @Test
    func createThreadHereFollowsTheOpenChannelAndItsPermissions() {
        let session = inChannel11()
        #expect(session.canCreateThreadHere)

        session.go(to: id(31))
        #expect(session.canCreateThreadHere, "a thread's parent")
        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: false, private: false) }
        session.apply(EventBatch([.channelUpdated(channelId: id(11), guildId: id(1))]))
        #expect(!session.canCreateThreadHere)
        session.close()
    }

    @Test
    func aCreateThatMayHaveHappenedSaysTheThreadMayExist() async {
        let form = await refused(.MaybeCreated)
        #expect(form?.alert?.title == "Thread may exist")
        #expect(form?.alert?.message.contains("check the channel list") == true)
        #expect(form?.message == "first")
    }

    func waitFor(_ condition: @escaping () -> Bool) async {
        for _ in 0..<500 where !condition() {
            try? await Task.sleep(for: .milliseconds(5))
        }
    }
}
