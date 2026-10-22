import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1 with threads 31 and 32 under it, and channel 12; each has a window at
// the present. The session is online, active, key and visible, in channel 11.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class SplitViewTests {
    let store = FakeStore()
    let account: FakeAccount
    let defaults = MemoryDefaults()
    let memory: AccountMemory

    init() {
        account = FakeAccount(store: store)
        memory = AccountMemory(defaults: defaults)
    }

    var subscription: FakeSubscription { store.subscription }
    var readings: [[ChannelId]] { account.readingCalls.current }

    func world(joined: Bool = false, listed: Bool = false) {
        var thread31 = thread(31, in: 11, name: "t1")
        thread31.thread?.joined = joined
        var thread32 = thread(32, in: 11, name: "t2")
        thread32.thread?.joined = false
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11)] + (listed ? [thread31] : []) + [channel(12)], in: 1)
            state.channels[id(31)] = thread31
            state.channels[id(32)] = thread32
            state.windows[id(11)] = window([1, 2])
            state.windows[id(12)] = window([3])
            state.windows[id(31)] = window([4])
            state.windows[id(32)] = window([5])
            state.show(
                message(1, in: 11), message(2, in: 11), message(3, in: 12),
                message(4, in: 31), message(5, in: 32))
        }
    }

    func split(_ thread: UInt64, guild: UInt64 = 1) -> AccountMemory.Split {
        AccountMemory.Split(thread: id(thread), guild: id(guild))
    }

    func makeSession() -> SessionModel {
        SessionModel(userId: id(1), account: account, memory: memory) { _ in }
    }

    func inChannel11(joined: Bool = false, listed: Bool = false) -> SessionModel {
        world(joined: joined, listed: listed)
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        return session
    }

    func shown(_ session: SessionModel) -> [ChannelId] {
        session.channels?.shown(open: session.messages?.channelId).map(\.id) ?? []
    }

    func settle(_ session: SessionModel) async {
        await finished(session.pendingReading)
    }

    // Thread 31's message 4 is from someone else and unread, so its list opens with the line.
    func threadUnread() {
        store.update { state in
            state.show(message(4, in: 31, by: user(2)))
            state.readStates[id(31)] = ReadState(acked: id(3), mentions: 0, estimated: false)
        }
    }

    func scrollUp(_ list: MessageListModel) {
        list.setAtBottom(false)
        list.userScrolled(by: 400, height: 600)
    }

    @Test
    func theBoxOpensTheThreadBesideItsParent() async {
        let session = inChannel11()

        await session.openBeside(thread: id(31), from: id(11))

        #expect(session.messages?.channelId == id(11))
        #expect(session.split?.channelId == id(31))
        #expect(session.splitChannel?.id == id(31))
        #expect(shown(session) == [id(11), id(12)], "an unjoined thread gets no row")
        session.close()
    }

    @Test
    func anotherBoxReplacesTheSplitAndItsOwnBoxKeepsIt() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        let first = session.split

        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.split === first)
        await session.openBeside(thread: id(32), from: id(11))

        #expect(session.split?.channelId == id(32))
        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    @Test
    func aBoxOutsideTheOpenChannelOpensNothing() async {
        let session = inChannel11()

        await session.openBeside(thread: id(31), from: id(12))

        #expect(session.split == nil)
        session.close()
    }

    @Test
    func bothListsReadAtTheirBottom() async {
        let session = inChannel11()
        #expect(session.reading == [id(11)])

        await session.openBeside(thread: id(31), from: id(11))

        #expect(session.reading == [id(11), id(31)])
        #expect(readings.last == [id(11), id(31)], "the open channel first")
        session.userInteracted()
        #expect(readings.last == [id(11), id(31)])
        session.close()
    }

    @Test
    func aScrolledUpListDoesntReadAndTheOtherStillDoes() async throws {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        let split = try #require(session.split)
        let parent = try #require(session.messages)

        scrollUp(split)
        await settle(session)
        #expect(session.reading == [id(11)])
        split.setAtBottom(true)
        await settle(session)
        #expect(session.reading == [id(11)], "not before a click or a key")
        session.userInteracted()
        #expect(session.reading == [id(11), id(31)])

        scrollUp(parent)
        await settle(session)
        #expect(session.reading == [id(31)])
        #expect(readings.last == [id(31)])
        session.close()
    }

    @Test
    func theSplitComesBackWithItsParent() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        session.open(channel: id(12))
        #expect(session.split == nil)
        #expect(session.reading == [id(12)])
        session.open(channel: id(11))

        #expect(session.split?.channelId == id(31))
        #expect(session.reading == [id(11), id(31)])
        session.close()
    }

    @Test
    func theSplitFollowsItsThreadsMessages() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        store.update { state in
            state.windows[id(31)] = window([4, 6])
            state.show(message(6, in: 31, by: user(2)))
        }

        subscription.send(.messageInserted(channelId: id(31), messageId: id(6)))
        await subscription.batches.pulled(2)

        #expect(session.split?.rows.map(\.id) == [id(4), id(6)])
        session.close()
    }

    @Test
    func escapeInEitherPaneMarksBothAndJumpsBoth() async {
        let session = inChannel11()
        threadUnread()
        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.split?.newMessagesLine == id(4))
        store.update { state in
            state.windows[id(31)] = window([4], latest: false)
        }

        #expect(session.escape())
        await finished(session.escaping)

        #expect(account.markedRead.current.sorted() == [id(11), id(31)])
        #expect(store.log.calls.contains(.load(id(31), .latest(limit: 50))))
        #expect(session.messages?.newMessagesLine == nil)
        #expect(session.split?.newMessagesLine == nil)
        session.close()
    }

    // Q37 and the official client's key handler: with nothing to mark or load in either list,
    // Escape closes the split.
    @Test
    func escapeWithNothingLeftToDoClosesTheSplit() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        #expect(!session.escape())

        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)).isEmpty)
        #expect(account.markedRead.current.isEmpty)
        #expect(session.escape(), "without a split Escape always does its part")
        session.close()
    }

    @Test(arguments: [11, 31])
    func escapeWithAnUnreadListKeepsTheSplit(unread: UInt64) async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        store.update { state in
            state.channelBadges[id(unread)] = badge(unread: true)
        }

        #expect(session.escape())
        await finished(session.escaping)

        #expect(session.split?.channelId == id(31))
        #expect(account.markedRead.current.sorted() == [id(11), id(31)])
        session.close()
    }

    @Test
    func markingTheServerReadEndsBothLines() async {
        let session = inChannel11()
        threadUnread()
        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.split?.newMessagesLine == id(4))

        session.markServerRead()

        #expect(account.markedGuildsRead.current == [[id(1)]])
        #expect(session.split?.newMessagesLine == nil)
        session.close()
    }

    @Test
    func markingTheSplitThreadReadEndsItsLine() async {
        let session = inChannel11()
        threadUnread()
        await session.openBeside(thread: id(31), from: id(11))

        session.markRead(channel: id(31))

        #expect(account.markedRead.current == [id(31)])
        #expect(session.split?.newMessagesLine == nil)
        session.close()
    }

    // Discord's "Open in Full View" (and the split's title) only navigate: the channel keeps
    // its split.
    @Test
    func openInFullViewMovesTheThreadAndTheChannelKeepsIt() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        session.openInFullView()

        #expect(session.messages?.channelId == id(31))
        #expect(session.split == nil)
        #expect(shown(session) == [id(11), id(31), id(12)], "the full view keeps its row")
        session.open(channel: id(11))
        #expect(session.split?.channelId == id(31))
        session.close()
    }

    // Opening a thread full width from its row closes and forgets its channel's split.
    @Test(arguments: [31, 32])
    func aThreadsRowForgetsItsChannelsSplit(row: UInt64) async {
        let session = inChannel11(joined: true, listed: true)
        await session.openBeside(thread: id(31), from: id(11))

        session.open(channel: id(row))

        #expect(session.messages?.channelId == id(row))
        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)).isEmpty)
        session.open(channel: id(11))
        #expect(session.split == nil)
        session.close()
    }

    // A link or mention only navigates, as "Open in Full View" does.
    @Test
    func goingToTheSplitThreadKeepsTheSplit() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        session.go(to: id(31))
        session.open(channel: id(11))

        #expect(session.split?.channelId == id(31))
        session.close()
    }

    // The channel's plain route closes the split in the official client.
    @Test
    func theOpenChannelsRowClosesItsSplit() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        session.open(channel: id(11))

        #expect(session.messages?.channelId == id(11))
        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func openingBesideAsksForTheSplitsComposer() async {
        let session = inChannel11()
        #expect(!session.focusSplit)

        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.focusSplit)
        session.splitFocused()
        #expect(!session.focusSplit)
        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.focusSplit, "the open split's own box focuses it too")
        session.splitFocused()
        session.open(channel: id(12))
        session.go(to: id(11))
        #expect(session.split?.channelId == id(31))
        #expect(!session.focusSplit, "a split that comes back with its channel doesn't")
        session.close()
    }

    // A forum's posts have no channel list to open beside.
    @Test
    func openInSplitViewNeedsAParentWithAMessageList() {
        world()
        store.update { state in
            state.list(
                [channel(13, kind: .guildForum), thread(33, in: 13, name: "post"), channel(11)],
                in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.go(to: id(33))

        session.openInSplitView()

        #expect(session.messages?.channelId == id(33))
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func openInSplitViewAsksForTheSplitsComposer() {
        let session = inChannel11(joined: true, listed: true)
        session.go(to: id(31))

        session.openInSplitView()

        #expect(session.split?.channelId == id(31))
        #expect(session.focusSplit)
        session.close()
    }

    @Test
    func openInSplitViewOpensTheParentWithTheThread() {
        let session = inChannel11(joined: true, listed: true)
        session.go(to: id(31))

        session.openInSplitView()

        #expect(session.messages?.channelId == id(11))
        #expect(session.split?.channelId == id(31))
        session.close()
    }

    // The split's composer goes with it; the channel's takes the focus.
    @Test
    func closingTheSplitAsksForTheChannelsComposer() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        #expect(!session.focusChannel)

        session.closeSplit()
        #expect(session.focusChannel)
        session.channelFocused()
        #expect(!session.focusChannel)
        await session.openBeside(thread: id(31), from: id(11))
        session.splitFocused()
        #expect(!session.escape())
        #expect(session.focusChannel, "Escape closing it does the same")
        session.channelFocused()
        session.closeSplit()
        #expect(!session.focusChannel, "nothing closed")
        session.close()
    }

    @Test
    func closingTheSplitForgetsIt() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        #expect(memory.splits(of: id(1)) == [id(11): split(31)])

        session.closeSplit()

        #expect(session.split == nil)
        #expect(session.reading == [id(11)])
        #expect(memory.splits(of: id(1)).isEmpty)
        session.open(channel: id(12))
        session.open(channel: id(11))
        #expect(session.split == nil)
        session.close()
    }

    @Test
    func aDeletedSplitThreadCloses() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        store.update { state in
            state.channels[id(31)] = nil
        }

        subscription.send(.channelRemoved(channelId: id(31), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.split == nil)
        #expect(session.messages?.channelId == id(11))
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func aThreadInTheSplitHighlightsOnlyItsParent() async {
        let session = inChannel11(joined: true, listed: true)

        await session.openBeside(thread: id(31), from: id(11))

        #expect(session.messages?.channelId == id(11), "the sidebar's selection")
        #expect(shown(session) == [id(11), id(31), id(12)], "a listed thread keeps its plain row")
        session.close()
    }

    @Test
    func leavingFromTheSplitDropsTheRowAtOnceAndKeepsTheSplit() async {
        let session = inChannel11(joined: true, listed: true)
        await session.openBeside(thread: id(31), from: id(11))

        await session.leave(thread: id(31), from: .sidebarOverflow)
        var left = thread(31, in: 11, name: "t1")
        left.thread?.joined = false
        store.update { state in
            state.channels[id(31)] = left
            state.channelLists[id(1)] = [id(11), id(12)]
        }
        session.apply(EventBatch([.channelUpdated(channelId: id(31), guildId: id(1))]))

        #expect(store.log.calls.contains(.leaveThread(id(31), .sidebarOverflow)))
        #expect(shown(session) == [id(11), id(12)])
        #expect(session.split?.channelId == id(31))
        #expect(session.splitChannel?.thread?.joined == false)
        #expect(session.membershipItem(for: id(31))?.title == "Join Thread")
        session.close()
    }

    @Test
    func theSplitsMenuJoinsWithSidebarOverflow() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        #expect(
            session.membershipItem(for: id(31))
                == SessionModel.MembershipItem(joins: true, enabled: true))
        await session.join(thread: id(31), from: .sidebarOverflow)

        #expect(store.log.calls.contains(.joinThread(id(31), .sidebarOverflow)))
        session.close()
    }

    @Test
    func theSplitSurvivesARelaunch() async {
        world(joined: true, listed: true)
        memory.remember(.init(place: .guild(id(1)), channel: id(11)), of: id(1))
        memory.remember(splits: [id(11): split(31)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.messages?.channelId == id(11))
        #expect(session.split?.channelId == id(31))
        #expect(!store.log.calls.contains(.checkThread(id(11), id(31))))
        session.close()
    }

    @Test
    func aSplitThreadMissingAtLaunchIsFetched() async {
        world()
        store.update { state in
            state.channels[id(31)] = nil
        }
        account.fetchedThreads.withLock { $0[id(31)] = thread(31, in: 11) }
        memory.remember(.init(place: .guild(id(1)), channel: id(11)), of: id(1))
        memory.remember(splits: [id(11): split(31)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(31))))
        #expect(session.split?.channelId == id(31))
        session.close()
    }

    // READY lists only active joined threads: one archived meanwhile leaves the store, and
    // Get Channel decides whether the split stays.
    @Test
    func aSplitThreadReadyDropsIsFetchedNotForgotten() async {
        let session = inChannel11(joined: true, listed: true)
        await session.openBeside(thread: id(31), from: id(11))
        var archived = thread(31, in: 11, name: "t1")
        archived.thread?.archived = true
        account.fetchedThreads.withLock { $0[id(31)] = archived }
        store.update { state in
            state.channels[id(31)] = nil
            state.channelLists[id(1)] = [id(11), id(12)]
        }

        session.apply(
            EventBatch([.ready, .channelRemoved(channelId: id(31), guildId: id(1))]))
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(31))))
        #expect(session.split?.channelId == id(31))
        #expect(memory.splits(of: id(1)) == [id(11): split(31)])
        session.close()
    }

    @Test
    func aSplitThreadGoneAtLaunchIsDropped() async {
        world()
        memory.remember(.init(place: .guild(id(1)), channel: id(11)), of: id(1))
        memory.remember(splits: [id(11): split(33), id(12): split(34)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(33))))
        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)) == [id(12): split(34)])
        session.close()
    }

    @Test
    func aSplitThreadThatFailsToFetchAtLaunchIsKept() async {
        world()
        account.threadFetchError.withLock { $0 = .Network(kind: .connect) }
        memory.remember(.init(place: .guild(id(1)), channel: id(11)), of: id(1))
        memory.remember(splits: [id(11): split(33)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)
        await finished(session.splitFetch)

        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)) == [id(11): split(33)])
        subscription.send(.channelUpdated(channelId: id(11), guildId: id(1)))
        await subscription.batches.pulled(3)
        #expect(
            store.log.calls.filter { $0 == .checkThread(id(11), id(33)) }.count == 1,
            "once per session")
        session.close()
    }

    @Test
    func aSplitThreadsUnsentMessagesBecomeDrafts() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        store.update { state in
            state.windows[id(31)] = window([4], pending: [6])
            state.show(message(6, in: 31, content: "unsent", delivery: .pending))
        }

        session.keepUnsentAsDrafts()

        #expect(session.drafts[id(31)] == "unsent")
        session.close()
    }
}
