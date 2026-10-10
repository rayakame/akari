import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct ConnectionBarTests {
    // On screen, since SwiftUI starts a view's tasks only once it appears.
    func shownHeight(_ notice: ConnectionNotice, after wait: Duration) async throws -> CGFloat {
        let host = NSHostingView(rootView: ConnectionBar(notice: notice) {}.frame(width: 400))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 60), styleMask: [.borderless],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        try await Task.sleep(for: wait)
        host.layoutSubtreeIfNeeded()
        return host.fittingSize.height
    }

    @Test
    func aClosedSessionShowsTheBarAtOnce() async throws {
        #expect(try await shownHeight(.closed(.Stopped), after: .milliseconds(300)) == 32)
    }

    @Test
    func reconnectingShowsOnlyAfterItsDelay() async throws {
        #expect(try await shownHeight(.reconnecting, after: .milliseconds(300)) == 0)
        #expect(try await shownHeight(.reconnecting, after: .milliseconds(1600)) == 32)
    }
}
