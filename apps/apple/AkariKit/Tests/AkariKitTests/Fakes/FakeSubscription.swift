import AkariKit
import Foundation

final class FakeSubscription: StoreSubscription, @unchecked Sendable {
    let batches = AsyncQueue<[StoreEvent]>()
    let closed = Signal()

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    func send(_ events: StoreEvent...) {
        batches.send(events)
    }

    /// Like an account that closed: `next()` returns what is buffered, then `nil`.
    func end() {
        batches.finish()
    }

    override func next() async -> [StoreEvent]? {
        await batches.next()
    }

    override func close() {
        batches.close()
        closed.fire()
    }
}
