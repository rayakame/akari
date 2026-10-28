import Observation
import Testing

@testable import AkariKit

// The age gate (D-10): the session reads it from the store, Continue agrees for the server, Go
// Back opens the server's first channel that isn't gated, and a gated channel is never read.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class AgeGateTests {
    let store = FakeStore()
    let account: FakeAccount
    let adult: ChannelId = id(11)
    let general: ChannelId = id(12)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
            state.windows[id(11)] = window([1])
            state.windows[id(12)] = window([2])
            state.show(message(1, in: 11), message(2, in: 12))
            state.ageGates[id(11)] = .ask(server: false)
        }
    }

    func makeSession() -> SessionModel {
        SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
    }

    func reading() -> SessionModel {
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        return session
    }

    @Test
    func continueAgreesForTheServerAndTheChannelIsReadThen() {
        let session = reading()
        #expect(session.messages?.channelId == adult)
        #expect(session.ageGate(of: adult) == .ask(server: false))
        #expect(session.ageGate(of: general) == .open)
        #expect(session.reading.isEmpty, "nothing of a gated channel is read")

        session.agreeToAgeRestricted(in: adult)

        #expect(account.agreements.current == [id(1)])
        #expect(session.ageGate(of: adult) == .open)
        #expect(session.reading == [adult])
        session.close()
    }

    @Test
    func goBackOpensTheServersFirstOpenChannel() {
        let session = reading()

        session.leaveAgeGate()

        #expect(session.messages?.channelId == general)
        #expect(session.place == .guild(id(1)))
        session.close()
    }

    @Test
    func goBackGoesHomeWhenEveryChannelIsGated() {
        store.update { $0.ageGates[id(12)] = .tooYoung(server: true) }
        let session = reading()

        session.leaveAgeGate()

        #expect(session.place == .home)
        session.close()
    }

    // The header's Threads, its browser and the New Thread form would show what the gate hides.
    @Test
    func theThreadsEntriesWaitForTheGate() {
        store.update { $0.threadCreations[id(11)] = ThreadCreation(public: true, private: true) }
        let session = reading()
        #expect(session.threadsButtonParent == nil)

        session.openThreadBrowser(of: adult, as: .modal)
        #expect(session.threadBrowser == nil)
        session.openThreadForm(in: adult, location: .threadBrowserToolbar)
        session.openThreadFormHere()
        #expect(session.threadForm == nil)
        session.agreeToAgeRestricted(in: adult)
        #expect(session.threadsButtonParent == adult)
        session.close()
    }

    // A link's channel the store doesn't have (an unsynced thread, a channel the user can't see)
    // is asked about first: the gate needs its parent, and Discord may refuse it.
    @Test
    func anUnknownChannelIsFetchedBeforeItOpens() async throws {
        let session = reading()
        session.leaveAgeGate()
        let unknown: ChannelId = id(31)
        account.fetchedThreads.withLock { $0[unknown] = thread(31, in: 11) }
        store.update { $0.windows[unknown] = window([3]) }

        session.go(to: unknown)

        #expect(session.messages?.channelId == general, "not before the store knows it")
        try await eventually { session.messages?.channelId == unknown }
        #expect(account.log.calls.contains(.fetchThread(unknown, unknown)))
        session.close()
    }

    // The user went elsewhere while Discord answered: the link's channel doesn't open over it.
    @Test
    func anotherChannelWinsOverAFetchStillRunning() async throws {
        let session = reading()
        session.leaveAgeGate()
        let unknown: ChannelId = id(31)
        account.fetchedThreads.withLock { $0[unknown] = thread(31, in: 12) }
        account.holdThreadFetches.withLock { $0 = true }

        session.go(to: unknown)
        session.open(channel: adult)
        account.threadFetches.send(())
        try await eventually { self.store.channel(id: unknown) != nil }
        try await Task.sleep(for: .milliseconds(50))

        #expect(session.messages?.channelId == adult)
        session.close()
    }

    @Test
    func aChannelDiscordRefusesDoesntOpen() async throws {
        let session = reading()
        session.leaveAgeGate()

        session.go(to: id(99))

        try await eventually { self.account.log.calls.contains(.fetchThread(id(99), id(99))) }
        try await Task.sleep(for: .milliseconds(50))
        #expect(session.messages?.channelId == general)
        session.close()
    }

    // A view reading the gate updates when an event may have changed it.
    @Test
    func anAgeGateEventUpdatesItsReaders() async {
        let session = reading()
        let changed = Locked(false)
        withObservationTracking {
            _ = session.ageGate(of: adult)
        } onChange: {
            changed.withLock { $0 = true }
        }

        session.apply(EventBatch([.ageGateChanged(guildId: id(1))]))

        #expect(changed.current)
        session.close()
    }
}

// The session's own media state: revealed spoilers stay, the eye toggles, a settings change
// reads every row again.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class MediaStateTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11)], in: 1)
            state.windows[id(11)] = window([1])
            state.show(message(1, in: 11))
        }
    }

    func list() -> MessageListModel {
        MessageListModel(channelId: id(11), userId: id(1), account: account, store: store)
    }

    @Test
    func aRevealedSpoilerStaysAndTheEyeToggles() {
        let list = list()
        let key = MediaKey(messageId: id(1), attachmentId: AttachmentId(rawValue: 5))

        list.reveal(media: key)
        list.reveal(media: key)
        #expect(list.revealedMedia == [key])
        list.toggleSensitive(key)
        #expect(list.shownSensitive == [key])
        list.toggleSensitive(key)
        #expect(list.shownSensitive.isEmpty)
    }

    // CodeRabbit: a restore or a READY replaces the settings and conversations covers come from.
    @Test(arguments: [StoreEvent.restored, .ready])
    func aSessionChangeReadsEveryRowAgain(_ event: StoreEvent) {
        let list = list()
        var changed = message(1, in: 11)
        changed.content = "after"
        store.update { $0.show(changed) }

        list.apply(EventBatch([event]))

        #expect(list.rows.map(\.message.content) == ["after"])
    }

    @Test
    func aDisplaySettingsChangeReadsEveryRowAgain() {
        let list = list()
        var changed = message(1, in: 11)
        changed.content = "after"
        store.update { $0.show(changed) }

        list.apply(EventBatch([.displaySettingsChanged]))

        #expect(list.rows.map(\.message.content) == ["after"])
    }

    @Test
    func theOriginalLinkComesFromTheAccount() async {
        let key = ImageKey.attachment(
            channelId: id(11), messageId: id(1), attachmentId: AttachmentId(rawValue: 5))
        account.originalLinks.withLock { $0[key] = "https://cdn.discordapp.com/attachments/1/5/a.png?ex=1" }

        #expect(await list().originalURL(key)?.host() == "cdn.discordapp.com")
        let other = ImageKey.attachment(
            channelId: id(11), messageId: id(1), attachmentId: AttachmentId(rawValue: 6))
        #expect(await list().originalURL(other) == nil)
    }
}

struct AgeGateBatchTests {
    @Test
    func theEventMarksTheGatesChanged() {
        #expect(EventBatch([.ageGateChanged(guildId: id(1))]).ageGatesChanged)
        #expect(!EventBatch([.displaySettingsChanged]).ageGatesChanged)
    }
}
