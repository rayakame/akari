import AppKit

// Quitting waits for the session's cache to be written, at most `limit`.
final class AppDelegate: NSObject, NSApplicationDelegate {
    var quit: ((Duration) async -> Void)?
    var limit: Duration = .seconds(1)
    var reply: (Bool) -> Void = { NSApp.reply(toApplicationShouldTerminate: $0) }

    func applicationWillFinishLaunching(_ notification: Notification) {
        if Launch.isTesting {
            TestHost.start()
        }
    }

    func applicationShouldTerminate(_ sender: NSApplication) -> NSApplication.TerminateReply {
        guard let quit else {
            return .terminateNow
        }
        let limit = limit
        Task {
            // The app keeps its own limit too; this one holds even if that work hangs.
            await firstOf({ await quit(limit) }, orAfter: limit + .milliseconds(500))
            reply(true)
        }
        return .terminateLater
    }
}

private func firstOf(_ work: @escaping () async -> Void, orAfter limit: Duration) async {
    var finished = false
    await withCheckedContinuation { (done: CheckedContinuation<Void, Never>) in
        let finish = { @MainActor in
            guard !finished else {
                return
            }
            finished = true
            done.resume()
        }
        Task { @MainActor in
            await work()
            finish()
        }
        Task { @MainActor in
            try? await Task.sleep(for: limit)
            finish()
        }
    }
}
