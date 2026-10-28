import AppKit
import Testing

@testable import Akari

extension Tag {
    /// Needs a window on screen or the app active: runs on CI, and locally only with
    /// `TEST_RUNNER_AKARI_SCREEN_TESTS=1`.
    @Tag static var onScreen: Self
}

/// A test window AppKit leaves where it is: a titled window ordered front is otherwise pulled
/// back onto a screen.
class OutOfSightWindow: NSWindow {
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }

    // AppKit moves a sheet onto a screen whatever its window: it comes transparent.
    override func beginSheet(
        _ sheetWindow: NSWindow, completionHandler handler: ((NSApplication.ModalResponse) -> Void)? = nil
    ) {
        sheetWindow.alphaValue = 0
        super.beginSheet(sheetWindow, completionHandler: handler)
    }
}

class OutOfSightPanel: NSPanel {
    override func constrainFrameRect(_ frameRect: NSRect, to screen: NSScreen?) -> NSRect {
        frameRect
    }
}

/// Every test window: out of sight (`TestHost.keepOutOfSight`), so a run never flashes a window
/// at whoever uses the Mac. `at` places it relative to the others, as on a screen.
@MainActor
enum TestWindow {
    static func make(
        size: NSSize, style: NSWindow.StyleMask = [.titled], at origin: NSPoint = .zero
    ) -> NSWindow {
        placed(
            OutOfSightWindow(
                contentRect: NSRect(origin: origin, size: size), styleMask: style, backing: .buffered,
                defer: false), at: origin)
    }

    static func panel(size: NSSize, style: NSWindow.StyleMask, at origin: NSPoint = .zero) -> NSPanel {
        placed(
            OutOfSightPanel(
                contentRect: NSRect(origin: origin, size: size), styleMask: style, backing: .buffered,
                defer: false), at: origin)
    }

    /// A window on the main screen, shown: only for what's measured on screen and run when asked
    /// (the performance probe), never for a test of the suite.
    static func visible(size: NSSize, style: NSWindow.StyleMask) -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(origin: NSPoint(x: 100, y: 100), size: size), styleMask: style,
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        return window
    }

    /// Whether `.onScreen` tests run.
    nonisolated static var screenTests: Bool { TestHost.showsWindows }

    /// A window of a test's own class, made out of sight as `make`'s are.
    static func hide<W: NSWindow>(_ window: W) -> W {
        hidden(window)
    }

    // AppKit lifts a new titled window above the Dock; `origin` is where its content goes.
    private static func placed<W: NSWindow>(_ window: W, at origin: NSPoint) -> W {
        let far = TestHost.origin
        let size = window.contentRect(forFrameRect: window.frame).size
        let frame = window.frameRect(forContentRect: NSRect(origin: origin, size: size))
        window.setFrameOrigin(NSPoint(x: far.x + frame.minX, y: far.y + frame.minY))
        return hidden(window)
    }

    private static func hidden<W: NSWindow>(_ window: W) -> W {
        window.isReleasedWhenClosed = false
        TestHost.keepOutOfSight(window)
        return window
    }
}
