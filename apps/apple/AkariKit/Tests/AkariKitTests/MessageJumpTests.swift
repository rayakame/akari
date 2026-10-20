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

    init() {
        account = FakeAccount(store: store)
        store.update { [mira] state in
            state.currentUser = user(1)
            state.channels[id(10)] = channel(10, guild: 1)
            state.permissions[id(10)] = [.viewChannel, .sendMessages]
            for raw in UInt64(1)...8 {
                state.show(message(raw, by: mira))
            }
            state.windows[id(10)] = window([5, 6, 7, 8])
            state.readStates[id(10)] = ReadState(acked: id(8), mentions: 0, estimated: false)
        }
    }

    func makeModel() -> MessageListModel {
        MessageListModel(channelId: here, userId: id(1), account: account, store: store)
    }

    func ids(_ model: MessageListModel) -> [UInt64] {
        model.rows.map(\.id.rawValue)
    }

    var aroundTwo: CallLog.Call { .load(here, .around(messageId: id(2), limit: 50)) }

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
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()

        await model.jump(to: id(2))

        #expect(store.log.actions == [aroundTwo])
        #expect(ids(model) == [1, 2, 3, 4, 5, 6])
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
    func escapeCancelsARunningJump() async {
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()
        let jump = Task { await model.jump(to: id(2)) }
        await account.loadReplies.pulled(1)

        await model.escape()
        account.loadReplies.send(nil)
        await finished(jump)

        #expect(store.log.actions == [aroundTwo, .supersede(here)])
        #expect(model.messageJump == nil)
        #expect(ids(model) == [5, 6, 7, 8], "the around page doesn't show")
        #expect(model.atPresent)
    }

    // The unread bar's jump doesn't wait for the message jump's load, which it overtakes.
    @Test
    func theUnreadBarDuringAJumpEndsAtTheLine() async {
        store.update { [here] in
            $0.readStates[here] = ReadState(acked: id(3), mentions: 0, estimated: false)
        }
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let jump = Task { await model.jump(to: id(2)) }
        await account.loadReplies.pulled(1)

        let unread = Task { await model.jumpToUnread() }
        await account.loadReplies.pulled(2)
        account.loadReplies.send(nil)
        account.loadReplies.send(nil)
        await finished(jump)
        await finished(unread)

        #expect(
            store.log.actions == [aroundTwo, .load(here, .around(messageId: id(3), limit: 50))])
        #expect(model.messageJump == nil)
        #expect(model.unreadJumps == 1)
    }

    // The user's last request places the list, also when it needs no load.
    @Test
    func aJumpToALoadedMessageCancelsARunningUnreadJump() async {
        store.update { [here] in
            $0.readStates[here] = ReadState(acked: id(3), mentions: 0, estimated: false)
        }
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let unread = Task { await model.jumpToUnread() }
        await account.loadReplies.pulled(1)

        await model.jump(to: id(7))
        account.loadReplies.send(nil)
        await finished(unread)

        #expect(
            store.log.actions == [
                .load(here, .around(messageId: id(3), limit: 50)), .supersede(here),
            ])
        #expect(model.unreadJumps == 0)
        #expect(ids(model) == [5, 6, 7, 8])
        #expect(model.messageJump == .init(messageId: id(7), number: 1))
    }

    @Test
    func aJumpDuringAnotherEndsAtTheLaterMessage() async {
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()
        let first = Task { await model.jump(to: id(2)) }
        await account.loadReplies.pulled(1)

        let second = Task { await model.jump(to: id(1)) }
        await account.loadReplies.pulled(2)
        account.loadReplies.send(nil)
        account.loadReplies.send(nil)
        await finished(first)
        await finished(second)

        #expect(model.messageJump == .init(messageId: id(1), number: 1))
    }
}
