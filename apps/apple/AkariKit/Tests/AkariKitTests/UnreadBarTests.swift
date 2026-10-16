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

    var aroundLoad: CallLog.Call { .load(here, .around(messageId: id(2), limit: 50)) }
    var latestLoad: CallLog.Call { .load(here, .latest(limit: 50)) }
    var loads: [CallLog.Call] {
        store.log.actions.filter { if case .load = $0 { true } else { false } }
    }

    // The window each load leaves, as the core would; `latest` gives one per latest load. The
    // fake drops an around page that something placed after, as the core does.
    func windows(around: MessageWindow?, latest: [MessageWindow] = []) {
        let pages = Locked(latest)
        account.windowAfter.withLock {
            $0 = { load in
                switch load {
                case .around: return around
                case .latest: return pages.withLock { $0.isEmpty ? nil : $0.removeFirst() }
                default: return nil
                }
            }
        }
    }

    func ids(_ model: MessageListModel) -> [UInt64] {
        model.rows.map(\.id.rawValue)
    }

    func inHistory(_ ids: [UInt64], readUpTo acked: UInt64) {
        store.update { [here] state in
            state.windows[here] = window(ids, latest: false)
            state.readStates[here] = readState(acked)
        }
    }

    // The first unread message isn't loaded; the jump's load waits for a reply.
    func waitingJump(_ model: MessageListModel) async -> Task<Void, Never> {
        model.setNewMessagesLineOnScreen(false)
        #expect(model.unreadBar != nil)
        let jump = Task { await model.jumpToUnread() }
        await account.loadReplies.pulled(1)
        return jump
    }

    @Test
    func escapeSupersedesARunningAroundLoad() async {
        shown([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        windows(around: window([1, 2, 3, 4, 5, 6], latest: false))
        let model = makeModel()
        let jump = await waitingJump(model)

        await model.escape()
        account.loadReplies.send(nil)
        await finished(jump)

        #expect(store.log.actions == [aroundLoad, .supersede(here)])
        #expect(ids(model) == [5, 6, 7, 8], "the around page doesn't show")
        #expect(model.atPresent)
        #expect(model.unreadJumps == 0)
        #expect(model.newMessagesLine == nil)
        #expect(model.loading == nil)
    }

    @Test
    func jumpToPresentSupersedesARunningAroundLoad() async {
        inHistory([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        windows(around: window([1, 2, 3, 4, 5, 6], latest: false), latest: [window([7, 8])])
        let model = makeModel()
        let jump = await waitingJump(model)

        let present = Task { await model.jumpToPresent() }
        await account.loadReplies.pulled(2)
        account.loadReplies.send(nil)
        await finished(jump)
        #expect(ids(model) == [5, 6, 7, 8], "the around page doesn't show")
        account.loadReplies.send(nil)
        await finished(present)

        #expect(store.log.actions == [aroundLoad, latestLoad])
        #expect(ids(model) == [7, 8])
        #expect(model.atPresent)
        #expect(model.unreadJumps == 0)
        #expect(model.loading == nil)
    }

    // The bar's "Mark As Read" while the jump loads, scrolled up into older messages: the list
    // stays where it is.
    @Test
    func markingReadSupersedesARunningAroundLoad() async throws {
        store.update { state in
            state.add(guild(1))
            state.list([channel(10)], in: 1)
        }
        inHistory([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        windows(around: window([1, 2, 3, 4, 5, 6], latest: false))
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        let model = try #require(session.messages)
        let jump = await waitingJump(model)

        session.markRead(channel: here)
        account.loadReplies.send(nil)
        await finished(jump)

        #expect(loads == [aroundLoad])
        #expect(store.log.actions.contains(.supersede(here)))
        #expect(ids(model) == [5, 6, 7, 8])
        #expect(!model.atPresent)
        #expect(model.unreadJumps == 0)
        #expect(model.unreadBar == nil)
        session.close()
    }

    // The jump to the present overtook the jump, and a second click came while it loaded.
    @Test
    func anOvertakenJumpLeavesTheBarToClickAgain() async {
        inHistory([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        windows(around: window([1, 2, 3, 4, 5, 6], latest: false), latest: [window([7, 8])])
        let model = makeModel()
        let jump = await waitingJump(model)
        let present = Task { await model.jumpToPresent() }
        await account.loadReplies.pulled(2)
        await model.jumpToUnread()
        account.loadReplies.send(nil)
        await finished(jump)
        account.loadReplies.send(nil)
        await finished(present)
        #expect(ids(model) == [7, 8])
        #expect(model.unreadJumps == 0)
        #expect(model.unreadBar != nil)

        account.holdLoads.withLock { $0 = false }
        await model.jumpToUnread()

        #expect(loads == [aroundLoad, latestLoad, aroundLoad])
        #expect(ids(model) == [1, 2, 3, 4, 5, 6])
        #expect(model.unreadJumps == 1)
        #expect(model.unreadBar == nil)
    }

    // The around page reached the list before its load returned, and the user clicked again.
    @Test
    func aSecondClickWhileTheJumpLoadsEndsAtTheLine() async {
        shown([5, 6, 7, 8], readUpTo: 2)
        account.holdLoads.withLock { $0 = true }
        let around = window([1, 2, 3, 4, 5, 6], latest: false)
        windows(around: around)
        let model = makeModel()
        let jump = await waitingJump(model)
        store.update { [here] in $0.windows[here] = around }
        model.apply(EventBatch([.messagesLoaded(channelId: here, first: id(1), last: id(4))]))

        await model.jumpToUnread()
        account.loadReplies.send(nil)
        await finished(jump)

        #expect(loads == [aroundLoad])
        #expect(ids(model) == [1, 2, 3, 4, 5, 6])
        #expect(!model.atPresent)
        #expect(model.newMessagesLine == id(3))
        #expect(model.unreadJumps == 2)
        #expect(model.unreadBar == nil)
    }

    // A newer page replaced the window while it loaded, so the core dropped the around page.
    @Test
    func aJumpWhosePageTheCoreDroppedKeepsTheBar() async throws {
        shown([5, 6, 7, 8], readUpTo: 2)
        windows(around: nil)
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let bar = try #require(model.unreadBar)

        await model.jumpToUnread()

        #expect(store.log.actions == [aroundLoad])
        #expect(model.unreadJumps == 0)
        #expect(model.unreadBar == bar)
    }

    // Nothing at or before the read position is left, e.g. deleted: the page starts after it.
    @Test
    func aJumpCountsOnceItsPageIsInWithoutTheReadPosition() async {
        store.update { [mira] state in
            for raw in UInt64(10)...110 {
                state.show(message(raw, by: mira))
            }
        }
        shown(Array(60...110), readUpTo: 2)
        account.windowAfterLoad.withLock { $0 = window(Array(10...59), latest: false) }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)

        await model.jumpToUnread()

        #expect(store.log.actions == [aroundLoad])
        #expect(model.newMessagesLine == id(10))
        #expect(model.unreadJumps == 1)
        #expect(model.unreadBar == nil)
    }

    @Test
    func aFailedAroundLoadKeepsTheBar() async throws {
        shown([5, 6, 7, 8], readUpTo: 2)
        account.loadError.withLock { $0 = .Network(kind: .timeout) }
        let model = makeModel()
        model.setNewMessagesLineOnScreen(false)
        let bar = try #require(model.unreadBar)

        await model.jumpToUnread()

        #expect(model.unreadJumps == 0)
        #expect(model.unreadBar == bar, "the user can click it again")
        #expect(model.failedLoads == [.around: .Network(kind: .timeout)])
        #expect(model.lastLoad?.failure == .Network(kind: .timeout))

        account.loadError.withLock { $0 = nil }
        account.windowAfterLoad.withLock { $0 = window([1, 2, 3, 4, 5, 6], latest: false) }
        await model.jumpToUnread()

        #expect(store.log.actions == [aroundLoad, aroundLoad])
        #expect(model.unreadJumps == 1)
        #expect(model.unreadBar == nil)
        #expect(model.failedLoads.isEmpty)
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
