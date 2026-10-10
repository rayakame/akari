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
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        window.appearance = NSAppearance(named: .darkAqua)
        let host = NSHostingView(rootView: SessionView(session: session) { _ in EmptyView() })
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let pixel = { (x: CGFloat, y: CGFloat) in
            rep.colorAt(x: Int(x * scale), y: Int(y * scale))?.usingColorSpace(.sRGB)
        }
        let right = host.bounds.width - 20

        // The bitmap's color space shifts the tokens, so colors are compared with the rail's.
        let rail = try #require(pixel(20, 300))
        #expect(!Self.same(pixel(right, 300), rail), "the page has its own color")
        #expect(Self.same(pixel(right, 10), rail), "over the page")
        #expect(Self.same(pixel(20, 10), rail), "over the rail")
    }

    private static func same(_ color: NSColor?, _ other: NSColor) -> Bool {
        guard let color else {
            return false
        }
        return abs(color.redComponent - other.redComponent) < 0.5 / 255
            && abs(color.greenComponent - other.greenComponent) < 0.5 / 255
            && abs(color.blueComponent - other.blueComponent) < 0.5 / 255
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
