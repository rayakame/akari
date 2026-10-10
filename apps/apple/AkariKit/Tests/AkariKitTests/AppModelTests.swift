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
    func logOutEndsTheSessionAndForgetsTheAccount() async throws {
        let (app, _, account) = try await restored()

        await app.logOut()

        #expect(account.log.calls.contains(.close))
        #expect(client.calls.current.last == .logout(id(42)))
        #expect(!client.calls.current.contains(.forgetToken(id(42))))
        #expect(memory.lastAccount == nil)
        #expect(try #require(app.screen.login).notice == nil)
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
