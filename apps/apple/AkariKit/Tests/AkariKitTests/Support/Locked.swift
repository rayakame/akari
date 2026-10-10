import Foundation

final class Locked<Value>: @unchecked Sendable {
    private let lock = NSLock()
    private var value: Value

    init(_ value: Value) {
        self.value = value
    }

    var current: Value { lock.withLock { value } }

    func withLock<R>(_ body: (inout Value) -> R) -> R {
        lock.withLock { body(&value) }
    }
}
