import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct NewMessagesLineTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let me = user(1)
    let mira = user(2, name: "mira")

    init() {
        account = FakeAccount(store: store)
        store.update { [mira] state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            for raw in UInt64(1)...8 {
                state.show(message(raw, by: mira))
            }
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: here, userId: id(1), account: account, store: store)
    }

    func readState(_ acked: UInt64?) -> ReadState {
        ReadState(acked: acked.map { id($0) }, mentions: 0, estimated: false)
    }

    func shown(_ ids: [UInt64], readUpTo acked: UInt64?) {
        store.update { [here] state in
            state.windows[here] = window(ids)
            state.readStates[here] = readState(acked)
        }
    }

    // A live message as the store adds it; the core reads it at once while the user reads.
    func arrive(_ raw: UInt64, in model: MessageListModel, by author: User? = nil) {
        store.update { [here, mira] state in
            state.show(message(raw, by: author ?? mira))
            let ids = (state.windows[here]?.messageIds.map(\.rawValue) ?? []) + [raw]
            state.windows[here] = window(ids)
            state.readStates[here] = readState(raw)
        }
        model.apply(
            EventBatch([
                .messageInserted(channelId: here, messageId: id(raw)),
                .badgeChanged(channelId: here, guildId: id(1)),
            ]))
    }

    func timeline(_ model: MessageListModel) -> MessageTimeline {
        MessageTimeline(rows: model.rows, calendar: berlin, newMessages: model.newMessagesLine)
    }

    @Test
    func theLineSitsBeforeTheFirstUnreadMessageAtOpen() {
        shown([1, 2, 3, 4], readUpTo: 2)

        let model = makeModel()

        #expect(model.newMessagesLine == id(3))
        #expect(timeline(model).shape == ["day", "+1", "2", "new", "3", "4"])
    }

    @Test
    func theLineUsesTheReadPositionFromBeforeTheAck() async {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()
        // The core acks the channel as soon as the session sets it as being read.
        store.update { [here] in $0.readStates[here] = readState(4) }

        await model.open()
        model.apply(
            EventBatch([
                .badgeChanged(channelId: here, guildId: id(1)),
                .messagesRefreshed(channelId: here),
            ]))

        #expect(model.newMessagesLine == id(3))
    }

    @Test
    func ownMessagesDontStartALine() {
        store.update { [me] in $0.show(message(5, by: me), message(6, by: me)) }
        shown([1, 2, 3, 4, 5, 6], readUpTo: 4)
        let model = makeModel()
        #expect(model.newMessagesLine == nil)

        arrive(7, in: model, by: me)
        #expect(model.newMessagesLine == nil)

        arrive(8, in: model)
        #expect(model.newMessagesLine == id(8))
    }

    @Test
    func noLineWithoutUnreadMessages() {
        shown([1, 2, 3, 4], readUpTo: 4)
        #expect(makeModel().newMessagesLine == nil)

        // Without a read position, what the list first shows counts as read.
        store.update { [here] in $0.readStates[here] = nil }
        #expect(makeModel().newMessagesLine == nil)

        store.update { [here] in $0.windows[here] = nil }
        #expect(makeModel().newMessagesLine == nil)
    }

    @Test
    func aMessageArrivingWithoutALineStartsOne() {
        shown([1, 2], readUpTo: 2)
        let model = makeModel()
        #expect(model.newMessagesLine == nil)
        #expect(model.atPresentBottom, "the user reads at the bottom")

        arrive(3, in: model)

        #expect(model.newMessagesLine == id(3))
    }

    @Test
    func aSecondMessageDoesntMoveTheLine() {
        shown([1, 2], readUpTo: 2)
        let model = makeModel()

        arrive(3, in: model)
        arrive(4, in: model)

        #expect(model.newMessagesLine == id(3))
        #expect(timeline(model).shape == ["day", "+1", "2", "new", "3", "4"])
    }

    @Test
    func theLineStaysAfterTheAck() {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()

        store.update { [here] in $0.readStates[here] = readState(4) }
        model.apply(EventBatch([.badgeChanged(channelId: here, guildId: id(1))]))
        model.setAtBottom(false)
        model.setAtBottom(true)
        // A reconnect: a new session, then the refreshed window.
        model.apply(EventBatch([.ready, .messagesStale(channelId: here)]))
        model.apply(EventBatch([.messagesRefreshed(channelId: here)]))

        #expect(model.newMessagesLine == id(3))
    }

    @Test
    func sendingAMessageRemovesTheLine() async {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()
        #expect(model.newMessagesLine == id(3))

        model.composer.draft = "hi"
        await model.composer.submit()
        #expect(model.newMessagesLine == nil)

        arrive(5, in: model)
        #expect(model.newMessagesLine == id(5), "a later message starts a new one")
    }

    @Test
    func escapeRemovesTheLine() async {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()
        #expect(model.newMessagesLine == id(3))

        await model.escape()

        #expect(account.markedRead.current == [here])
        #expect(model.newMessagesLine == nil)
    }

    // Marking read acks up to the channel's newest message, also one the list hasn't loaded.
    @Test
    func escapeAwayFromThePresentLeavesNoLineInTheLatestPage() async {
        store.update { [here] in $0.windows[here] = window([1, 2, 3, 4], latest: false) }
        store.update { [here] in $0.readStates[here] = readState(2) }
        account.readStateAfterMarkRead.withLock { $0 = readState(8) }
        account.windowAfterLoad.withLock { $0 = window([5, 6, 7, 8]) }
        let model = makeModel()
        #expect(model.newMessagesLine == id(3))

        await model.escape()

        #expect(model.rows.map(\.id) == [5, 6, 7, 8].map { id($0) })
        #expect(model.newMessagesLine == nil)
    }

    @Test
    func markingTheChannelOrItsServerReadRemovesTheLine() throws {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(10)], in: 1)
        }
        shown([1, 2, 3, 4], readUpTo: 2)
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        let list = try #require(session.messages)
        #expect(list.newMessagesLine == id(3))

        session.markRead(channel: id(11))
        session.markRead(guild: id(2))
        #expect(list.newMessagesLine == id(3), "another channel or server")

        session.markRead(channel: here)
        #expect(list.newMessagesLine == nil)

        arrive(5, in: list)
        #expect(list.newMessagesLine == id(5))
        session.markServerRead()
        #expect(list.newMessagesLine == nil)
        session.close()
    }

    // The store had no read state when the list opened, and the cache brings one before READY.
    @Test
    func aCacheRestoredAfterTheListOpenedGivesTheReadPosition() {
        store.update { [here] in $0.windows[here] = window([1, 2, 3, 4]) }
        let model = makeModel()
        #expect(model.newMessagesLine == nil)

        store.update { [here] in $0.readStates[here] = readState(2) }
        model.apply(EventBatch([.restored, .messagesRefreshed(channelId: here)]))

        #expect(model.newMessagesLine == id(3))
    }

    // The latest page may not reach back to the first unread message; an older one can.
    @Test
    func anOlderPageWithEarlierUnreadMessagesMovesTheLineUp() async {
        store.update { [here] in $0.windows[here] = window([4, 5, 6]) }
        store.update { [here] in $0.readStates[here] = readState(2) }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6]) }
        let model = makeModel()
        #expect(model.newMessagesLine == id(4))

        await model.loadOlder()

        #expect(model.newMessagesLine == id(3))
    }

    // Guild 1 with this channel, opened from the cache while the gateway connects, in an
    // active key window.
    func connectingSession() throws -> (SessionModel, MessageListModel) {
        store.update { state in
            state.connection = .connecting
            state.add(guild(1))
            state.list([channel(10)], in: 1)
        }
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        return (session, try #require(session.messages))
    }

    // Read on another device while Akari was closed: READY has a newer position than the cache.
    @Test
    func aReadyWithANewerReadPositionMovesTheLineDown() throws {
        shown([1, 2, 3, 4, 5, 6], readUpTo: 2)
        let (session, list) = try connectingSession()
        #expect(list.newMessagesLine == id(3))

        store.update { [here] in $0.readStates[here] = readState(4) }
        session.apply(EventBatch([.ready]))
        #expect(list.newMessagesLine == id(5))

        store.update { [here] in $0.readStates[here] = readState(6) }
        session.apply(EventBatch([.ready]))
        #expect(list.newMessagesLine == nil, "past every loaded message")
        session.close()
    }

    // The core acks the channel as soon as it's set as being read, here in READY's own batch.
    @Test
    func aReadyMovesTheLineBeforeTheChannelIsRead() throws {
        shown([1, 2, 3, 4, 5, 6], readUpTo: 2)
        account.readStateAfterReading.withLock { $0 = readState(6) }
        let (session, list) = try connectingSession()

        store.update { [here] in $0.readStates[here] = readState(4) }
        session.apply(EventBatch([.ready, .connection(state: .online)]))

        #expect(account.readingCalls.current == [here])
        #expect(list.newMessagesLine == id(5))
        session.close()
    }

    @Test
    func aReadyNeverMovesTheLineUp() throws {
        shown([1, 2, 3, 4, 5, 6], readUpTo: 4)
        let (session, list) = try connectingSession()

        store.update { [here] in $0.readStates[here] = readState(2) }
        session.apply(EventBatch([.ready]))

        #expect(list.newMessagesLine == id(5))
        session.close()
    }

    // After Akari read the channel, READY's position is Akari's own ack.
    @Test
    func aReconnectAfterReadingKeepsTheLine() throws {
        shown([1, 2, 3, 4, 5, 6], readUpTo: 2)
        account.readStateAfterReading.withLock { $0 = readState(6) }
        let (session, list) = try connectingSession()
        session.apply(EventBatch([.ready, .connection(state: .online)]))
        #expect(account.readingCalls.current == [here])

        session.apply(EventBatch([.connection(state: .connecting)]))
        session.apply(EventBatch([.ready]))
        session.apply(EventBatch([.connection(state: .online)]))

        #expect(list.newMessagesLine == id(3))
        session.close()
    }
}
