import Foundation
import Testing

@testable import AkariKit

// What keeps a remembered split and what drops it: Discord's answer, the user's membership and
// view, a server left; and what a split the session shows on its own waits for.
extension SplitViewTests {
    func relaunched(splits: [ChannelId: AccountMemory.Split]) -> SessionModel {
        memory.remember(.init(place: .guild(id(1)), channel: id(11)), of: id(1))
        memory.remember(splits: splits, of: id(1))
        let session = makeSession()
        session.start()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        return session
    }

    func ready() async {
        subscription.send(.ready)
        await subscription.batches.pulled(2)
    }

    func unjoined(_ raw: UInt64, name: String) -> Channel {
        var found = thread(raw, in: 11, name: name)
        found.thread?.joined = false
        return found
    }

    // Thread 33 under channel 11, which only Get Channel brings.
    func thread33() {
        account.fetchedThreads.withLock { $0[id(33)] = thread(33, in: 11, name: "t3") }
        store.update { state in
            state.windows[id(33)] = window([7])
            state.show(message(7, in: 33))
        }
    }

    func threadRequests(_ thread: UInt64) -> Int {
        store.log.calls.filter {
            $0 == .checkThread(id(11), id(thread)) || $0 == .fetchThread(id(11), id(thread))
        }.count
    }

