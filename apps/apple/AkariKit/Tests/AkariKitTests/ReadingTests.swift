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

    @Test
    func readingIsSetOnlyWhileEngagedActiveKeyVisibleAndAtTheBottom() throws {
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
            ("scrolled up", { list.setAtBottom(false) }, { list.setAtBottom(true) }),
        ]
        for (name, leave, comeBack) in breaks {
            leave()
            #expect(session.reading == nil, "\(name)")
            comeBack()
            #expect(session.reading == nil, "\(name), not engaged")
            session.userInteracted()
            #expect(session.reading == general, "\(name), engaged again")
        }
        #expect(readings == [general, nil, general, nil, general, nil, general, nil, general])
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
    func scrollingUpStopsReading() {
        let session = readingSession()

        session.messages?.setAtBottom(false)

        #expect(session.messages?.atPresentBottom == false)
        #expect(readings == [general, nil])
        session.close()
    }

    @Test
    func scrollingBackDownDoesntReadUntilAClick() {
        let session = readingSession()
        session.messages?.setAtBottom(false)
        session.messages?.setAtBottom(true)
        #expect(session.reading == nil)

        session.userInteracted()

        #expect(readings == [general, nil, general])
        session.close()
    }

    // Stricter than the official client, which acks on that click.
    @Test
    func aClickWhileScrolledUpReadsOnceBackAtTheBottom() {
        let session = readingSession()
        session.messages?.setAtBottom(false)
        session.userInteracted()
        #expect(session.reading == nil)

        session.messages?.setAtBottom(true)

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

    @Test
    func aReplacedListNoLongerCounts() throws {
        let session = readingSession()
        let old = try #require(session.messages)
        session.open(channel: random)

        old.setAtBottom(false)

        #expect(readings == [general, random])
        session.close()
    }
}
