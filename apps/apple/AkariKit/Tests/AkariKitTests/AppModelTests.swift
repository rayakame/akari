import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
final class AppModelTests {
    let client = FakeClient()
    let suite = "app.akari.tests.\(UUID().uuidString)"
    let memory: AccountMemory

    init() throws {
        memory = AccountMemory(defaults: try #require(UserDefaults(suiteName: suite)))
    }

    deinit {
        UserDefaults.standard.removePersistentDomain(forName: suite)
    }

    func makeApp() -> AppModel {
        AppModel(client: client, memory: memory)
    }

    func restored() async throws -> (AppModel, SessionModel, FakeAccount) {
        memory.lastAccount = id(42)
        memory.remember(.init(place: .home, channel: id(5)), of: id(42))
        client.tokens.withLock { $0[id(42)] = "stored.token" }
        let app = makeApp()
        await app.start()
        let session = try #require(app.screen.session)
        let account = try #require(client.accounts.current.first)
        return (app, session, account)
    }

    @Test
    func startRestoresTheLastAccount() async throws {
        let (_, session, account) = try await restored()

        #expect(session.userId == id(42))
        #expect(client.calls.current == [.loadToken(id(42)), .account(token: "stored.token")])
        #expect(account.log.calls.contains(.connect))
        session.close()
    }

    @Test
    func aSecondStartDuringTheTokenLoadDoesNothing() async throws {
        memory.lastAccount = id(42)
        client.tokens.withLock { $0[id(42)] = "stored.token" }
        client.holdTokenLoads.withLock { $0 = true }
        let app = makeApp()

        async let first: Void = app.start()
        await client.tokenLoads.pulled(1)
        client.tokenLoads.send(())
        client.tokenLoads.send(())
        await app.start()
        await first

        let accounts = client.calls.current.filter { call in
            if case .account = call { true } else { false }
        }
        #expect(accounts.count == 1)
        #expect(client.calls.current.filter { $0 == .loadToken(id(42)) }.count == 1)
        try #require(app.screen.session).close()
    }

    @Test
    func startShowsLoginWithoutAStoredToken() async throws {
        let fresh = makeApp()
        await fresh.start()
        #expect(try #require(fresh.screen.login).notice == nil)
        #expect(client.calls.current.isEmpty)

        memory.lastAccount = id(42)
        let app = makeApp()
        await app.start()

        #expect(try #require(app.screen.login).notice == nil)
        #expect(memory.lastAccount == nil)
    }

    @Test
    func anUnreadableTokenShowsLoginWithANotice() async throws {
        memory.lastAccount = id(42)
        client.loadError.withLock { $0 = .Unavailable }
        let app = makeApp()

        await app.start()

        #expect(try #require(app.screen.login).notice == .tokenUnreadable)
        #expect(memory.lastAccount == id(42))
    }

    @Test
    func loginSavesRemembersAndOpensTheSession() async throws {
        let password = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [password] }
        let app = makeApp()
        await app.start()
        let login = try #require(app.screen.login)

        login.login = "me@example.com"
        login.password = "hunter2"
        password.reply(.done(success: success(7, token: "fresh.token")))
        await login.submit()

        let session = try #require(app.screen.session)
        #expect(session.userId == id(7))
        #expect(client.calls.current.contains(.saveToken(id(7), "fresh.token")))
        #expect(client.calls.current.last == .account(token: "fresh.token"))
        #expect(memory.lastAccount == id(7))
        #expect(app.warning == nil)
        session.close()
    }

    @Test
    func aTokenThatCantBeSavedStillLogsIn() async throws {
        let password = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [password] }
        client.saveError.withLock { $0 = .Unavailable }
        let app = makeApp()
        await app.start()
        let login = try #require(app.screen.login)

        login.login = "me@example.com"
        login.password = "hunter2"
        password.reply(.done(success: success(7)))
        await login.submit()

