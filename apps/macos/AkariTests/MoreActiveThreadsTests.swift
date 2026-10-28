import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Discord's channel row (canary 646904, chunk 711a24515900e496): the threads popout shows 200 ms
// after the pointer enters a row, and goes 250 ms after it leaves.
@MainActor
struct ThreadsHoverTests {
    // Each wait of the hover suspends here until the test releases it.
    final class Gate: @unchecked Sendable {
        var waits: [Duration] = []
        private var waiting: [CheckedContinuation<Void, Never>] = []
        private var released = 0
        private var returned = 0

        func wait(_ duration: Duration) async {
            await withCheckedContinuation { continuation in
                waits.append(duration)
                waiting.append(continuation)
            }
            returned += 1
        }

        // Ends every wait so far and returns once each is back in its hover, which acts in that
        // same turn; how many turns that takes depends on the machine's load.
        func release() async {
            let resumed = waiting
            waiting = []
            released += resumed.count
            resumed.forEach { $0.resume() }
            await until { self.returned == self.released }
        }

        func waitStarted(_ count: Int) async {
            await until { self.waits.count >= count }
        }

        private func until(_ condition: () -> Bool) async {
            let deadline = ContinuousClock.now + .seconds(5)
            while !condition(), ContinuousClock.now < deadline {
                await Task.yield()
            }
        }
    }

    let gate = Gate()
    let general = ChannelId(rawValue: 11)

    func hover() -> ThreadsHover {
        ThreadsHover(wait: { [gate] in await gate.wait($0) })
    }

    @Test
    func itShowsAfter200msOnlyForAChannelWithThreads() async {
        let hover = hover()

        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        #expect(hover.channel == nil)
        await gate.release()

        #expect(gate.waits == [.milliseconds(200)])
        #expect(hover.channel == general)

        let other = self.hover()
        other.hover(general, true) { false }
        await gate.waitStarted(2)
        await gate.release()
        #expect(other.channel == nil)
    }

    @Test
    func leavingTheRowHidesItAfter250msUnlessThePointerIsOnThePopout() async {
        let hover = hover()
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.hover(general, false) { true }
        await gate.waitStarted(2)
        #expect(hover.channel == general)
        hover.hoverPopout(true)
        await gate.release()
        #expect(hover.channel == general)

        hover.hoverPopout(false)
        await gate.waitStarted(3)
        await gate.release()
        #expect(gate.waits.last == .milliseconds(250))
        #expect(hover.channel == nil)
    }

    // The popout's enter can come before the row's leave.
    @Test
    func itStaysWhenThePointerReachedThePopoutBeforeTheRowSaidItLeft() async {
        let hover = hover()
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.hoverPopout(true)
        hover.hover(general, false) { true }
        await gate.waitStarted(2)
        await gate.release()

        #expect(hover.channel == general)
    }

    @Test
    func leavingBeforeTheDelayShowsNothing() async {
        let hover = hover()

        hover.hover(general, true) { true }
        hover.hover(general, false) { true }
        await gate.waitStarted(2)
        await gate.release()

        #expect(hover.channel == nil)
    }

    @Test
    func movingToARowWithoutThreadsClosesTheFirstRowsPopout() async {
        let hover = hover()
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.hover(general, false) { true }
        hover.hover(ChannelId(rawValue: 12), true) { false }
        await gate.waitStarted(3)
        await gate.release()

        #expect(hover.channel == nil)
    }

    @Test
    func movingToARowWithThreadsShowsItsOwn() async {
        let hover = hover()
        let other = ChannelId(rawValue: 12)
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.hover(general, false) { true }
        hover.hover(other, true) { true }
        await gate.waitStarted(3)
        await gate.release()

        #expect(hover.channel == other)
    }

    // Another server, a collapsed category, a channel no longer listed: no pointer leaves it.
    @Test
    func aRowLeavingTheListTakesItsPopoutAlong() async {
        let hover = hover()
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.rowGone(ChannelId(rawValue: 12))
        #expect(hover.channel == general)
        hover.rowGone(general)
        #expect(hover.channel == nil)

        hover.hover(general, true) { true }
        await gate.waitStarted(2)
        hover.rowGone(general)
        await gate.release()
        #expect(hover.channel == nil)
    }

    @Test
    func aClickClosesItAtOnce() async {
        let hover = hover()
        hover.hover(general, true) { true }
        await gate.waitStarted(1)
        await gate.release()

        hover.dismiss()

        #expect(hover.channel == nil)
    }
}

@MainActor
struct MoreActiveThreadsPopoutTests {
    let now = Date(timeIntervalSince1970: 1_792_704_720)

    func peek(_ raw: UInt64, _ name: String) -> ThreadPeek {
        ThreadPeek(
            id: ChannelId(rawValue: raw), name: name, ownerId: UserId(rawValue: 2),
            owner: User(
                id: UserId(rawValue: 2), username: "ray", globalName: nil, displayName: "Ray",
                bot: false, system: false, avatar: .defaultAvatar(index: 2)),
            time: now - 3600)
    }

