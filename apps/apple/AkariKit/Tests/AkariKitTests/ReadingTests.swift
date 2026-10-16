import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
final class ReadingTests {
    let store = FakeStore()
    let account: FakeAccount
    let general: ChannelId = id(11)
    let random: ChannelId = id(12)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
            state.windows[id(11)] = window([1, 2])
            state.windows[id(12)] = window([3])
            state.show(message(1, in: 11), message(2, in: 11), message(3, in: 12))
        }
    }

    var readings: [ChannelId?] { account.readingCalls.current }

    func makeSession() -> SessionModel {
        SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
    }

    // The guild's first channel open in the active app's key, visible window.
    func readingSession() -> SessionModel {
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        #expect(readings == [general])
        return session
    }

    // As the table reports a scroll by the user to older messages.
    func scrollUp(_ list: MessageListModel) {
        list.setAtBottom(false)
        list.userScrolled(by: 400, height: 600)
    }

    // The list's reports reach the core a turn later.
    func settle(_ session: SessionModel) async {
        await finished(session.pendingReading)
    }

    @Test
    func readingIsSetOnlyWhileEngagedActiveKeyVisibleAndAtTheBottom() async throws {
        let session = makeSession()
        session.start()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        #expect(readings.isEmpty, "no channel open")
        session.open(.guild(id(1)))
        #expect(session.reading == general)
        let list = try #require(session.messages)

        let breaks: [(String, () -> Void, () -> Void)] = [
            ("inactive", { session.setAppActive(false) }, { session.setAppActive(true) }),
            ("not key", { session.setWindowKey(false) }, { session.setWindowKey(true) }),
            ("hidden", { session.setWindowVisible(false) }, { session.setWindowVisible(true) }),
            ("scrolled up", { self.scrollUp(list) }, { list.setAtBottom(true) }),
        ]
        for (name, leave, comeBack) in breaks {
            leave()
            await settle(session)
            #expect(session.reading == nil, "\(name)")
            comeBack()
            await settle(session)
            #expect(session.reading == nil, "\(name), not engaged")
            session.userInteracted()
            #expect(session.reading == general, "\(name), engaged again")
        }
        #expect(readings == [general, nil, general, nil, general, nil, general, nil, general])
        session.close()
    }

    // At READY the store has no channel being read, so READY's read states reach the lists.
    @Test
    func readingWaitsForTheSessionToBeOnline() {
        store.update { $0.connection = .connecting }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        session.apply(EventBatch([.ready]))
        #expect(readings.isEmpty)

        session.apply(EventBatch([.connection(state: .online)]))

        #expect(readings == [general])
        session.close()
    }

    @Test
    func goingOfflineStopsReading() {
        let session = readingSession()

        session.apply(EventBatch([.connection(state: .connecting)]))
        #expect(session.reading == nil)
        session.apply(EventBatch([.connection(state: .offline)]))
        session.apply(EventBatch([.connection(state: .online)]))

        #expect(readings == [general, nil, general], "the engagement stays")
        session.close()
    }

    // The core forgets the channel whenever the connection drops, also before a resume.
    @Test
    func readingIsSetAgainAfterAReconnect() {
        let session = readingSession()

        session.apply(EventBatch([.connection(state: .connecting), .connection(state: .online)]))
        #expect(readings == [general, general], "dropped and resumed within one batch")

        session.apply(EventBatch([.connection(state: .connecting)]))
        session.apply(EventBatch([.ready, .connection(state: .online)]))
        #expect(readings == [general, general, nil, general])

        store.update { $0.windows[id(11)] = window([1, 2], stale: true) }
        session.apply(
            EventBatch([
                .connection(state: .connecting), .ready, .messagesStale(channelId: general),
                .connection(state: .online),
            ]))
        #expect(session.reading == nil, "not before the list is refreshed")
        store.update { $0.windows[id(11)] = window([1, 2]) }
        session.apply(EventBatch([.messagesRefreshed(channelId: general)]))

        #expect(readings == [general, general, nil, general, nil, general])
        session.close()
    }

    // READY opens the remembered channel before its list has applied READY's stale mark.
    @Test
    func theChannelReadyReopensIsntReadBeforeItsRefreshed() {
        store.update { state in
            state.connection = .connecting
            state.currentUser = nil
        }
        let memory = AccountMemory(defaults: MemoryDefaults())
        memory.remember(.init(place: .guild(id(1)), channel: random), of: id(1))
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }
        session.start()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)

        store.update { state in
            state.currentUser = user(1)
            state.windows[id(12)] = window([3], stale: true)
        }
        session.apply(
            EventBatch([.ready, .messagesStale(channelId: random), .connection(state: .online)]))
        #expect(session.messages?.channelId == random)
        #expect(readings.isEmpty)

        store.update { $0.windows[id(12)] = window([3]) }
        session.apply(EventBatch([.messagesRefreshed(channelId: random)]))
        #expect(readings == [random])
        session.close()
    }

    @Test
    func aListAwayFromThePresentIsntRead() {
        store.update { $0.windows[id(12)] = window([3], latest: false) }
        let session = readingSession()

        session.open(channel: random)
        #expect(session.reading == nil)

        store.update { $0.windows[id(12)] = window([3, 4]) }
        session.apply(EventBatch([.messageInserted(channelId: random, messageId: id(4))]))
        #expect(session.reading == random, "opening engaged it")
        #expect(readings == [general, nil, random])
        session.close()
    }

    @Test
    func openingAChannelEngages() {
        let session = readingSession()
        session.setWindowKey(false)
        session.setWindowKey(true)
        #expect(session.reading == nil)

        session.open(channel: random)

        #expect(readings == [general, nil, random])
        session.close()
    }

    @Test
    func scrollingUpStopsReading() async throws {
        let session = readingSession()

        scrollUp(try #require(session.messages))
        await settle(session)

        #expect(session.messages?.atPresentBottom == false)
        #expect(readings == [general, nil])
        session.close()
    }

    @Test
    func scrollingBackDownDoesntReadUntilAClick() async throws {
        let session = readingSession()
        scrollUp(try #require(session.messages))
        await settle(session)
        session.messages?.setAtBottom(true)
        await settle(session)
        #expect(session.reading == nil)

        session.userInteracted()

        #expect(readings == [general, nil, general])
        session.close()
    }

    // Stricter than the official client, which acks on that click.
    @Test
    func aClickWhileScrolledUpReadsOnceBackAtTheBottom() async throws {
        let session = readingSession()
        scrollUp(try #require(session.messages))
        session.userInteracted()
        #expect(session.reading == nil)

        session.messages?.setAtBottom(true)
        await settle(session)

        #expect(readings == [general, nil, general])
        session.close()
    }

    // The bar's jump to the first unread message: reading pauses and comes back at the bottom.
    @Test
    func theTablesOwnJumpKeepsEngagement() async throws {
        let session = readingSession()
        let list = try #require(session.messages)

        list.setAtBottom(false)
        await settle(session)
        #expect(session.reading == nil)
        list.userScrolled(by: -300, height: 600)
        list.setAtBottom(true)
        await settle(session)

        #expect(readings == [general, nil, general])
        session.close()
    }

    // A trackpad's momentum or bounce, or a message arriving during a gesture.
    @Test
    func aSmallNudgeAtTheBottomKeepsEngagement() async throws {
        let session = readingSession()
        let list = try #require(session.messages)

        list.setAtBottom(false)
        await settle(session)
        list.userScrolled(by: 60, height: 600)
        list.userScrolled(by: -20, height: 600)
        list.userScrolled(by: 60, height: 600)
        list.setAtBottom(true)
        await settle(session)
        #expect(readings == [general, nil, general])

        list.setAtBottom(false)
        await settle(session)
        list.userScrolled(by: 100, height: 600)
        list.setAtBottom(true)
        await settle(session)
        #expect(readings == [general, nil, general, nil, general], "counted from the bottom")
        session.close()
    }

    @Test
    func scrollingClearlyAwayDisengages() async throws {
        let session = readingSession()
        let list = try #require(session.messages)

        list.setAtBottom(false)
        await settle(session)
        list.userScrolled(by: 100, height: 600)
        list.userScrolled(by: 100, height: 600)
        list.setAtBottom(true)
        await settle(session)
        #expect(session.reading == nil)
        session.userInteracted()
        #expect(session.reading == general)

        list.setAtBottom(false)
        list.userScrolled(by: 50, height: 120)
        list.setAtBottom(true)
        await settle(session)
        #expect(session.reading == nil, "a short list")

        #expect(readings == [general, nil, general, nil])
        session.close()
    }

    // The table reports from inside AppKit's scroll callbacks.
    @Test
    func setReadingIsCalledOnlyOnChange() async throws {
        let session = readingSession()
        let list = try #require(session.messages)

        for _ in 0..<5 {
            list.setAtBottom(false)
            list.setAtBottom(true)
        }
        list.setAtBottom(true)
        #expect(session.reading == general)
        await settle(session)
        #expect(readings == [general], "the burst ended where it started")

        list.setAtBottom(false)
        #expect(session.reading == nil)
        #expect(readings == [general], "not inside the callback")
        await settle(session)
        #expect(readings == [general, nil])
        session.close()
    }

    // Before the Mac sleeps.
    @Test
    func suspendDisengages() {
        let session = readingSession()

        session.suspend()
        #expect(session.reading == nil)
        session.userInteracted()
        #expect(session.reading == nil, "a click before the offline batch")
        session.apply(EventBatch([.connection(state: .offline)]))
        session.resume()
        session.apply(
            EventBatch([.connection(state: .connecting), .ready, .connection(state: .online)]))
        #expect(session.reading == nil, "not before a click or a key")

        session.userInteracted()
        #expect(readings == [general, nil, general])
        session.close()
    }

    @Test
    func goingToTheBackgroundStopsReading() {
        let session = readingSession()

        session.setAppActive(false)
        session.setWindowKey(false)

        #expect(readings == [general, nil])
        session.close()
    }

    @Test
    func comingBackToTheWindowDoesntReadUntilAClick() {
        let session = readingSession()
        session.setAppActive(false)
        session.setWindowKey(false)

        session.setAppActive(true)
        session.setWindowKey(true)
        #expect(session.reading == nil)

        session.userInteracted()
        #expect(readings == [general, nil, general])
        session.close()
    }

    @Test
    func aMinimizedWindowStopsReading() {
        let session = readingSession()

        session.setWindowVisible(false)
        #expect(session.reading == nil)
        session.setWindowVisible(true)
        #expect(session.reading == nil)

        session.userInteracted()
        #expect(readings == [general, nil, general])
        session.close()
    }

    @Test
    func readingIsSetOnceForTheSameChannel() {
        let session = readingSession()

        session.userInteracted()
        session.userInteracted()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.open(channel: general)
        store.update { $0.windows[id(11)] = window([1, 2, 4]) }
        session.apply(EventBatch([.messageInserted(channelId: general, messageId: id(4))]))

        #expect(readings == [general])
        session.close()
    }

    @Test
    func closingTheSessionOrTheListStopsReading() {
        let session = readingSession()
        session.open(.home)
        #expect(session.messages == nil)
        #expect(readings == [general, nil])

        session.open(.guild(id(1)))
        #expect(readings == [general, nil, general])
        session.close()
        #expect(readings == [general, nil, general, nil])
    }

    // On a 401 AppModel closes the session from onClosed, inside the batch that ended it.
    @Test
    func aClosedAccountIsToldNothing() {
        var closing: SessionModel?
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in closing?.close() }
        closing = session
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        #expect(readings == [general])

        session.apply(EventBatch([.connection(state: .closed(error: .AuthenticationFailed))]))

        #expect(session.reading == nil)
        #expect(readings == [general])
    }

    // Going elsewhere first opens the place's last or first channel; that one isn't read.
    @Test
    func goingToAChannelElsewhereReadsOnlyThatChannel() {
        store.update { state in
            state.add(guild(2))
            state.list([channel(13, guild: 2), channel(14, guild: 2)], in: 2)
            state.talk(dm(20, with: 5), dm(21, with: 6), with: user(5), user(6))
        }
        let session = readingSession()
        session.open(.home)
        session.open(channel: id(20))
        session.open(.guild(id(1)))
        let before = readings.count

        session.go(to: id(21))
        #expect(Array(readings.dropFirst(before)) == [id(21)], "not the DM last opened at home")

        session.go(to: id(14))
        #expect(
            Array(readings.dropFirst(before)) == [id(21), id(14)],
            "not the server's first channel")
        session.close()
    }

    // Only the user's choice reads a channel, not a list the session replaced on its own.
    @Test
    func aListReplacedOnItsOwnIsntReadUntilAClick() {
        store.update { $0.list([channel(11), channel(12), channel(16)], in: 1) }
        let session = readingSession()

        store.update { state in
            state.channelLists[id(1)] = [id(12), id(16)]
            state.channels[id(11)] = nil
        }
        session.apply(EventBatch([.channelRemoved(channelId: general, guildId: id(1))]))
        #expect(session.messages?.channelId == random)
        #expect(session.reading == nil)

        session.userInteracted()
        #expect(readings == [general, nil, random])
        session.close()
    }

    @Test
    func theConversationShownForAGoneServerIsntReadUntilAClick() {
        store.update { $0.talk(dm(20, with: 5), with: user(5)) }
        let session = readingSession()
        session.open(.home)
        session.open(channel: id(20))
        session.open(.guild(id(1)))
        let before = readings.count

        store.update { state in
            state.guildIds = []
            state.guilds[id(1)] = nil
        }
        session.apply(EventBatch([.guildRemoved(guildId: id(1))]))
        #expect(session.place == .home)
        #expect(session.messages?.channelId == id(20))
        #expect(session.reading == nil)

        session.userInteracted()
        #expect(Array(readings.dropFirst(before)) == [nil, id(20)])
        session.close()
    }

    // The launch reopens the last channel as if the user had opened it.
    @Test
    func theChannelReopenedAtLaunchIsReadWithoutAClick() {
        store.update { $0.connection = .connecting }
        let memory = AccountMemory(defaults: MemoryDefaults())
        memory.remember(.init(place: .guild(id(1)), channel: random), of: id(1))
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }
        session.start()
        for shown in [false, true] {
            session.setAppActive(shown)
            session.setWindowKey(shown)
            session.setWindowVisible(shown)
        }
        session.apply(EventBatch([.ready, .connection(state: .online)]))
        #expect(session.messages?.channelId == random)
        #expect(readings == [random])

        store.update { state in
            state.channelLists[id(1)] = [id(11)]
            state.channels[id(12)] = nil
        }
        session.apply(EventBatch([.channelRemoved(channelId: random, guildId: id(1))]))
        #expect(session.messages?.channelId == general)
        #expect(session.reading == nil, "replaced on its own")

        session.userInteracted()
        #expect(readings == [random, nil, general])
        session.close()
    }

    // READY leaves the list stale; reading it then would ack messages the user hasn't seen.
    @Test
    func aStaleListIsntReadUntilItsRefreshed() {
        store.update { $0.connection = .connecting }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)

        store.update { $0.windows[id(11)] = window([1, 2], stale: true) }
        session.apply(
            EventBatch([.ready, .messagesStale(channelId: general), .connection(state: .online)]))
        #expect(session.messages?.isStale == true)
        #expect(readings.isEmpty)

        store.update { $0.windows[id(11)] = window([1, 2]) }
        session.apply(EventBatch([.messagesRefreshed(channelId: general)]))
        #expect(readings == [general])
        session.close()
    }

    @Test
    func aReplacedListNoLongerCounts() async throws {
        let session = readingSession()
        let old = try #require(session.messages)
        session.open(channel: random)

        scrollUp(old)
        await settle(session)

        #expect(readings == [general, random])
        session.close()
    }
}
