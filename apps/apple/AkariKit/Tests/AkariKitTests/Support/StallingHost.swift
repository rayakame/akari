import Foundation
import Network

// A host that accepts connections and never answers, so a request to it waits until it's
// cancelled or times out.
final class StallingHost: @unchecked Sendable {
    private let listener: NWListener
    private let connections = Locked<[NWConnection]>([])

    private init(listener: NWListener) {
        self.listener = listener
    }

    static func start() async throws -> StallingHost {
        let listener = try NWListener(using: .tcp, on: .any)
        let host = StallingHost(listener: listener)
        listener.newConnectionHandler = { connection in
            host.connections.withLock { $0.append(connection) }
            connection.start(queue: .global())
        }
        let resumed = Locked(false)
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
            listener.stateUpdateHandler = { state in
                let outcome: Result<Void, Error>
                switch state {
                case .ready:
                    outcome = .success(())
                case .failed(let error):
                    outcome = .failure(error)
                default:
                    return
                }
                let first = resumed.withLock { was -> Bool in
                    let before = was
                    was = true
                    return !before
                }
                if first {
                    continuation.resume(with: outcome)
                }
            }
            listener.start(queue: .global())
        }
        return host
    }

    var url: String {
        "https://127.0.0.1:\(listener.port?.rawValue ?? 0)/"
    }

    func stop() {
        listener.cancel()
        connections.withLock { connections in
            connections.forEach { $0.cancel() }
            connections.removeAll()
        }
    }
}
