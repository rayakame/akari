import AppKit
import Testing

@testable import Akari

// The suites run while someone works at the Mac: the host stays in the background and every test
// window out of sight, yet lays out, draws, hit-tests and takes events as on screen.
@MainActor
struct TestWindowTests {
    final class Recorder: NSView {
        var clicks = 0

        // The host is never active: no window is key, every click a first one.
        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func mouseDown(with event: NSEvent) {
            clicks += 1
        }
    }

    func onAScreen(_ window: NSWindow) -> Bool {
        NSScreen.screens.contains { $0.frame.intersects(window.frame) }
    }

    func inSight(_ window: NSWindow) -> Bool {
        window.alphaValue > 0 && onAScreen(window)
    }

    @Test
    func theHostIsAnAccessoryApp() {
        #expect(NSApp.activationPolicy() == .accessory)
    }

    // The host's own window too.
    @Test
    func noWindowOfTheHostIsInSight() {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        window.orderFrontRegardless()
        defer { window.close() }

        #expect(!NSApp.windows.contains { $0.isVisible && inSight($0) })
    }

    @Test
    func aWindowStaysOutOfSightOrderedFrontAndResized() {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        defer { window.close() }
        window.orderFrontRegardless()
        #expect(window.isVisible)
        #expect(window.alphaValue == 0)
        #expect(!onAScreen(window), "AppKit pulls titled windows onto a screen")

        window.setContentSize(NSSize(width: 900, height: 700))
        #expect(!onAScreen(window))
        #expect(window.contentView?.bounds.size == NSSize(width: 900, height: 700))
    }

    // Windows placed relative to each other keep their arrangement; a panel over a hidden window
    // stays over it.
    @Test
    func windowsKeepTheirArrangement() {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        let panel = TestWindow.panel(size: NSSize(width: 100, height: 100), style: [.borderless], at: NSPoint(x: 40, y: 40))
        let shield = TestWindow.panel(size: window.frame.size, style: [.borderless])
        defer {
            for each in [window, panel, shield] {
                each.close()
            }
        }

        #expect(panel.frame.origin == NSPoint(x: window.frame.minX + 40, y: window.frame.minY + 40))
        #expect(shield.frame == window.frame)
    }

    // Layout, hit testing and sent events work out of sight.
    @Test
    func itLaysOutHitTestsAndTakesEvents() throws {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        defer { window.close() }
        let content = try #require(window.contentView)
        let target = Recorder(frame: NSRect(x: 100, y: 100, width: 50, height: 50))
        content.addSubview(target)
        window.orderFrontRegardless()
        window.layoutIfNeeded()

        let point = target.convert(NSPoint(x: 25, y: 25), to: nil)
        #expect(content.hitTest(content.superview?.convert(point, from: nil) ?? point) === target)
        window.sendEvent(
            try #require(
                NSEvent.mouseEvent(
                    with: .leftMouseDown, location: point, modifierFlags: [], timestamp: 0,
                    windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                    pressure: 1)))
        #expect(target.clicks == 1)
    }
}
