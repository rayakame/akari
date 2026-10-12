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
        final class Flush {
            var limit: Duration?
            var released = false
        }
        let flush = Flush()
        let delegate = delegate { limit in
            flush.limit = limit
            let deadline = ContinuousClock.now + .seconds(5)
            while !flush.released, ContinuousClock.now < deadline {
                try? await Task.sleep(for: .milliseconds(10))
            }
        }
        delegate.limit = .seconds(30)

        let answer = delegate.applicationShouldTerminate(NSApplication.shared)
        try? await Task.sleep(for: .milliseconds(300))

        #expect(answer == .terminateLater)
        #expect(flush.limit == .seconds(30))
        #expect(replies.all.isEmpty)

        flush.released = true
        await waitForReply()

        #expect(replies.all == [true])
    }

    @Test
    func withoutAnAppQuittingIsImmediate() {
        let delegate = delegate(quit: nil)

        #expect(delegate.applicationShouldTerminate(NSApplication.shared) == .terminateNow)
        #expect(replies.all.isEmpty)
    }
}