        let session = try #require(app.screen.session)
        #expect(app.warning == .tokenNotSaved)
        #expect(memory.lastAccount == nil)
        session.close()
    }

    @Test
    func aRejectedTokenReturnsToLoginAndForgetsIt() async throws {
        let (app, _, account) = try await restored()

        account.fakeStore.subscription.send(
            .connection(state: .closed(error: .AuthenticationFailed))
        )
        await client.forgotten.wait()

        #expect(client.calls.current.filter { $0 == .forgetToken(id(42)) }.count == 1)
        #expect(!client.calls.current.contains(.logout(id(42))))
        #expect(memory.lastAccount == nil)
        #expect(memory.lastSpot(of: id(42)) == nil)
        #expect(try #require(app.screen.login).notice == .sessionExpired)
        #expect(account.log.calls.contains(.close))
    }

    @Test
    func otherClosesStayOnTheSession() async throws {
        let (app, session, account) = try await restored()

        account.fakeStore.subscription.send(.connection(state: .closed(error: .Rejected(code: 4000))))
        await account.fakeStore.subscription.batches.pulled(2)

        #expect(app.screen.session === session)
        #expect(memory.lastAccount == id(42))
        #expect(!client.calls.current.contains(.forgetToken(id(42))))
        session.close()
    }

    @Test
    func logOutForgetsFirstThenEndsTheSessionInTheBackground() async throws {
        let (app, _, account) = try await restored()
        client.holdEndSessions.withLock { $0 = true }

        await app.logOut()

        #expect(try #require(app.screen.login).notice == nil)
        #expect(account.log.calls.contains(.close))
        #expect(memory.lastAccount == nil)
        #expect(memory.lastSpot(of: id(42)) == nil)
        client.endSessions.send(())
        await app.endingSession?.value
        let calls = client.calls.current
        let forgot = try #require(calls.firstIndex(of: .forgetToken(id(42))))
        let ended = try #require(calls.firstIndex(of: .endSession("stored.token")))
        #expect(forgot < ended)
        #expect(!calls.contains(.logout(id(42))))
        #expect(app.warning == nil)
    }

    @Test
    func aFailedForgetStaysOnTheSession() async throws {
        let (app, session, account) = try await restored()
        client.forgetError.withLock { $0 = .Unavailable }

        await app.logOut()

        #expect(app.screen.session === session)
        #expect(!account.log.calls.contains(.close))
        #expect(memory.lastAccount == id(42))
        #expect(memory.lastSpot(of: id(42)) != nil)
        #expect(app.logoutError == .Unavailable)
        #expect(!client.calls.current.contains(.endSession("stored.token")))
        app.dismissLogoutError()
        #expect(app.logoutError == nil)
        session.close()
    }

    @Test
    func logOutRunsOnce() async throws {
        let (app, _, _) = try await restored()
        client.holdForgets.withLock { $0 = true }

        async let first: Void = app.logOut()
        await client.forgets.pulled(1)
        await app.logOut()
        client.forgets.send(())
        await first

        #expect(client.calls.current.filter { $0 == .forgetToken(id(42)) }.count == 1)
        #expect(app.screen.login != nil)
    }

    @Test
    func aSessionThatExpiresDuringLogOutIsntClosedTwice() async throws {
        let (app, _, account) = try await restored()
        client.holdForgets.withLock { $0 = true }

        async let logOut: Void = app.logOut()
        await client.forgets.pulled(1)
        account.fakeStore.subscription.send(
            .connection(state: .closed(error: .AuthenticationFailed))
        )
        await account.fakeStore.subscription.batches.pulled(2)
        let expired = try #require(app.screen.login)
        client.forgets.send(())
        client.forgets.send(())
        await logOut
        await client.forgotten.wait()

        #expect(app.screen.login === expired)
        #expect(expired.notice == .sessionExpired)
        #expect(!client.calls.current.contains(.endSession("stored.token")))
    }

    @Test
    func aLoginDuringTheBackgroundLogoutKeepsItsToken() async throws {
        let (app, _, _) = try await restored()
        client.holdEndSessions.withLock { $0 = true }
        let password = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [password] }

        await app.logOut()
        let login = try #require(app.screen.login)
        login.login = "me@example.com"
        login.password = "hunter2"
        password.reply(.done(success: success(42, token: "token-2")))
        await login.submit()
        client.endSessions.send(())
        await app.endingSession?.value

        #expect(client.tokens.current[id(42)] == "token-2")
        let calls = client.calls.current
        let saved = try #require(calls.firstIndex(of: .saveToken(id(42), "token-2")))
        let storeCallsAfter = calls[(saved + 1)...].filter { call in
            switch call {
            case .forgetToken, .logout, .saveToken: true
            default: false
            }
        }
        #expect(storeCallsAfter.isEmpty)
        try #require(app.screen.session).close()
    }

    @Test
    func anUnconfirmedLogoutWarnsOnTheLoginScreen() async throws {
        let (app, _, _) = try await restored()
        client.endSessionError.withLock { $0 = .Network(kind: .connect) }

        await app.logOut()
        await app.endingSession?.value

        #expect(app.warning == .logoutNotConfirmed)
        app.dismissWarning()
        #expect(app.warning == nil)
    }

    @Test
    func reconnectBuildsANewSessionFromTheHeldToken() async throws {
        let (app, session, account) = try await restored()
        account.fakeStore.subscription.send(.connection(state: .closed(error: .Stopped)))
        await account.fakeStore.subscription.batches.pulled(2)
        #expect(session.connection == .closed(error: .Stopped))

        app.reconnect()

        let next = try #require(app.screen.session)
        #expect(next !== session)
        #expect(next.userId == id(42))
        let calls = client.calls.current
        #expect(calls.filter { $0 == .account(token: "stored.token") }.count == 2)
        #expect(calls.filter { $0 == .loadToken(id(42)) }.count == 1)
        #expect(account.log.calls.contains(.close))
        let second = try #require(client.accounts.current.last)
        #expect(second !== account)
        #expect(second.log.calls.contains(.connect))
        next.close()
    }

    @Test
    func reconnectOnlyAfterAnErrorClose() async throws {
        let (app, session, account) = try await restored()

        app.reconnect()
        #expect(app.screen.session === session)

        account.fakeStore.subscription.send(
            .connection(state: .closed(error: .AuthenticationFailed))
        )
        await client.forgotten.wait()
        app.reconnect()

        #expect(app.screen.login != nil)
        #expect(client.accounts.current.count == 1)
    }

    @Test
    func suspendAndResumeReachTheOpenSession() async throws {
        let fresh = makeApp()
        await fresh.start()
        fresh.suspend()
        fresh.resume()

        let (app, session, account) = try await restored()
        account.log.forget()
        app.suspend()
        app.resume()

        #expect(account.log.calls == [.disconnect, .connect])
        session.close()
    }

    @Test
    func warningsCanBeDismissed() async throws {
        let password = FakePasswordLogin()
        client.passwordLogins.withLock { $0 = [password] }
        client.saveError.withLock { $0 = .Unavailable }
        let app = makeApp()
        await app.start()
        let login = try #require(app.screen.login)
        login.login = "me@example.com"
        login.password = "hunter2"
        password.reply(.done(success: success(7)))
        await login.submit()
        #expect(app.warning == .tokenNotSaved)

        app.dismissWarning()

        #expect(app.warning == nil)
        try #require(app.screen.session).close()
    }
}

extension AppModel.Screen {
    var session: SessionModel? {
        if case .session(let session) = self { session } else { nil }
    }

    var login: LoginModel? {
        if case .login(let login) = self { login } else { nil }
    }
}
