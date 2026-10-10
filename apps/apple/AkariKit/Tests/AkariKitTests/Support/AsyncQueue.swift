import Foundation

/// Scripted values for a fake's async pull, e.g. `StoreSubscription.next()`.
final class AsyncQueue<Element: Sendable>: @unchecked Sendable {
    private struct State {
        var buffered: [Element] = []
        var finished = false
        var waiting: [CheckedContinuation<Element?, Never>] = []
        var pulls = 0
        var pullWaiters: [UUID: (count: Int, continuation: CheckedContinuation<Void, Never>)] = [:]
    }

    private let state = Locked(State())

    func send(_ element: Element) {
        state.withLock { state in
            if state.waiting.isEmpty {
                state.buffered.append(element)
            } else {
                state.waiting.removeFirst().resume(returning: element)
            }
        }
    }

    /// Pulls return what is buffered, then `nil`.
    func finish() {
        state.withLock { state in
            state.finished = true
            state.waiting.forEach { $0.resume(returning: nil) }
            state.waiting = []
        }
    }

    /// Pulls return `nil` at once, without what is buffered.
    func close() {
        state.withLock { state in
            state.buffered = []
        }
        finish()
    }

    func next() async -> Element? {
        await withCheckedContinuation { continuation in
            state.withLock { state in
                state.pulls += 1
                for (key, waiter) in state.pullWaiters where waiter.count <= state.pulls {
                    waiter.continuation.resume()
                    state.pullWaiters[key] = nil
                }
                if !state.buffered.isEmpty {
                    continuation.resume(returning: state.buffered.removeFirst())
                } else if state.finished {
                    continuation.resume(returning: nil)
                } else {
                    state.waiting.append(continuation)
                }
            }
        }
    }

    var pulls: Int { state.current.pulls }

    /// Returns once `next()` was called `count` times (or the task is cancelled). A consumer
    /// that handles each value before pulling again has then handled the first `count - 1`.
    func pulled(_ count: Int) async {
        let key = UUID()
        await withTaskCancellationHandler {
            await withCheckedContinuation { continuation in
                state.withLock { state in
                    if state.pulls >= count || Task.isCancelled {
                        continuation.resume()
                    } else {
                        state.pullWaiters[key] = (count, continuation)
                    }
                }
            }
        } onCancel: {
            state.withLock { $0.pullWaiters.removeValue(forKey: key)?.continuation.resume() }
        }
    }
}