    // 8 around; the title 8 + 18 + 2; rows 8 + 18 + 8, 2 apart; "See All" a row too.
    @Test
    func thePopoutMatchesDiscordsMeasures() throws {
        let popout = MoreActiveThreadsPopout(
            peeks: [peek(31, "t1"), peek(32, "t2"), peek(33, "t3")], now: now,
            open: { _, _ in }, seeAllThreads: {})
        let host = NSHostingView(rootView: popout)
        let size = host.fittingSize

        let expected: CGFloat = 8 + 28 + 4 * 34 + 3 * 2 + 8
        #expect(abs(size.height - expected) <= 0.5, "\(size)")
        #expect(size.width >= 272 && size.width <= 400, "\(size)")
        #expect(MoreActiveThreadsPopout.height(threads: 3) == expected)
        let one = NSHostingView(
            rootView: MoreActiveThreadsPopout(
                peeks: [peek(31, "t1")], now: now, open: { _, _ in }, seeAllThreads: {}))
        #expect(abs(one.fittingSize.height - MoreActiveThreadsPopout.height(threads: 1)) <= 0.5)
    }

    // Discord's position layer (canary 646904, module 623646) aligns it with the row's top and
    // nudges it up to stay 12 inside the window, never above 12; a row above the window gives 0.
    @Test
    func itStaysInsideTheWindowAsDiscordsPopoutsDo() {
        let row = { (y: CGFloat) in CGRect(x: 72, y: y, width: 224, height: 32) }

        #expect(MoreActiveThreadsPopout.top(row: row(100), height: 186, area: 813) == 100)
        #expect(MoreActiveThreadsPopout.top(row: row(700), height: 186, area: 813) == 615)
        #expect(MoreActiveThreadsPopout.top(row: row(-5), height: 186, area: 813) == 0)
        #expect(MoreActiveThreadsPopout.top(row: row(100), height: 186, area: 150) == 12)
    }
}

// The popout in the window: what SessionView hands the session and the hover.
@MainActor
struct MoreActiveThreadsScreenTests {
    let store = FakeBrowserStore()
    let gate = ThreadsHoverTests.Gate()

    func settle(until done: () -> Bool) async {
        let deadline = ContinuousClock.now + .milliseconds(500)
        repeat {
            try? await Task.sleep(for: .milliseconds(10))
        } while !done() && ContinuousClock.now < deadline
    }

    // #new's nine joined threads aren't listed; the popout shows beside #new's row.
    func peeking(focus: WindowFocus = WindowFocus()) async -> (SessionModel, ThreadsHover, NSWindow)
    {
        let session = store.session()
        session.open(channel: FakeBrowserStore.parent)
        let hover = ThreadsHover(wait: { [gate] in await gate.wait($0) })
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 813),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = NSHostingView(
            rootView: SessionScreen(session: session, splitWidth: SplitWidth(), hover: hover)
                .environment(\.windowFocus, focus))
        window.orderFrontRegardless()
        window.contentView?.layoutSubtreeIfNeeded()
        hover.hover(FakeBrowserStore.parent, true) { true }
        await gate.waitStarted(1)
        await gate.release()
        return (session, hover, window)
    }

    func panel(of window: NSWindow) -> ContainerPanel? {
        window.childWindows?.compactMap { $0 as? ContainerPanel }.first
    }

    func closeAll(_ window: NSWindow) {
        for child in window.childWindows ?? [] {
            child.orderOut(nil)
        }
        window.orderOut(nil)
    }

    // A panel of its own beside the row, 17 right of the channel list, never key and with no
    // shield: the list and the messages keep their hover.
    @Test
    func itIsAPanelBesideTheRowThatNeverTakesTheKey() async throws {
        var keyed: [NSWindow] = []
        let (session, _, window) = await peeking(focus: WindowFocus { keyed.append($0) })
        defer { closeAll(window) }
        await settle { panel(of: window)?.isVisible == true }
        let popout = try #require(panel(of: window))

        #expect(!popout.canBecomeKey)
        #expect(keyed.isEmpty)
        #expect(window.childWindows?.contains { $0 is ShieldWindow } == false)
        // The row ends 8 before the channel list's edge.
        let left = popout.frame.minX - window.frame.minX
        let row = SidebarWidth.stored(in: .standard) - 8
        #expect(abs(left - (row + MoreActiveThreadsPopout.spacing)) <= 1, "\(left)")
        #expect(
            popout.frame.height == MoreActiveThreadsPopout.height(threads: session.peekedThreads.count))
    }

    @Test
    func theAppGoingToTheBackgroundTakesThePopoutAlong() async throws {
        let (_, hover, window) = await peeking()
        defer { closeAll(window) }
        await settle { panel(of: window)?.isVisible == true }

        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        await settle { hover.channel == nil && (window.childWindows ?? []).isEmpty }

        #expect(hover.channel == nil)
        #expect((window.childWindows ?? []).isEmpty)
    }

    @Test
    func theShownPopoutsThreadsComeFromTheSession() async {
        let (session, _, window) = await peeking()
        defer { closeAll(window) }

        await settle { !session.peekedThreads.isEmpty }

        #expect(session.peekedThreads.count == 3)
    }

    @Test
    func goingHomeTakesThePopoutAlong() async {
        let (session, hover, window) = await peeking()
        defer { closeAll(window) }
        #expect(hover.channel == FakeBrowserStore.parent)

        session.open(.home)
        await settle { hover.channel == nil && (window.childWindows ?? []).isEmpty }

        #expect(hover.channel == nil)
        #expect((window.childWindows ?? []).isEmpty)
    }
}
