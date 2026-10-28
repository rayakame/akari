import AppKit
import SwiftUI
import Testing

@testable import Akari

// Temporary CI probe: which window setup lets a posted drag reach the split gap on the runner.
@MainActor
struct ZZDragProbeTests {
    @Test(arguments: ["hidden", "onScreenAlpha0", "offScreenAlpha1", "onScreenAlpha1"])
    func probe(_ variant: String) async throws {
        var seen: [String] = []
        let monitor = NSEvent.addLocalMonitorForEvents(matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp]) { event in
            seen.append("\(event.type.rawValue)@\(event.windowNumber):\(event.locationInWindow)")
            return event
        }
        defer { monitor.map(NSEvent.removeMonitor) }
        let cursor = ResizeCursor(push: {}, pop: {})
        let split = SplitWidth()
        let host = NSHostingView(
            rootView: SplitGap(split: split, page: 1200, cursor: cursor)
                .frame(width: SplitWidth.gap, height: 100)
                .frame(width: 100, height: 100, alignment: .leading)
                .takingFirstClicks())
        let window = TestWindow.hide(
            MessageTableTests.KeyWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: 100, height: 100)), styleMask: [.titled],
                backing: .buffered, defer: false))
        switch variant {
        case "onScreenAlpha0": window.setFrameOrigin(NSPoint(x: 200, y: 200))
        case "offScreenAlpha1": window.alphaValue = 1
        case "onScreenAlpha1":
            window.setFrameOrigin(NSPoint(x: 200, y: 200))
            window.alphaValue = 1
        default: break
        }
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        host.layoutSubtreeIfNeeded()
        let before = split.width
        try ResizeCursorTests().drag(in: window, from: NSPoint(x: 4, y: 50), by: -40)
        let deadline = ContinuousClock.now + .seconds(3)
        while split.width == before, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        print("PROBE \(variant): width \(before) -> \(split.width), frame \(window.frame), alpha \(window.alphaValue), active \(NSApp.isActive), policy \(NSApp.activationPolicy().rawValue), screen \(String(describing: window.screen?.frame)), screens \(NSScreen.screens.map(\.frame)), events \(seen)")
    }
}
