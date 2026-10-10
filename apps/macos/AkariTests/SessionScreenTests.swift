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

@MainActor
struct ComposerAlignmentTests {
    // Bottom and top edges in pixels, found by scanning a column for color changes.
    func edges(_ image: Rendered, x: CGFloat) throws -> (top: Int, bottom: Int) {
        let height = CGFloat(image.rep.pixelsHigh) / image.scale
        let below = try #require(image.firstChange(at: x, above: height - 1))
        let top = try #require(image.firstChange(at: x, above: CGFloat(below - 3) / image.scale))
        return (top, below)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theComposerLinesUpWithTheUserPanel(appearance: NSAppearance.Name) throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: GuildAccount(),
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.open(.guild(GuildId(rawValue: 1)))
        let image = try Rendered(SessionScreen(session: session), appearance: appearance)

        let panel = try edges(image, x: 40)
        let composer = try edges(image, x: image.width - 60)

        #expect(abs(panel.bottom - composer.bottom) <= 1, "panel \(panel), composer \(composer)")
        #expect(
            abs((panel.bottom - panel.top) - (composer.bottom - composer.top)) <= 2,
            "panel \(panel), composer \(composer)")
    }
}
