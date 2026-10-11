import AppKit
import Testing

@testable import Akari

@MainActor
struct AppDelegateTests {
    final class Replies {
        var all: [Bool] = []
    }

    let replies = Replies()

    func delegate(quit: ((Duration) async -> Void)?) -> AppDelegate {
        let delegate = AppDelegate()
        delegate.limit = .milliseconds(200)
        delegate.quit = quit
        delegate.reply = { [replies] in replies.all.append($0) }
        return delegate
    }

    func waitForReply() async {
        let deadline = ContinuousClock.now + .seconds(3)
        while replies.all.isEmpty, ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
    }

    @Test
    func quittingRepliesEvenWhenTheFlushHangs() async {
        let delegate = delegate { _ in
            try? await Task.sleep(for: .seconds(60))
        }
        let started = ContinuousClock.now

        let answer = delegate.applicationShouldTerminate(NSApplication.shared)
        await waitForReply()

        #expect(answer == .terminateLater)
        #expect(replies.all == [true])
        #expect(started.duration(to: .now) < .seconds(2))
    }

    @Test
    func quittingWaitsForTheFlush() async {
        final class Flushed {
            var limit: Duration?
        }
        let flushed = Flushed()
        let delegate = delegate { limit in
            flushed.limit = limit
        }

        let answer = delegate.applicationShouldTerminate(NSApplication.shared)
        await waitForReply()

        #expect(answer == .terminateLater)
        #expect(flushed.limit == .milliseconds(200))
        #expect(replies.all == [true])
    }

    @Test
    func withoutAnAppQuittingIsImmediate() {
        let delegate = delegate(quit: nil)

        #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateNow)
        #expect(replies.all.isEmpty)
    }
}
