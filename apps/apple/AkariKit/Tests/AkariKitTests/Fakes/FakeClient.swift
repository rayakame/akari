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
    }

    let calls = Locked<[Call]>([])
    /// Handed out in order by `passwordLogin()` and `qrLogin()`; fresh fakes after that.
    let passwordLogins = Locked<[FakePasswordLogin]>([])
    let qrLogins = Locked<[FakeQrLogin]>([])
    let qrError = Locked<LoginError?>(nil)
    let tokens = Locked<[UserId: String]>([:])
    let loadError = Locked<TokenStoreError?>(nil)
    let saveError = Locked<TokenStoreError?>(nil)
    /// Every account `account(token:)` returned.
    let accounts = Locked<[FakeAccount]>([])
    let forgotten = Signal()

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

    override func account(token: Token) throws -> Account {
        record(.account(token: (token as? FakeToken)?.value ?? "?"))
        let account = FakeAccount()
        accounts.withLock { $0.append(account) }
        return account
    }

    override func loadToken(account: UserId) async throws -> Token? {
        record(.loadToken(account))
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
        tokens.withLock { $0[account] = nil }
        forgotten.fire()
    }
}
