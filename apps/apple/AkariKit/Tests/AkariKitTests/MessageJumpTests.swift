import Foundation
import Testing

@testable import AkariKit

// "a message" in a pin's line jumps to the pinned message, as the unread bar jumps to its line.
@MainActor @Suite(.timeLimit(.minutes(1)))
struct MessageJumpTests {
    let store = FakeStore()
    let account: FakeAccount
    let here: ChannelId = id(10)
    let mira = user(2, name: "mira")

    // Messages 1...10; the list shows 5...8, read up to 8.
    init() {
        account = FakeAccount(store: store)
        store.update { [mira] state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            for raw in UInt64(1)...10 {
                state.show(message(raw, by: mira))
            }
            state.windows[id(10)] = window([5, 6, 7, 8])
            state.readStates[id(10)] = ReadState(acked: id(8), mentions: 0, estimated: false)
        }
        // Each load's own page: around a message the two on each side, the latest 7...10.
        account.windowAfter.withLock {
            $0 = { load in
                switch load {
                case .around(let around, _):
                    let raw = around.rawValue
                    return window(Array(max(3, raw) - 2...min(10, raw + 2)), latest: raw + 2 >= 10)
                case .latest: return window([7, 8, 9, 10])
                default: return nil
                }
            }
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: here, userId: id(1), account: account, store: store)
    }

    func ids(_ model: MessageListModel) -> [UInt64] {
        model.rows.map(\.id.rawValue)
    }

    func readUpTo(_ raw: UInt64) {
        store.update { [here] in
            $0.readStates[here] = ReadState(acked: id(raw), mentions: 0, estimated: false)
        }
    }

    func around(_ raw: UInt64) -> CallLog.Call {
        .load(here, .around(messageId: id(raw), limit: 50))
    }

    var latest: CallLog.Call { .load(here, .latest(limit: 50)) }

    var loads: [CallLog.Call] {
        store.log.actions.filter { if case .load = $0 { true } else { false } }
    }

    // Each held load waits for a reply; `count` loads are waiting once it returns.
    func started(_ count: Int, _ action: @escaping @MainActor () async -> Void) async -> Task<
        Void, Never
    > {
        let task = Task { await action() }
        await account.loadReplies.pulled(count)
        return task
    }

    func reply(_ count: Int = 1) {
        for _ in 0..<count {
            account.loadReplies.send(nil)
        }
    }

    @Test
    func aLoadedMessageIsJumpedToAtOnce() async {
        let model = makeModel()

        await model.jump(to: id(6))

        #expect(store.log.actions.isEmpty)
        #expect(model.messageJump == .init(messageId: id(6), number: 1))
    }

    @Test
    func jumpingToItAgainJumpsAgain() async {
        let model = makeModel()

        await model.jump(to: id(6))
        await model.jump(to: id(6))

        #expect(model.messageJump == .init(messageId: id(6), number: 2))
    }

    @Test
    func aMessageOlderThanTheLoadedOnesIsLoadedAroundFirst() async {
        let model = makeModel()

        await model.jump(to: id(2))

        #expect(store.log.actions == [around(2)])
        #expect(ids(model) == [1, 2, 3, 4])
        #expect(!model.atPresent)
        #expect(model.messageJump == .init(messageId: id(2), number: 1))
        #expect(model.loading == nil)
    }

    // Deleted since it was pinned: its neighbors are loaded already, and the view goes to the
    // next one.
    @Test
    func aGoneMessageAmongTheLoadedOnesNeedsNoLoad() async {
        store.update { [here] in $0.windows[here] = window([4, 5, 7, 8]) }
        let model = makeModel()

        await model.jump(to: id(6))

        #expect(store.log.actions.isEmpty)
        #expect(model.messageJump == .init(messageId: id(6), number: 1))
    }

