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
        /// Discord didn't confirm the logout; the session may still be active there.
        case logoutNotConfirmed
    }

    public private(set) var screen: Screen = .starting
    public private(set) var warning: Warning?
    /// Why the last log out didn't happen; the session stays open.
    public private(set) var logoutError: TokenStoreError?

    @ObservationIgnored private let client: DiscordClient
    @ObservationIgnored private let memory: AccountMemory
    @ObservationIgnored private var started = false
    // The open session's token, so a reconnect needs no Keychain read and a log out can end
    // the session on Discord after the Keychain item is gone.
    @ObservationIgnored private var token: Token?
    @ObservationIgnored private var loggingOut = false
    @ObservationIgnored var endingSession: Task<Void, Never>?

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
        LaunchLog.mark("token loaded")
        guard let token else {
            memory.lastAccount = nil
            return showLogin(nil)
        }
        open(userId, token)
    }

    /// Deletes the token from the Keychain, then shows the login screen and ends the session
    /// on Discord in the background. If the Keychain item can't be deleted, the session stays
    /// open and `logoutError` says why.
    public func logOut() async {
        guard case .session(let session) = screen, !loggingOut else {
            return
        }
        loggingOut = true
        defer { loggingOut = false }
        let token = self.token
        do {
            try await client.forgetToken(account: session.userId)
        } catch {
            logoutError = error as? TokenStoreError ?? .Unavailable
            return
        }
        // The session may have expired meanwhile; that already returned to the login screen.
        guard case .session(let current) = screen, current === session else {
            return
        }
        session.close()
        forget(session.userId)
        warning = nil
        showLogin(nil)
        guard let token else {
            return
        }
        endingSession = Task { [client] in
            do {
                try await client.endSession(token: token)
            } catch {
                if case .login = self.screen {
                    self.warning = .logoutNotConfirmed
                }
            }
        }
    }

    /// Opens a new session from the held token after a close that wasn't a rejected token.
    public func reconnect() {
        guard case .session(let old) = screen, case .closed(let error) = old.connection,
            error != .AuthenticationFailed, let token
        else {
            return
        }
        old.close()
        open(old.userId, token, drafts: old.drafts)
    }

    /// Before the Mac sleeps.
    public func suspend() {
        if case .session(let session) = screen {
            session.suspend()
        }
    }

    /// After the Mac wakes.
    public func resume() {
        if case .session(let session) = screen {
            session.resume()
        }
    }

    public func dismissWarning() {
        warning = nil
    }

    public func dismissLogoutError() {
        logoutError = nil
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

    private func open(_ userId: UserId, _ token: Token, drafts: Drafts = Drafts()) {
        let account: Account
        do {
            account = try client.account(token: token)
        } catch {
            return showLogin(nil)
        }
        self.token = token
        let session = SessionModel(
            userId: userId, account: account, memory: memory, drafts: drafts
        ) { [weak self] error in
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
        forget(userId)
        showLogin(.sessionExpired)
        Task { [client] in
            try? await client.forgetToken(account: userId)
        }
    }

    private func forget(_ userId: UserId) {
        token = nil
        memory.lastAccount = nil
        memory.remember(nil, of: userId)
    }
}
