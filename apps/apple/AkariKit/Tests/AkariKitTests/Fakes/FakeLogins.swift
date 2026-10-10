import AkariKit
import Foundation

final class FakeToken: Token, @unchecked Sendable {
    let value: String

    init(_ value: String) {
        self.value = value
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }
}

final class FakePasswordLogin: PasswordLogin, @unchecked Sendable {
    enum Call: Equatable {
        case submit(String, String)
        case sendMfaSms
        case submitMfa(MfaMethod, String)
        case confirmNewLocation(String)
        case verifyPhone(String)
        case cancel
    }

    let calls = Locked<[Call]>([])
    /// One reply per step, in order.
    let replies = AsyncQueue<Result<LoginStep, LoginError>>()
    let cancelled = Signal()

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    func reply(_ step: LoginStep) {
        replies.send(.success(step))
    }

    func fail(_ error: LoginError) {
        replies.send(.failure(error))
    }

    override func submit(login: String, password: String) async throws -> LoginStep {
        try await step(.submit(login, password))
    }

    override func sendMfaSms() async throws -> LoginStep {
        try await step(.sendMfaSms)
    }

    override func submitMfa(method: MfaMethod, code: String) async throws -> LoginStep {
        try await step(.submitMfa(method, code))
    }

    override func confirmNewLocation(linkOrToken: String) async throws -> LoginStep {
        try await step(.confirmNewLocation(linkOrToken))
    }

    override func verifyPhone(code: String) async throws -> LoginStep {
        try await step(.verifyPhone(code))
    }

    // Like akari-core: a running step fails with Cancelled.
    override func cancel() {
        calls.withLock { $0.append(.cancel) }
        replies.close()
        cancelled.fire()
    }

    private func step(_ call: Call) async throws -> LoginStep {
        calls.withLock { $0.append(call) }
        guard let reply = await replies.next() else {
            throw LoginError.Cancelled
        }
        return try reply.get()
    }
}

final class FakeQrLogin: QrLogin, @unchecked Sendable {
    let events = AsyncQueue<Result<QrEvent, LoginError>>()
    let cancelled = Signal()

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    func send(_ event: QrEvent) {
        events.send(.success(event))
    }

    func fail(_ error: LoginError) {
        events.send(.failure(error))
    }

    override func next() async throws -> QrEvent {
        guard let event = await events.next() else {
            throw LoginError.Cancelled
        }
        return try event.get()
    }

    override func cancel() {
        events.close()
        cancelled.fire()
    }
}
