import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// One window for all of them, and one of them renames it.
@MainActor @Suite(.serialized)
struct WindowChromeTests {
    // The session draws the title itself, centered on the window; the window keeps its title
    // for the Window menu, Mission Control and VoiceOver.
    @Test
    func theTitleBarShowsTheSidebarColorAndNoNativeTitle() async throws {
        let window = try await mainWindow()

        #expect(window.titlebarAppearsTransparent)
        #expect(window.titleVisibility == .hidden)
        #expect(window.title == "Akari")
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

    // A session names the window through navigationTitle; SwiftUI shows such a title in the
    // title bar unless the scene says not to.
    @Test
    func noNativeTitleShowsWhenTheWindowIsNamed() async throws {
        let window = try await mainWindow()
        let shared = TestingWindowTitle.shared
        defer { shared.text = "Akari" }

        shared.text = "testserver"
        for _ in 0..<40 where window.title != "testserver" {
            try await Task.sleep(for: .milliseconds(50))
        }
        try await Task.sleep(for: .milliseconds(300))

        #expect(window.title == "testserver")
        let shown = nativeTitles(in: window).map { "\(type(of: $0)) at \($0.frame)" }
        #expect(shown.isEmpty, "the title bar draws the title in \(shown)")
    }

    // What AppKit draws in the title bar sits beside the content view, not in it.
    private func nativeTitles(in window: NSWindow) -> [NSView] {
        guard let content = window.contentView, let frame = content.superview else {
            return []
        }
        func views(_ view: NSView) -> [NSView] {
            [view] + view.subviews.flatMap(views)
        }
        return frame.subviews.filter { $0 !== content }.flatMap(views).filter { view in
            guard let field = view as? NSTextField, field.stringValue == window.title else {
                return false
            }
            return !field.isHiddenOrHasHiddenAncestor && field.alphaValue > 0
                && !field.bounds.isEmpty
        }
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

    override func setReading(channelId: ChannelId?) {}
    override func markRead(channelId: ChannelId) {}
    override func markGuildsRead(guildIds: [GuildId]) {}
}

nonisolated private final class EmptyStore: Store, @unchecked Sendable {
    init() {
        super.init(noHandle: NoHandle())
    }

    required init(unsafeFromHandle handle: UInt64) {
        fatalError("a fake has no Rust object")
    }
}
