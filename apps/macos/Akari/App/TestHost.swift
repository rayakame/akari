import AppKit
import SwiftUI

/// The app as the host of its hosted tests, which run while someone works at the Mac: no Dock
/// icon, no menu bar, never the frontmost app, and its windows out of sight.
@MainActor
enum TestHost {
    /// Windows a test needs on screen, or the app active: on CI, and locally when asked for.
    nonisolated static var showsWindows: Bool {
        ProcessInfo.processInfo.environment["AKARI_SCREEN_TESTS"] != nil
    }

    private static var observer: NSObjectProtocol?

    static func start() {
        NSApp.setActivationPolicy(.accessory)
        guard !showsWindows else {
            return
        }
        // Launching activates the app; the focus goes back to whatever had it.
        observer = NotificationCenter.default.addObserver(
            forName: NSApplication.didBecomeActiveNotification, object: nil, queue: .main
        ) { _ in
            MainActor.assumeIsolated { NSApp.deactivate() }
        }
    }

    /// Beyond every screen's frame. Windows placed from here keep their arrangement.
    static var origin: NSPoint {
        let screens = NSScreen.screens.reduce(NSRect.zero) { $0.union($1.frame) }
        return NSPoint(x: screens.maxX + 20_000, y: screens.maxY + 20_000)
    }

    /// Transparent and beyond every screen: layout, drawing into bitmaps, hit tests and sent
    /// events work as on screen. A window already out there stays where it is.
    static func keepOutOfSight(_ window: NSWindow) {
        window.alphaValue = 0
        let screens = NSScreen.screens.map(\.frame)
        if screens.contains(where: { $0.intersects(window.frame) }) || screens.isEmpty {
            let origin = origin
            window.setFrameOrigin(
                NSPoint(x: window.frame.minX + origin.x, y: window.frame.minY + origin.y))
        }
    }
}

/// The test host's own window, out of sight like the tests' windows.
struct TestHostWindow: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        Anchor()
    }

    func updateNSView(_ nsView: NSView, context: Context) {}

    private final class Anchor: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            if let window {
                TestHost.keepOutOfSight(window)
                // AppKit keeps a titled window it shows partly on a screen.
                window.ignoresMouseEvents = true
            }
        }
    }
}
