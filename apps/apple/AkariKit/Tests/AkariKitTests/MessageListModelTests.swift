import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct MessageListModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            state.show(message(1), message(2), message(3), message(4), message(5))
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: here, account: account, store: store)
    }

    func loaded() -> MessageListModel {
        let model = makeModel()
        store.log.forget()
        return model
    }

    func shown(_ window: MessageWindow?) {
        store.update { $0.windows[id(10)] = window }
    }

    @Test(arguments: [nil, window([]), window([1, 2], stale: true)])
    func openLoadsTheLatestPageWhenEmptyOrStale(_ window: MessageWindow?) async {
        shown(window)
        let model = loaded()

        await model.open()

        #expect(store.log.actions == [.load(here, .latest(limit: 50))])
    }

    @Test
    func openOnlyViewsAFilledWindow() async {
        shown(window([1, 2]))
        let model = loaded()

        await model.open()

        #expect(store.log.actions == [.view(here)])
    }

    @Test
    func oneBatchReadsTheWindowOnceAndOnlyNewOrChangedMessages() {
        shown(window([3]))
        let model = loaded()
        shown(window([3, 4, 5]))
        store.update { $0.show(message(3, content: "edited")) }

        model.apply(
            EventBatch([
                .messageInserted(channelId: here, messageId: id(4)),
                .messageInserted(channelId: here, messageId: id(5)),
                .messageUpdated(channelId: here, messageId: id(3)),
            ])
        )

        #expect(store.reads == [.window(here), .messages(here, [id(3), id(4), id(5)])])
        #expect(model.rows.map(\.message) == [message(3, content: "edited"), message(4), message(5)])
    }

    @Test
    func anEditOfAHeldMessageReadsOnlyThatMessage() {
        shown(window([1, 2]))
        let model = loaded()
        store.update { $0.show(message(2, content: "edited")) }

        model.apply(
            EventBatch([
                .messageUpdated(channelId: here, messageId: id(2)),
                .messageUpdated(channelId: here, messageId: id(7)),
            ])
        )

        #expect(store.reads == [.messages(here, [id(2)])])
        #expect(model.rows.map(\.message) == [message(1), message(2, content: "edited")])
    }

    @Test
    func eventsForOtherChannelsReadNothing() {
        shown(window([1]))
        let model = loaded()
        let other: ChannelId = id(11)

        model.apply(
            EventBatch([
                .messageInserted(channelId: other, messageId: id(2)),
                .messageUpdated(channelId: other, messageId: id(1)),
                .messageReplaced(channelId: other, pendingId: id(3), messageId: id(4)),
                .messagesLoaded(channelId: other, first: id(1), last: id(2)),
                .messagesStale(channelId: other),
                .messagesCleared(channelId: other),
                .channelUpdated(channelId: other, guildId: id(1)),
                .currentMemberUpdated(guildId: id(2)),
                .guildUpdated(guildId: id(2)),
            ])
        )

        #expect(store.reads.isEmpty)
        #expect(model.rows.map(\.message.id) == [id(1)])
    }

    @Test
    func eventsTheFirstReadAlreadyShowsChangeNothing() {
        shown(window([1, 2, 3]))
        let model = loaded()

        model.apply(
            EventBatch([
                .messagesTrimmed(channelId: here, first: id(2), last: id(3)),
                .messageInserted(channelId: here, messageId: id(3)),
            ])
        )

        #expect(model.rows.map(\.id) == [id(1), id(2), id(3)])
        #expect(store.reads == [.window(here)])
    }

    @Test
    func aConfirmedMessageKeepsItsRow() {
        store.update { $0.show(message(7, delivery: .pending), message(8), message(9)) }
        shown(window([1], pending: [7]))
        let model = loaded()

        shown(window([1, 8]))
        model.apply(
            EventBatch([.messageReplaced(channelId: here, pendingId: id(7), messageId: id(8))])
        )

        #expect(model.rows.map(\.id) == [id(1), id(7)])
        #expect(model.rows.map(\.message.id) == [id(1), id(8)])
        #expect(store.reads == [.window(here), .messages(here, [id(8)])])

        shown(window([1, 8, 9]))
        model.apply(EventBatch([.messageInserted(channelId: here, messageId: id(9))]))
        #expect(model.rows.map(\.id) == [id(1), id(7), id(9)])

        shown(window([9]))
        model.apply(EventBatch([.messagesTrimmed(channelId: here, first: id(1), last: id(8))]))
        shown(window([8, 9]))
        model.apply(EventBatch([.messagesLoaded(channelId: here, first: id(8), last: id(8))]))
        #expect(model.rows.map(\.id) == [id(8), id(9)])
    }

    @Test
    func aConfirmationOutsideThePresentRemovesThePendingRow() {
        store.update { $0.show(message(7, delivery: .pending)) }
        shown(window([1], pending: [7], latest: false))
        let model = loaded()

        shown(window([1], latest: false))
        model.apply(
            EventBatch([.messageReplaced(channelId: here, pendingId: id(7), messageId: id(8))])
        )

        #expect(model.rows.map(\.id) == [id(1)])
        #expect(store.reads == [.window(here)])
    }

    @Test
    func deletedAndClearedMessagesLeave() {
        shown(window([1, 2, 3]))
        let model = loaded()

        shown(window([1, 3]))
        model.apply(EventBatch([.messageDeleted(channelId: here, messageId: id(2))]))
        #expect(model.rows.map(\.id) == [id(1), id(3)])

        shown(nil)
        model.apply(EventBatch([.messagesCleared(channelId: here)]))
        #expect(model.rows.isEmpty)
        #expect(store.reads == [.window(here), .window(here)])
    }

    @Test
    func jumpToPresentLoadsTheLatest() async {
        shown(window([1, 2], latest: false))
        let model = loaded()
        #expect(!model.atPresent)

        await model.jumpToPresent()
        shown(window([4, 5]))
        model.apply(
            EventBatch([
                .messagesCleared(channelId: here),
                .messagesLoaded(channelId: here, first: id(4), last: id(5)),
            ])
        )

        #expect(store.log.actions == [.load(here, .latest(limit: 50))])
        #expect(model.rows.map(\.id) == [id(4), id(5)])
        #expect(model.atPresent)
    }

    @Test
    func rowsArriveWithTheLoad() async {
        let model = loaded()
        account.windowAfterLoad.withLock { $0 = window([1, 2]) }

        await model.open()

        #expect(model.loading == nil)
        #expect(model.rows.map(\.id) == [id(1), id(2)])

        account.windowAfterLoad.withLock { $0 = window([0, 1, 2], latest: true, oldest: true) }
        store.update { $0.show(message(0)) }
        await model.loadOlder()
        #expect(model.rows.map(\.id) == [id(0), id(1), id(2)])
        #expect(model.reachedOldest)
    }

    @Test
    func loadOlderStopsAtTheOldestAndWhileLoading() async {
        shown(window([3, 4]))
        let model = loaded()
        account.holdLoads.withLock { $0 = true }

        async let first: Void = model.loadOlder()
        await account.loadReplies.pulled(1)
        #expect(model.loading == .older)
        await model.loadOlder()
        account.loadReplies.send(nil)
        await first
        #expect(model.loading == nil)
        #expect(store.log.actions == [.load(here, .older(limit: 50))])

        shown(window([1, 2, 3, 4], oldest: true))
        model.apply(EventBatch([.messagesLoaded(channelId: here, first: id(1), last: id(2))]))
        await model.loadOlder()

        #expect(model.reachedOldest)
        #expect(store.log.actions == [.load(here, .older(limit: 50))])
    }

    @Test
    func aJumpDuringALoadShowsTheJumpUntilItEnds() async {
        shown(window([3, 4], latest: false))
        let model = loaded()
        account.holdLoads.withLock { $0 = true }

        async let older: Void = model.loadOlder()
        await account.loadReplies.pulled(1)
        async let jump: Void = model.jumpToPresent()
        await account.loadReplies.pulled(2)
        account.loadReplies.send(nil)
        await older
        #expect(model.loading == .latest)

        account.loadReplies.send(nil)
        await jump
        #expect(model.loading == nil)
    }

    @Test
    func aLoadThatEndsFirstLeavesTheOtherShowing() async {
        shown(window([3, 4], latest: false))
        let model = loaded()
        account.holdLoads.withLock { $0 = true }

        async let older: Void = model.loadOlder()
        await account.loadReplies.pulled(1)
        account.holdLoads.withLock { $0 = false }
        await model.jumpToPresent()
        #expect(model.loading == .older)
        await model.loadOlder()
        #expect(store.log.actions.filter { $0 == .load(here, .older(limit: 50)) }.count == 1)

        account.holdLoads.withLock { $0 = true }
        async let jump: Void = model.jumpToPresent()
        await account.loadReplies.pulled(2)
        account.holdLoads.withLock { $0 = false }
        await model.jumpToPresent()
        #expect(model.loading == .latest)

        account.loadReplies.send(nil)
        await older
        #expect(model.loading == .latest)
        account.loadReplies.send(nil)
        await jump
        #expect(model.loading == nil)
    }

    @Test
    func loadNewerStopsAtThePresentAndErrorsShowUntilALoadWorks() async {
        shown(window([1, 2]))
        let model = loaded()

        await model.loadNewer()
        #expect(store.log.actions.isEmpty)

        shown(window([1, 2], latest: false))
        model.apply(EventBatch([.messagesTrimmed(channelId: here, first: id(3), last: id(3))]))
        account.loadError.withLock { $0 = .Network(kind: .timeout) }
        await model.loadNewer()
        #expect(model.loadError == .Network(kind: .timeout))

        account.loadError.withLock { $0 = nil }
        await model.loadNewer()
        #expect(model.loadError == nil)
        #expect(store.log.actions == [.load(here, .newer(limit: 50)), .load(here, .newer(limit: 50))])
    }

    @Test
    func flagsFollowTheWindow() {
        shown(window([1], latest: false, oldest: true, stale: true))
        let model = loaded()
        #expect(model.reachedOldest)
        #expect(!model.atPresent)
        #expect(model.isStale)

        shown(window([1], latest: true, oldest: false, stale: false))
        model.apply(EventBatch([.messagesStale(channelId: here)]))

        #expect(!model.reachedOldest)
        #expect(model.atPresent)
        #expect(!model.isStale)
    }

    @Test
    func sendErrorsShowAndTheFailedMessageStays() async {
        store.update { $0.show(message(7, delivery: .failed)) }
        shown(window([1]))
        let model = loaded()
        account.sendError.withLock { $0 = .RateLimited(retryAfter: 2) }

        await model.send("hello")
        shown(window([1], pending: [7]))
        model.apply(EventBatch([.messageInserted(channelId: here, messageId: id(7))]))
        #expect(model.sendError == .RateLimited(retryAfter: 2))
        #expect(model.rows.last?.message.delivery == .failed)

        account.sendError.withLock { $0 = nil }
        await model.retry(id(7))
        #expect(model.sendError == nil)
        model.discard(id(7))

        #expect(
            store.log.actions == [.send(here, "hello"), .retry(here, id(7)), .discard(here, id(7))]
        )
    }

    @Test
    func canSendFollowsPermissionsAndDms() {
        store.update { $0.permissions[id(10)] = [.viewChannel] }
        let model = loaded()
        #expect(!model.canSend)

        store.update { $0.permissions[id(10)] = [.viewChannel, .sendMessages] }
        model.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))
        #expect(model.canSend)

        store.update { $0.permissions[id(10)] = [.viewChannel] }
        model.apply(EventBatch([.channelUpdated(channelId: here, guildId: id(1))]))
        #expect(!model.canSend)

        store.update { $0.permissions[id(10)] = [.administrator, .sendMessages] }
        model.apply(EventBatch([.guildUpdated(guildId: id(1))]))
        #expect(model.canSend)

        store.update { $0.permissions[id(10)] = [] }
        model.apply(EventBatch([.ready]))
        #expect(!model.canSend)

        store.update { $0.channels[id(20)] = channel(20, guild: nil, kind: .dm) }
        let dm = MessageListModel(channelId: id(20), account: account, store: store)
        let unknown = MessageListModel(channelId: id(30), account: account, store: store)
        #expect(dm.canSend)
        #expect(!unknown.canSend)
    }
}
