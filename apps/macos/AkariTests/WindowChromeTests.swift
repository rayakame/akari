import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct WindowChromeTests {
    @Test
    func theTitleBarShowsTheSidebarColorUnderTheTitle() async throws {
        let window = try await mainWindow()

        #expect(window.titlebarAppearsTransparent)
        #expect(window.titleVisibility == .visible)
        #expect(window.styleMask.contains(.fullSizeContentView))
        #expect(window.backgroundColor == Palette.frame)
    }

    @Test
    func theStripUnderTheTitleBarIsTheSidebarColorAcrossTheWindow() throws {
        let session = SessionModel(userId: UserId(rawValue: 1), account: EmptyAccount()) { _ in }
        let image = try Rendered(SessionView(session: session) { _, _ in EmptyView() })
        let right = image.width - 20

        // The rail is `frame`.
        let rail = image.color(20, 300)
        #expect(!same(image.color(right, 300), rail), "the page has its own color")
        #expect(same(image.color(right, 10), rail), "over the page")
        #expect(same(image.color(20, 10), rail), "over the rail")
    }

    // SwiftUI styles its window once it shows, so wait until that's done.
    private func mainWindow() async throws -> NSWindow {
        for _ in 0..<40 {
            if let window = NSApp.windows.first(where: { $0.isVisible && $0.title == "Akari" }) {
                try await Task.sleep(for: .milliseconds(200))
                return window
            }
            try await Task.sleep(for: .milliseconds(50))
        }
        throw CancellationError()
    }
}

nonisolated private final class EmptyAccount: Account, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }

    override func store() -> Store {
        EmptyStore()
    }
}

nonisolated private final class EmptyStore: Store, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }
}
