import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct UnreadBarTests {
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

    func shown(_ ids: [UInt64], readUpTo acked: UInt64?, oldest: Bool = false) {
        store.update { [here] state in
            state.windows[here] = window(ids, oldest: oldest)
            state.readStates[here] = readState(acked)
        }
    }

    func arrive(_ raw: UInt64, in model: MessageListModel) {
        store.update { [here, mira] state in
            state.show(message(raw, by: mira))
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

    func time(of raw: UInt64) -> Date {
        message(raw).timestamp
    }

    // A new list, its line placed out of sight.
    func bar() -> MessageListModel.UnreadBar? {
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        return model.unreadBar
    }

    // The list opens at the bottom, with the line above the view.
    @Test
    func theBarShowsWhenTheFirstUnreadIsAboveTheView() throws {
        shown([1, 2, 3, 4, 5, 6, 7, 8], readUpTo: 2)
        let model = makeModel()
        #expect(model.atBottom)
        #expect(model.unreadBar == nil, "until the view has placed the line")

        model.setNewMessagesLineOnScreen(false)

        let bar = try #require(model.unreadBar)
        #expect(bar.count == 6)
        #expect(bar.since == time(of: 3))
    }

    // Scrolled up a screen, one message arrives below the view (Q12d).
    @Test
    func theBarShowsForAnUnreadMessageBelowTheView() throws {
        shown([1, 2, 3, 4], readUpTo: 4)
        let model = makeModel()
        model.setAtBottom(false)
        #expect(model.unreadBar == nil)

        arrive(5, in: model)
        model.setNewMessagesLineOnScreen(false)

        let bar = try #require(model.unreadBar)
        #expect(bar.count == 1)
        #expect(bar.since == time(of: 5))
    }

    // Three unread messages, all visible at open (Q14).
    @Test
    func noBarWhenTheFirstUnreadIsOnScreen() {
        shown([1, 2, 3, 4, 5], readUpTo: 2)
        let model = makeModel()

        model.setNewMessagesLineOnScreen(true)

        #expect(model.newMessagesLine == id(3))
        #expect(model.unreadBar == nil)
    }

    @Test
    func theBarGoesAwayOnceTheLineWasSeen() async {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        #expect(model.unreadBar != nil)

        model.setNewMessagesLineOnScreen(true)
        model.setAtBottom(false)
        model.setNewMessagesLineOnScreen(false)
        arrive(5, in: model)
        #expect(model.newMessagesLine == id(3))
        #expect(model.unreadBar == nil, "the same line stays seen")

        await model.escape()
        arrive(6, in: model)
        model.setNewMessagesLineOnScreen(false)
        #expect(model.newMessagesLine == id(6))
        #expect(model.unreadBar?.count == 1, "a new line hasn't been seen")
    }

    @Test
    func theBarSaysHowManyAndSince() throws {
        shown([1, 2, 3], readUpTo: 2)
        let one = try #require(bar())
        #expect(one.count == 1)
        #expect(!one.more)
        #expect(one.since == time(of: 3))
        #expect(one.text(time: "17:13") == "1 new message since 17:13")

        // The user's own messages after the line aren't new.
        store.update { [me] in $0.show(message(5, by: me)) }
        shown([1, 2, 3, 4, 5, 6, 7, 8], readUpTo: 2)
        let several = try #require(bar())
        #expect(several.count == 5)
        #expect(several.since == time(of: 3))
        #expect(several.text(time: "17:13") == "5 new messages since 17:13")

        // The read position is older than every loaded message: since its own time.
        let position: UInt64 = 60_000 << 22
        store.update { [mira] state in
            for raw in position + 1...position + 4 {
                state.show(message(raw, by: mira))
            }
        }
        shown(Array(position + 1...position + 4), readUpTo: position)
        let more = try #require(bar())
        #expect(more.count == 4)
        #expect(more.more)
        #expect(more.since == Date(timeIntervalSince1970: 1_420_070_460))
        #expect(more.text(time: "17:13") == "4+ new messages since 17:13")

        // From the channel's first message on, every unread message is loaded.
        shown(Array(position + 1...position + 4), readUpTo: position, oldest: true)
        let all = try #require(bar())
        #expect(!all.more)
        #expect(all.since == time(of: position + 1))
        #expect(all.text(time: "17:13") == "4 new messages since 17:13")

        // Never read: there's no position to tell the time of.
        shown([3, 4, 5, 6], readUpTo: nil)
        store.update { [mira] in $0.show(message(5, by: mira)) }
        let never = try #require(bar())
        #expect(never.more)
        #expect(never.since == time(of: 3))
    }

    @Test
    func clickingTheBarJumpsToTheLine() async {
        shown([1, 2, 3, 4, 5, 6, 7, 8], readUpTo: 2)
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)

        await model.jumpToUnread()

        #expect(store.log.actions.isEmpty, "the line's message is loaded")
        #expect(model.unreadJumps == 1)
        #expect(model.unreadBar == nil)

        // The first unread message is older than the loaded ones.
        store.update { [here] in $0.windows[here] = window([5, 6, 7, 8]) }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let far = makeModel()
        far.setNewMessagesLineOnScreen(false)
        #expect(far.newMessagesLine == id(5))
        #expect(far.unreadBar?.more == true)

        await far.jumpToUnread()

        #expect(store.log.actions == [.load(here, .around(messageId: id(2), limit: 50))])
        #expect(far.newMessagesLine == id(3))
        #expect(far.unreadJumps == 1)
        #expect(far.unreadBar == nil)
        #expect(far.loading == nil)
    }

    // Nothing read yet: there is no position to load around.
    @Test
    func aChannelNeverReadJumpsToItsFirstLoadedUnreadMessage() async {
        shown([5, 6, 7, 8], readUpTo: nil)
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        #expect(model.newMessagesLine == id(5))

        await model.jumpToUnread()

        #expect(store.log.actions.isEmpty)
        #expect(model.unreadJumps == 1)
        #expect(model.newMessagesLine == id(5))
        #expect(model.unreadBar == nil)
    }

    @Test
    func jumpingAgainWhileTheFirstUnreadLoadsStartsNoSecondLoad() async {
        shown([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        let model = makeModel()

        async let first: Void = model.jumpToUnread()
        await account.loadReplies.pulled(1)
        await model.jumpToUnread()
        #expect(model.unreadJumps == 0, "the first jump goes once its messages are in")
        account.loadReplies.send(nil)
        await first

        #expect(store.log.actions == [.load(here, .around(messageId: id(2), limit: 50))])
        #expect(model.unreadJumps == 1)
        #expect(model.newMessagesLine == id(3))
    }

    @Test
    func theBarsMarkAsReadMarksRead() throws {
        store.update { state in
            state.add(guild(1))
            state.list([channel(10)], in: 1)
        }
        shown([1, 2, 3, 4], readUpTo: 2)
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        let list = try #require(session.messages)
        list.setNewMessagesLineOnScreen(false)
        #expect(list.unreadBar != nil)

        session.markRead(channel: here)

        #expect(account.markedRead.current == [here])
        #expect(list.unreadBar == nil)
        #expect(list.unreadJumps == 0, "it doesn't jump")
        session.close()
    }

    // Also once the bar has gone: the line is still there to jump to.
    @Test
    func shiftPageUpJumpsToTheFirstUnread() async {
        shown([1, 2, 3, 4], readUpTo: 2)
        let model = makeModel()
        model.setNewMessagesLineOnScreen(true)
        #expect(model.unreadBar == nil)

        await model.jumpToUnread()
        await model.jumpToUnread()

        #expect(model.unreadJumps == 2)
        #expect(store.log.actions.isEmpty)
    }

    @Test
    func shiftPageUpWithoutALineDoesNothing() async {
        shown([5, 6, 7, 8], readUpTo: 8)
        let model = makeModel()
        #expect(model.newMessagesLine == nil)

        await model.jumpToUnread()

        #expect(model.unreadJumps == 0)
        #expect(store.log.actions.isEmpty)
    }
}
