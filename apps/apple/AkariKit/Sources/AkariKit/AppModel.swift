import Observation

/// The app's top level: restores the last account or shows the login screen, and returns to
/// it when the session ends for good.
@MainActor @Observable
public final class AppModel {
    public enum Screen {
        case starting
        case login(LoginModel)
        case session(SessionModel)
    }

    public enum Warning: Equatable, Sendable {
        case tokenNotSaved
    }

    public private(set) var screen: Screen = .starting
    public private(set) var warning: Warning?

    @ObservationIgnored private let client: DiscordClient
    @ObservationIgnored private let memory: AccountMemory
    @ObservationIgnored private var started = false

    public init(client: DiscordClient, memory: AccountMemory = AccountMemory()) {
        self.client = client
        self.memory = memory
    }

    /// Restores the last account, or shows the login screen.
    public func start() async {
        // Set before the token load awaits, which can wait on a Keychain dialog.
        guard !started else {
            return
        }
        started = true
        guard let userId = memory.lastAccount else {
            return showLogin(nil)
        }
        let token: Token?
        do {
            token = try await client.loadToken(account: userId)
        } catch {
            return showLogin(.tokenUnreadable)
        }
        guard let token else {
            memory.lastAccount = nil
            return showLogin(nil)
        }
        open(userId, token)
    }

    /// Ends the session, logs out on Discord and forgets the account. A token Discord
    /// rejected (`AuthenticationFailed`) is deleted with `forgetToken`, without a request.
    public func logOut() async {
        guard case .session(let session) = screen else {
            return
        }
        session.close()
        memory.lastAccount = nil
        warning = nil
        showLogin(nil)
        // The token is deleted even when Discord can't be reached.
        try? await client.logout(account: session.userId)
    }

    private func showLogin(_ notice: LoginNotice?) {
        screen = .login(
            LoginModel(client: client, notice: notice) { [weak self] success in
                await self?.loggedIn(success)
            })
    }

    private func loggedIn(_ success: LoginSuccess) async {
        do {
            try await client.saveToken(account: success.userId, token: success.token)
            memory.lastAccount = success.userId
            warning = nil
        } catch {
            warning = .tokenNotSaved
        }
        open(success.userId, success.token)
    }

    private func open(_ userId: UserId, _ token: Token) {
        let account: Account
        do {
            account = try client.account(token: token)
        } catch {
            return showLogin(nil)
        }
        let session = SessionModel(userId: userId, account: account) { [weak self] error in
            self?.closed(userId, error)
        }
        screen = .session(session)
        session.start()
    }

    private func closed(_ userId: UserId, _ error: GatewayError?) {
        guard error == .AuthenticationFailed, case .session(let session) = screen,
            session.userId == userId
        else {
            return
        }
        session.close()
        memory.lastAccount = nil
        showLogin(.sessionExpired)
        Task { [client] in
            try? await client.forgetToken(account: userId)
        }
    }
}