    // The cache keeps unjoined threads, and READY keeps them while their channel is there.
    @Test
    func aRememberedUnjoinedSplitIsCheckedOnceAfterReady() async {
        world()
        account.fetchedThreads.withLock { $0[id(31)] = unjoined(31, name: "t1") }
        let session = relaunched(splits: [id(11): split(31)])
        #expect(session.split?.channelId == id(31), "the cache's thread shows at once")
        #expect(threadRequests(31) == 0, "not before READY")

        await ready()
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(31))))
        #expect(session.split?.channelId == id(31))
        session.open(channel: id(12))
        session.open(channel: id(11))
        #expect(threadRequests(31) == 1, "once per session")
        session.close()
    }

    @Test
    func aSplitThreadDeletedWhileAkariWasClosedIsDropped() async {
        world()
        let session = relaunched(splits: [id(11): split(31), id(12): split(34)])
        #expect(session.split?.channelId == id(31))

        await ready()
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(31))))
        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)) == [id(12): split(34)])
        session.close()
    }

    // Discord's answer isn't a refusal when its guild isn't in the store yet.
    @Test
    func anUnplacedAnswerKeepsTheRememberedSplit() async {
        world()
        account.unplacedThreads.withLock { $0.insert(id(33)) }
        let session = relaunched(splits: [id(11): split(33)])

        await ready()
        await finished(session.splitFetch)

        #expect(store.log.calls.contains(.checkThread(id(11), id(33))))
        #expect(memory.splits(of: id(1)) == [id(11): split(33)])
        account.unplacedThreads.withLock { $0 = [] }
        thread33()
        subscription.send(.channelUpdated(channelId: id(11), guildId: id(1)))
        await subscription.batches.pulled(3)
        await finished(session.splitFetch)
        #expect(threadRequests(33) == 2, "asked again")
        #expect(session.split?.channelId == id(33))
        session.close()
    }

    // The launch's request joins the click's; whichever shows the split, the click is the user's.
    @Test
    func aBoxClickBeforeReadyStillReadsTheThreadTheLaunchShows() async {
        world()
        thread33()
        account.holdThreadFetches.withLock { $0 = true }
        let session = relaunched(splits: [id(11): split(33)])
        let opening = Task { await session.openBeside(thread: id(33), from: id(11)) }
        await account.threadFetches.pulled(1)

        await ready()
        account.threadFetches.send(())
        await opening.value
        await finished(session.splitFetch)
        await settle(session)

        #expect(threadRequests(33) == 1)
        #expect(session.split?.channelId == id(33))
        #expect(session.reading == [id(11), id(33)])
        session.close()
    }

    @Test
    func aBoxClickWhileTheLaunchAsksForTheSameThreadAsksOnce() async {
        world()
        thread33()
        account.holdThreadFetches.withLock { $0 = true }
        let session = relaunched(splits: [id(11): split(33)])
        await ready()
        await account.threadFetches.pulled(1)

        let opening = Task { await session.openBeside(thread: id(33), from: id(11)) }
        for _ in 0..<5 {
            await Task.yield()
        }
        account.threadFetches.send(())
        account.threadFetches.send(())
        await opening.value
        await finished(session.splitFetch)

        await settle(session)

        #expect(threadRequests(33) == 1)
        #expect(session.split?.channelId == id(33))
        #expect(session.reading == [id(11), id(33)], "the click is the user's")
        session.close()
    }

    // Discord sends no THREAD_DELETE for a private thread the user is removed from or leaves.
    @Test
    func aPrivateSplitThreadClosesWhenTheUserLosesItsMembership() async {
        let session = inChannel11(joined: true, listed: true)
        store.update { state in
            state.channels[id(31)]?.thread?.private = true
            state.permissions[id(31)] = [.viewChannel, .sendMessages]
        }
        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.split?.channelId == id(31))

        store.update { state in
            state.channels[id(31)]?.thread?.joined = false
            state.channelLists[id(1)] = [id(11), id(12)]
        }
        subscription.send(.channelUpdated(channelId: id(31), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.split == nil)
        #expect(session.messages?.channelId == id(11))
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func aModeratorKeepsAPrivateSplitThreadWithoutMembership() async {
        let session = inChannel11(joined: true, listed: true)
        store.update { state in
            state.channels[id(31)]?.thread?.private = true
            state.permissions[id(31)] = [.viewChannel, .manageThreads]
        }
        await session.openBeside(thread: id(31), from: id(11))

        store.update { state in
            state.channels[id(31)]?.thread?.joined = false
        }
        subscription.send(.channelUpdated(channelId: id(31), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.split?.channelId == id(31))
        #expect(memory.splits(of: id(1)) == [id(11): split(31)])
        session.close()
    }

    @Test
    func aSplitThreadTheUserCanNoLongerSeeIsForgotten() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        store.update { state in
            state.permissions[id(31)] = []
        }
        subscription.send(.channelUpdated(channelId: id(11), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.split == nil)
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func aChannelTheUserCanNoLongerSeeTakesItsSplit() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))

        store.update { state in
            state.permissions[id(11)] = []
            state.permissions[id(31)] = []
            state.channelLists[id(1)] = [id(12)]
        }
        subscription.send(.channelUpdated(channelId: id(11), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.messages?.channelId == id(12))
        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    @Test
    func aServerTheUserLeavesTakesItsSplits() async {
        world()
        memory.remember(splits: [id(11): split(31), id(21): split(41, guild: 2)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.guildRemoved(guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(memory.splits(of: id(1)) == [id(21): split(41, guild: 2)])
        session.close()
    }

    @Test
    func aServerDownKeepsItsSplits() async {
        world()
        memory.remember(splits: [id(11): split(31)], of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.guildUnavailable(guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(memory.splits(of: id(1)) == [id(11): split(31)])
        session.close()
    }

    // READY tells nothing of a server left while Akari was closed when the cache had none of it.
    @Test
    func aServerLeftWhileAkariWasClosedLosesItsSplitsAtReady() async {
        world()
        store.update { $0.unavailableGuildIds = [id(3)] }
        memory.remember(
            splits: [id(11): split(31), id(21): split(41, guild: 2), id(23): split(43, guild: 3)],
            of: id(1))
        let session = makeSession()
        session.start()

        await ready()

        #expect(memory.splits(of: id(1)) == [id(11): split(31), id(23): split(43, guild: 3)])
        session.close()
    }

    // Discord's key handler (canary 646574, module 92960) counts only what to load or ack: a
    // scrolled-up list with neither closes the split, without a scroll.
    @Test(arguments: [11, 31])
    func escapeWithAListScrolledUpAndNothingToDoClosesTheSplit(pane: UInt64) async throws {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        let list = try #require(pane == 11 ? session.messages : session.split)

        list.setAtBottom(false)

        #expect(!session.escape())
        #expect(session.split == nil)
        #expect(account.markedRead.current.isEmpty)
        session.close()
    }

    // Its local ack clears both dividers in the press that closes the split.
    @Test
    func escapeClearsTheLinesAndClosesTheSplitInOnePress() async throws {
        world()
        store.update { state in
            state.show(message(2, in: 11, by: user(2)), message(4, in: 31, by: user(2)))
            state.readStates[id(11)] = ReadState(acked: id(1), mentions: 0, estimated: false)
            state.readStates[id(31)] = ReadState(acked: id(3), mentions: 0, estimated: false)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        await session.openBeside(thread: id(31), from: id(11))
        let channel = try #require(session.messages)
        let thread = try #require(session.split)
        #expect(channel.newMessagesLine == id(2))
        #expect(thread.newMessagesLine == id(4))

        #expect(!session.escape())

        #expect(session.split == nil)
        #expect(channel.newMessagesLine == nil)
        #expect(thread.newMessagesLine == nil)
        #expect(account.markedRead.current.isEmpty, "nothing unread to ack")
        session.close()
    }

    @Test
    func aSplitShownAfterAFetchWaitsForAClick() async {
        world()
        thread33()
        let session = relaunched(splits: [id(11): split(33)])

        await ready()
        await finished(session.splitFetch)
        await settle(session)

        #expect(session.split?.channelId == id(33))
        #expect(session.reading == [id(11)])
        session.userInteracted()
        #expect(session.reading == [id(11), id(33)])
        session.close()
    }

    @Test
    func aSplitABatchBringsWaitsForAClick() async {
        world()
        account.holdThreadFetches.withLock { $0 = true }
        let session = relaunched(splits: [id(11): split(33)])
        await ready()

        store.update { state in
            state.channels[id(33)] = thread(33, in: 11, name: "t3")
        }
        subscription.send(.channelAdded(channelId: id(33), guildId: id(1)))
        await subscription.batches.pulled(3)
        await settle(session)

        #expect(session.split?.channelId == id(33))
        #expect(session.reading == [id(11)])
        account.threadFetches.send(())
        session.close()
    }

    @Test
    func aSplitBackWithItsChannelAtLaunchIsRead() async {
        world(joined: true, listed: true)
        let session = relaunched(splits: [id(11): split(31)])

        await ready()
        await settle(session)

        #expect(session.reading == [id(11), id(31)])
        #expect(threadRequests(31) == 0, "READY listed it")
        session.close()
    }
}
