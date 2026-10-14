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

        #expect(
            store.reads == [
                .window(here), .messages(here, [id(3), id(4), id(5)]), .slowmode(here),
            ])
        #expect(
            model.rows.map(\.message) == [message(3, content: "edited"), message(4), message(5)])
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
                .channelUpdated(channelId: id(21), guildId: id(2)),
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
        #expect(store.reads == [.window(here), .slowmode(here)])
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
        #expect(store.reads == [.window(here), .messages(here, [id(8)]), .slowmode(here)])

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
        #expect(store.reads == [.window(here), .slowmode(here)])
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
        #expect(store.reads == [.window(here), .slowmode(here), .window(here), .slowmode(here)])
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
        #expect(model.failedLoads == [.newer: .Network(kind: .timeout)])

        account.loadError.withLock { $0 = nil }
        await model.loadNewer()
        #expect(model.failedLoads.isEmpty)
        #expect(
            store.log.actions == [.load(here, .newer(limit: 50)), .load(here, .newer(limit: 50))])
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
    func aSuccessAtOneEdgeKeepsTheOtherEdgesFailure() async {
        shown(window([1, 2], latest: false))
        let model = loaded()
        account.loadError.withLock { $0 = .ServerError(status: 500) }
        await model.loadOlder()
        await model.loadNewer()
        #expect(
            model.failedLoads == [
                .older: .ServerError(status: 500), .newer: .ServerError(status: 500),
            ])

        account.loadError.withLock { $0 = nil }
        await model.loadNewer()
        #expect(model.failedLoads == [.older: .ServerError(status: 500)])

        await model.jumpToPresent()
        #expect(model.failedLoads.isEmpty)
    }

    @Test
    func aLoadKeepsItsTraceForTheLog() async {
        shown(window([3, 4, 5]))
        let model = loaded()
        let report = loadReport()
        account.loadReport.withLock { $0 = report }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5]) }

        await model.loadOlder()
        let first = model.lastLoad
        await model.loadOlder()

        #expect(first?.sequence == 1)
        #expect(first?.channelId == here)
        #expect(first?.kind == .older)
        #expect(first?.report == report)
        #expect(first?.failure == nil)
        #expect(first.map { $0.read > .zero } == true)
        #expect(model.lastLoad?.sequence == 2)
    }

    // The table charges a page's render to the update that inserted these rows, not to a
    // message that arrived meanwhile.
    @Test
    func aLoadKnowsWhichRowsItBrought() async {
        shown(window([3, 4, 5]))
        let model = loaded()
        account.loadReport.withLock { $0 = loadReport() }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6]) }
        store.update { $0.show(message(1), message(2), message(6)) }

        await model.loadOlder()

        #expect(model.lastLoad?.page == [id(1), id(2)])
    }

    @Test
    func aLoadWithNothingToLoadHasNoReport() async {
        shown(window([3, 4, 5]))
        let model = loaded()

        await model.loadOlder()

        #expect(model.lastLoad?.sequence == 1)
        #expect(model.lastLoad?.report == nil)
        #expect(model.lastLoad?.failure == nil)
    }

    @Test
    func aFailedLoadKeepsItsFailure() async {
        shown(window([3, 4, 5]))
        let model = loaded()
        account.loadError.withLock { $0 = .ServerError(status: 500) }

        await model.loadOlder()

        #expect(model.lastLoad?.failure == .ServerError(status: 500))
        #expect(model.lastLoad?.report == nil)
    }

    @Test
    func theFirstRowsSayWhereTheyCameFrom() {
        let empty = makeModel()
        shown(window([3, 4, 5]))
        let held = makeModel()

        #expect(empty.openedFrom == .network)
        #expect(held.openedFrom == .window)
    }

    @Test
    func theFirstRowsSayTheyCameFromDisk() {
        shown(window([]))
        let model = loaded()
        #expect(model.openedFrom == .network)

        shown(window([1, 2], stale: true))
        model.apply(
            EventBatch([
                .messagesLoaded(channelId: here, first: id(1), last: id(2)),
                .messagesStale(channelId: here),
            ]))

        #expect(model.openedFrom == .disk)
        #expect(model.rows.map(\.id) == [id(1), id(2)])
    }

    @Test
    func rowsFromTheNetworkStayFromTheNetwork() {
        shown(window([]))
        let model = loaded()

        shown(window([1, 2]))
        model.apply(EventBatch([.messagesLoaded(channelId: here, first: id(1), last: id(2))]))

        #expect(model.openedFrom == .network)
    }

    @Test
    func aPageAfterRowsFromDiskCountsOnlyWhatTheNetworkAdded() async {
        shown(window([]))
        let model = loaded()
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3]) }
        let opening = Task { await model.open() }
        while !store.log.actions.contains(where: { if case .load = $0 { true } else { false } }) {
            await Task.yield()
        }

        shown(window([1, 2], stale: true))
        model.apply(
            EventBatch([
                .messagesLoaded(channelId: here, first: id(1), last: id(2)),
                .messagesStale(channelId: here),
            ]))
        account.loadReplies.send(nil)
        await opening.value

        #expect(model.openedFrom == .disk)
        #expect(model.lastLoad?.page == [id(3)])
    }

    var messageReads: [CallLog.Read] {
        store.reads.filter { read in
            switch read {
            case .window, .messages: true
            default: false
            }
        }
    }

    @Test
    func aRevealSurvivesARereadAndALoad() {
        shown(window([1, 2]))
        let model = loaded()
        let key = SpoilerKey(messageId: id(2), index: 0)
        model.reveal(key)

        store.update { $0.show(message(2, content: "edited")) }
        model.apply(EventBatch([.messageUpdated(channelId: here, messageId: id(2))]))
        shown(window([1, 2, 3]))
        model.apply(EventBatch([.messageInserted(channelId: here, messageId: id(3))]))

        #expect(model.revealedSpoilers == [key])
    }

    @Test
    func revealsEndWithTheModel() {
        shown(window([1, 2]))
        loaded().reveal(SpoilerKey(messageId: id(2), index: 0))

        #expect(loaded().revealedSpoilers.isEmpty)
    }

    @Test(arguments: [
        StoreEvent.guildUpdated(guildId: id(1)),
        .channelUpdated(channelId: id(11), guildId: id(1)),
        .currentMemberUpdated(guildId: id(1)),
        .userUpdated(userId: id(7)),
        .currentUserUpdated,
        // Suppressed mentions aren't highlighted.
        .guildSettingsChanged(guildId: id(1)),
    ])
    func aRenameReloadsOnlyRowsThatShowIt(_ event: StoreEvent) {
        shown(window([1, 2, 3]))
        let model = loaded()
        let before = model.rows
        store.update { $0.show(message(2, content: "renamed")) }

        model.apply(EventBatch([event]))

        #expect(messageReads == [.messages(here, [id(1), id(2), id(3)])])
        #expect(model.rows[0] == before[0] && model.rows[2] == before[2])
        #expect(model.rows[1].message.content == "renamed")
    }

    @Test(arguments: [
        StoreEvent.guildUpdated(guildId: id(2)),
        .channelUpdated(channelId: id(20), guildId: id(2)),
        .currentMemberUpdated(guildId: id(2)),
        .guildSettingsChanged(guildId: id(2)),
        .guildSettingsChanged(guildId: nil),
    ])
    func aRenameElsewhereReadsNothing(_ event: StoreEvent) {
        shown(window([1, 2, 3]))
        let model = loaded()

        model.apply(EventBatch([event]))

        #expect(messageReads.isEmpty)
    }
}
