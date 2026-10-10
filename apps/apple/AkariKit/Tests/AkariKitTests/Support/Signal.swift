import Foundation

// Fires once; `wait()` returns at once afterwards, or when the waiting task is cancelled
// (a test's time limit), so a missing signal fails the test instead of hanging the run.
final class Signal: Sendable {
    private struct State {
        var fired = false
        var waiting: [UUID: CheckedContinuation<Void, Never>] = [:]
    }

    private let state = Locked(State())

    var fired: Bool { state.current.fired }

    func fire() {
        state.withLock { state in
            state.fired = true
            state.waiting.values.forEach { $0.resume() }
            state.waiting = [:]
        }
    }

    func wait() async {
        let key = UUID()
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                state.withLock { state in
                    if state.fired || Task.isCancelled {
                        continuation.resume()
                    } else {
                        state.waiting[key] = continuation
                    }
                }
            }
        } onCancel: {
            state.withLock { $0.waiting.removeValue(forKey: key)?.resume() }
        }
    }
}
