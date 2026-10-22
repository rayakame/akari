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

        func wait(_ duration: Duration) async {
            await withCheckedContinuation { continuation in
                waits.append(duration)
                waiting.append(continuation)
            }
        }

        // Ends every wait so far; the hover's timers run on the main actor's next turns.
        func release() async {
            let released = waiting
            waiting = []
            released.forEach { $0.resume() }
            for _ in 0..<5 {
                await Task.yield()
            }
        }

        func waitStarted(_ count: Int) async {
            for _ in 0..<100 where waits.count < count {
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
                bot: false, system: false),
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
    }
}
