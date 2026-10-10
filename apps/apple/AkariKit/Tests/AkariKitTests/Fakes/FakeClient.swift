import AkariKit
import Foundation

final class FakeClient: DiscordClient, @unchecked Sendable {
    enum Call: Equatable {
        case passwordLogin
        case qrLogin
        case account(token: String)
        case saveToken(UserId, String)
        case loadToken(UserId)
        case logout(UserId)
        case forgetToken(UserId)
        case endSession(String)
    }

    let calls = Locked<[Call]>([])
    // Handed out in order by `passwordLogin()` and `qrLogin()`; fresh fakes after that.
    let passwordLogins = Locked<[FakePasswordLogin]>([])
    let qrLogins = Locked<[FakeQrLogin]>([])
    let qrError = Locked<LoginError?>(nil)
    let tokens = Locked<[UserId: String]>([:])
    let loadError = Locked<TokenStoreError?>(nil)
    // While set, each token load waits for one value here, like a Keychain dialog.
    let holdTokenLoads = Locked(false)
    let tokenLoads = AsyncQueue<Void>()
    let saveError = Locked<TokenStoreError?>(nil)
    // Every account `account(token:)` returned.
    let accounts = Locked<[FakeAccount]>([])
    let forgotten = Signal()
    let forgetError = Locked<TokenStoreError?>(nil)
    // While set, each forget or end of a session waits for one value here.
    let holdForgets = Locked(false)
    let forgets = AsyncQueue<Void>()
    let holdEndSessions = Locked(false)
    let endSessions = AsyncQueue<Void>()
    let endSessionError = Locked<LogoutError?>(nil)

    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    private func record(_ call: Call) {
        calls.withLock { $0.append(call) }
    }

    override func passwordLogin() -> PasswordLogin {
        record(.passwordLogin)
        return passwordLogins.withLock { $0.isEmpty ? FakePasswordLogin() : $0.removeFirst() }
    }

    override func qrLogin() throws -> QrLogin {
        record(.qrLogin)
        if let error = qrError.current {
            throw error
        }
        return qrLogins.withLock { $0.isEmpty ? FakeQrLogin() : $0.removeFirst() }
    }

    let accountError = Locked<GatewayError?>(nil)

    override func account(token: Token) throws -> Account {
        record(.account(token: (token as? FakeToken)?.value ?? "?"))
        if let error = accountError.current {
            throw error
        }
        let account = FakeAccount()
        accounts.withLock { $0.append(account) }
        return account
    }

    override func loadToken(account: UserId) async throws -> Token? {
        record(.loadToken(account))
        if holdTokenLoads.current {
            _ = await tokenLoads.next()
        }
        if let error = loadError.current {
            throw error
        }
        return tokens.current[account].map(FakeToken.init)
    }

    override func saveToken(account: UserId, token: Token) async throws {
        let value = (token as? FakeToken)?.value ?? "?"
        record(.saveToken(account, value))
        if let error = saveError.current {
            throw error
        }
        tokens.withLock { $0[account] = value }
    }

    override func logout(account: UserId) async throws {
        record(.logout(account))
        tokens.withLock { $0[account] = nil }
    }

    override func forgetToken(account: UserId) async throws {
        record(.forgetToken(account))
        if holdForgets.current {
            _ = await forgets.next()
        }
        if let error = forgetError.current {
            throw error
        }
        tokens.withLock { $0[account] = nil }
        forgotten.fire()
    }

    override func endSession(token: Token) async throws {
        record(.endSession((token as? FakeToken)?.value ?? "?"))
        if holdEndSessions.current {
            _ = await endSessions.next()
        }
        if let error = endSessionError.current {
            throw error
        }
    }
}
