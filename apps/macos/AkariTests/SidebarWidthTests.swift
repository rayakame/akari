import AppKit
import Foundation
import SwiftUI
import Testing

@testable import Akari

struct SidebarWidthTests {
    @Test
    func theWidthIsClampedAndRemembered() {
        let defaults = MemoryDefaults()

        #expect(SidebarWidth.stored(in: defaults) == 375)
        #expect(SidebarWidth.clamped(200) == 264)
        #expect(SidebarWidth.clamped(500) == 432)
        SidebarWidth.store(300, in: defaults)
        #expect(SidebarWidth.stored(in: defaults) == 300)
        SidebarWidth.store(9000, in: defaults)
        #expect(SidebarWidth.stored(in: defaults) == 432)
        #expect(SidebarWidth.listWidth(for: 375) == 302)
    }
}

// The sidebar's edge holds the resize cursor as the split's gap does: one push on the way in,
// held through a drag past the edge, one pop on the way out or when the edge goes.
@MainActor
struct SidebarResizerCursorTests {
    func hosted(_ cursor: ResizeCursor, width: Binding<CGFloat>) -> (NSWindow, NSHostingView<AnyView>) {
        let host = NSHostingView(
            rootView: AnyView(
                SidebarResizer(width: width, cursor: cursor, store: { _ in })
                    .frame(width: 4, height: 100)
                    .frame(width: 100, height: 100, alignment: .leading)
                    .takingFirstClicks()))
        let window = MessageTableTests.KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 100, height: 100), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        host.layoutSubtreeIfNeeded()
        return (window, host)
    }

    @Test(.enabled(if: firstClicks))
    func aDragPastTheEdgeHoldsTheCursorUntilItsRelease() async throws {
        var calls: [String] = []
        let cursor = ResizeCursor(push: { calls.append("push") }, pop: { calls.append("pop") })
        var width: CGFloat = 300
        let (window, _) = hosted(cursor, width: Binding(get: { width }, set: { width = $0 }))
        defer { window.orderOut(nil) }

        try ResizeCursorTests().drag(in: window, from: NSPoint(x: 2, y: 50), by: 60, release: false)
        let deadline = ContinuousClock.now + .seconds(5)
        while width == 300, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(width != 300)
        #expect(cursor.pushed, "held with the pointer off the edge")

        NSApp.postEvent(
            try #require(
                NSEvent.mouseEvent(
                    with: .leftMouseUp, location: NSPoint(x: 62, y: 50), modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                    clickCount: 1, pressure: 1)), atStart: false)
        while cursor.pushed, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(calls == ["push", "pop"])
    }

    @Test
    func anEdgeThatGoesAwayPopsItsCursor() async throws {
        var pops = 0
        let cursor = ResizeCursor(push: {}, pop: { pops += 1 })
        var width: CGFloat = 300
        let (window, host) = hosted(cursor, width: Binding(get: { width }, set: { width = $0 }))
        defer { window.orderOut(nil) }
        cursor.hover(true)

        host.rootView = AnyView(EmptyView())
        let deadline = ContinuousClock.now + .seconds(5)
        while pops == 0, ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }

        #expect(pops == 1)
        #expect(!cursor.pushed)
    }
}
