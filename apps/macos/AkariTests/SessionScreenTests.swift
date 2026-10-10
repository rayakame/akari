import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct SessionScreenTests {
    func session(_ account: GuildAccount, defaults: UserDefaults) -> SessionModel {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: account,
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.open(.guild(GuildId(rawValue: 1)))
        return session
    }

    func waitForLoad(_ account: GuildAccount) async throws {
        for _ in 0..<40 where account.loads.withLock({ $0 }) == 0 {
            try await Task.sleep(for: .milliseconds(50))
        }
    }

    // A reconnect replaces the session with one for the same channel.
    @Test
    func aNewSessionForTheSameChannelLoadsIt() async throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let first = GuildAccount()
        let second = GuildAccount()
        let host = NSHostingView(
            rootView: AnyView(SessionScreen(session: session(first, defaults: defaults))))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        try await waitForLoad(first)
        #expect(first.loads.withLock { $0 } == 1)

        host.rootView = AnyView(SessionScreen(session: session(second, defaults: defaults)))
        try await waitForLoad(second)

        #expect(second.loads.withLock { $0 } == 1)
    }
}