    @Test
    func aFailedLoadLeavesTheListWhereItIs() async {
        account.loadError.withLock { $0 = .Network(kind: .timeout) }
        let model = makeModel()

        await model.jump(to: id(2))

        #expect(model.messageJump == nil)
        #expect(ids(model) == [5, 6, 7, 8])
        #expect(model.atPresent)
        #expect(
            model.failedLoads == [.aroundMessage: .Network(kind: .timeout)],
            "the list's edges show no error")
    }

    @Test
    func aSecondClickWhileItsJumpLoadsAsksOnce() async {
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        let first = await started(1) { await model.jump(to: id(2)) }

        await model.jump(to: id(2))
        reply()
        await finished(first)

        #expect(loads == [around(2)])
        #expect(model.messageJump == .init(messageId: id(2), number: 1))
    }

    @Test
    func escapeCancelsARunningJump() async {
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        let jump = await started(1) { await model.jump(to: id(2)) }

        await model.escape()
        reply()
        await finished(jump)

        #expect(store.log.actions == [around(2), .supersede(here)])
        #expect(model.messageJump == nil)
        #expect(ids(model) == [5, 6, 7, 8], "the around page doesn't show")
        #expect(model.atPresent)
    }

    @Test
    func theJumpToPresentCancelsARunningJump() async {
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        let jump = await started(1) { await model.jump(to: id(2)) }
        let present = await started(2) { await model.jumpToPresent() }

        reply(2)
        await finished(jump)
        await finished(present)

        #expect(loads == [around(2), latest])
        #expect(model.messageJump == nil)
        #expect(ids(model) == [7, 8, 9, 10])
        #expect(model.atPresent)
    }

    // As the session's mark as read, a send and a channel switch end it.
    @Test
    func markingReadAndLeavingCancelARunningJump() async {
        for end in [{ (model: MessageListModel) in model.endNewMessagesLine() }, { $0.retire() }] {
            store.update { [here] in $0.windows[here] = window([5, 6, 7, 8]) }
            account.holdLoads.withLock { $0 = true }
            let model = makeModel()
            let jump = await started(1) { await model.jump(to: id(2)) }

            end(model)
            reply()
            await finished(jump)

            #expect(model.messageJump == nil)
            #expect(ids(model) == [5, 6, 7, 8])
        }
    }

    // The unread bar's jump doesn't wait for the message jump's load, which it overtakes.
    @Test
    func theUnreadBarDuringAJumpEndsAtTheLine() async {
        readUpTo(3)
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let jump = await started(1) { await model.jump(to: id(2)) }

        let unread = await started(2) { await model.jumpToUnread() }
        reply(2)
        await finished(jump)
        await finished(unread)

        #expect(loads == [around(2), around(3)])
        #expect(ids(model) == [1, 2, 3, 4, 5])
        #expect(model.messageJump == nil)
        #expect(model.unreadJumps == 1)
    }

    // The line is loaded, but the jump's page would land after the bar's scroll.
    @Test
    func theUnreadBarDuringAJumpEndsAtTheLineAlsoWhenItsLoaded() async {
        readUpTo(6)
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        #expect(model.newMessagesLine == id(7))
        let jump = await started(1) { await model.jump(to: id(2)) }

        let unread = await started(2) { await model.jumpToUnread() }
        reply(2)
        await finished(jump)
        await finished(unread)

        #expect(loads == [around(2), around(6)])
        #expect(ids(model) == [4, 5, 6, 7, 8])
        #expect(model.messageJump == nil)
        #expect(model.unreadJumps == 1)
    }

    // Nothing read yet: no position to load around, so the jump's page is dropped instead.
    @Test
    func aNeverReadChannelsBarDuringAJumpEndsAtTheLine() async {
        store.update { [here] in
            $0.readStates[here] = ReadState(acked: nil, mentions: 0, estimated: false)
        }
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        #expect(model.newMessagesLine == id(5))
        let jump = await started(1) { await model.jump(to: id(2)) }

        await model.jumpToUnread()
        reply()
        await finished(jump)

        #expect(store.log.actions == [around(2), .supersede(here)])
        #expect(ids(model) == [5, 6, 7, 8])
        #expect(model.messageJump == nil)
        #expect(model.unreadJumps == 1)
    }

    // The user's last request places the list, also when its message is loaded: the running
    // load would replace the rows it scrolled.
    @Test
    func aJumpDuringTheUnreadBarsJumpEndsAtTheMessage() async {
        readUpTo(3)
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let unread = await started(1) { await model.jumpToUnread() }

        let jump = await started(2) { await model.jump(to: id(7)) }
        reply(2)
        await finished(unread)
        await finished(jump)

        #expect(loads == [around(3), around(7)])
        #expect(ids(model) == [5, 6, 7, 8, 9])
        #expect(model.unreadJumps == 0)
        #expect(model.messageJump == .init(messageId: id(7), number: 1))
    }

    // The core already put the bar's page in the window; the list hasn't heard yet.
    @Test
    func aJumpAfterTheUnreadPageLandedInTheCoreEndsAtTheMessage() async {
        readUpTo(3)
        account.holdLoads.withLock { $0 = true }
        account.aroundLandsFirst.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let unread = await started(1) { await model.jumpToUnread() }

        let jump = await started(2) { await model.jump(to: id(7)) }
        // Both pages land; in this order.
        reply()
        await finished(unread)
        reply()
        await finished(jump)

        #expect(ids(model) == [5, 6, 7, 8, 9])
        #expect(model.unreadJumps == 0)
        #expect(model.messageJump == .init(messageId: id(7), number: 1))
    }

    // In history, the jump to the present loads; a click on a loaded pin then wins.
    @Test
    func aJumpDuringTheJumpToPresentEndsAtTheMessage() async {
        store.update { [here] in $0.windows[here] = window([5, 6, 7, 8], latest: false) }
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        let present = await started(1) { await model.jumpToPresent() }

        let jump = await started(2) { await model.jump(to: id(6)) }
        reply(2)
        await finished(present)
        await finished(jump)

        #expect(loads == [latest, around(6)])
        #expect(ids(model) == [4, 5, 6, 7, 8])
        #expect(!model.atPresent)
        #expect(model.messageJump == .init(messageId: id(6), number: 1))
    }

    // The bar's jump overtook the jump to the present, which is owed should the bar's jump not
    // land; a jump to a loaded pin then places the list, and nothing owes the present.
    @Test
    func aJumpAfterAnOvertakenJumpToPresentStaysAtTheMessage() async {
        store.update { [here] in $0.windows[here] = window([5, 6, 7, 8], latest: false) }
        readUpTo(2)
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let present = await started(1) { await model.jumpToPresent() }
        let unread = await started(2) { await model.jumpToUnread() }
        reply()
        await finished(present)

        let jump = await started(3) { await model.jump(to: id(6)) }
        reply(2)
        await finished(unread)
        await finished(jump)

        #expect(loads == [latest, around(2), around(6)])
        #expect(ids(model) == [4, 5, 6, 7, 8])
        #expect(model.unreadJumps == 0)
        #expect(model.messageJump == .init(messageId: id(6), number: 1))
    }

    @Test
    func aJumpDuringAnotherEndsAtTheLaterMessage() async {
        account.holdLoads.withLock { $0 = true }
        let model = makeModel()
        let first = await started(1) { await model.jump(to: id(3)) }

        let second = await started(2) { await model.jump(to: id(1)) }
        reply(2)
        await finished(first)
        await finished(second)

        #expect(loads == [around(3), around(1)], "the overtaken jump loads nothing more")
        #expect(ids(model) == [1, 2, 3])
        #expect(model.messageJump == .init(messageId: id(1), number: 1))
    }
}
