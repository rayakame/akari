import AkariKit
import AppKit
import Testing

@testable import Akari

// The table's scrolls reach a real session through the app's bridge, as in the message area.
@MainActor
struct EngagementTests {
    let list = MessageTableTests()
    let session: SessionModel
    let channel: ChannelId
    let bridge: MessageListBridge

    init() throws {
        session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(CrowdedStore.guildIds[0]))
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        let messages = try #require(session.messages)
        channel = messages.channelId
        bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in },
            markServerRead: {})
        list.controller.actions = bridge
        list.show(list.page)
        #expect(session.reading == channel)
    }

    var bottomClip: BottomClipView? { list.clip as? BottomClipView }

    // A scroll by the user, positive toward older messages.
    func nudge(by distance: CGFloat) {
        list.clip.scroll(to: NSPoint(x: 0, y: list.clip.bounds.minY - distance))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()
    }

    func scrollToBottom() throws {
        let bottom = try #require(bottomClip).originRange.upperBound
        nudge(by: list.clip.bounds.minY - bottom)
    }

    @Test
    func aTrackpadNudgeAtTheBottomReportsNoScrollAway() throws {
        nudge(by: 40)
        #expect(session.reading == nil, "off the bottom")
        nudge(by: -10)
        nudge(by: 40)
        try scrollToBottom()
        #expect(session.reading == channel, "still engaged")

        list.scroll(toRowOf: 120)
        try scrollToBottom()
        #expect(session.reading == nil, "scrolled clearly away")
        session.userInteracted()
        #expect(session.reading == channel)
    }

    var tallMessage: MessageListModel.Row {
        row(
            201, by: author(2), at: list.noon + 201 * 120,
            content: String(repeating: "words that wrap ", count: 80))
    }

    // Rows below wait for the gesture to end; only the user's own movement unpins.
    @Test
    func aMessageArrivingMidGestureKeepsEngagement() throws {
        list.postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        list.show(list.page + [tallMessage])
        nudge(by: 5)
        nudge(by: -3)
        #expect(session.reading == channel, "still pinned")

        list.postLiveScroll(NSScrollView.didEndLiveScrollNotification)

        #expect(list.clip.bounds.minY == (try #require(bottomClip)).originRange.upperBound)
        #expect(session.reading == channel)
    }

    // A short arrival leaves the list pinned without a scroll when the gesture ends. A nudge that
    // unpins against the rows now, but not against the bottom held before them, must unpin.
    @Test
    func theHeldBottomEndsWithTheGesture() throws {
        let slack = list.controller.scrollView.contentInsets.bottom + 2
        let before = list.table.contentHeight
        list.postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        list.show(list.page + [row(201, by: author(1), at: list.noon + 201 * 120)])
        let grown = list.table.contentHeight - before
        try #require(grown > 3 && grown <= slack)
        list.postLiveScroll(NSScrollView.didEndLiveScrollNotification)

        nudge(by: slack - grown / 2)

        #expect(session.messages?.atBottom == false)
        #expect(session.reading == nil)
    }

    @Test
    func rowsRemovedMidGestureKeepTheListPinned() throws {
        list.postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        list.show(list.page + [tallMessage])
        list.show(Array(list.page.dropLast(3)))
        try scrollToBottom()
        nudge(by: -3)

        list.postLiveScroll(NSScrollView.didEndLiveScrollNotification)

        #expect(session.messages?.atBottom == true)
        #expect(session.reading == channel)
    }

    @Test
    func scrollingBackFromAnArrivalMidGestureUnpins() throws {
        list.postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        list.show(list.page + [tallMessage])
        try scrollToBottom()
        nudge(by: 100)
        #expect(session.messages?.atBottom == false)

        list.postLiveScroll(NSScrollView.didEndLiveScrollNotification)

        #expect(list.clip.bounds.minY < (try #require(bottomClip)).originRange.upperBound)
    }

    @Test
    func movingTowardOlderMessagesMidGestureStillUnpins() throws {
        list.postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        list.show(list.page + [tallMessage])
        nudge(by: 40)
        #expect(session.reading == nil)

        list.postLiveScroll(NSScrollView.didEndLiveScrollNotification)

        #expect(list.clip.bounds.minY < (try #require(bottomClip)).originRange.upperBound)
    }

    @Test
    func theBarsJumpKeepsEngagement() {
        let line = MessageId(rawValue: 150)
        list.show(MessageTableState(rows: list.page, newMessages: line))
        list.show(MessageTableState(rows: list.page, newMessages: line, unreadJumps: 1))
        #expect(!list.visibleRows.contains(list.table.numberOfRows - 1))
        #expect(session.reading == nil)

        list.controller.jumpToPresent(load: false)

        #expect(session.reading == channel)
    }
}
