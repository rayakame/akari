import AkariKit
import AppKit
import Testing

@testable import Akari

// A child panel holding an AppKit view, as the emoji picker's.
@MainActor
struct ChildWindowsViewTests {
    @Test
    func aPanelHoldsAnAppKitViewOverItsShieldAndGoesWithTearDown() throws {
        let window = TestWindow.make(size: NSSize(width: 600, height: 400))
        let windows = ChildWindows(keyable: true)
        var keyed: [NSWindow] = []
        windows.focus = WindowFocus(makeKey: { keyed.append($0) })
        let view = NSView()
        let frame = NSRect(x: window.frame.minX + 20, y: window.frame.minY + 20, width: 200, height: 100)

        windows.show(view, at: frame, in: window, dimmed: false)

        let panel = try #require(windows.panel)
        #expect(panel.contentView === view)
        #expect(panel.frame == frame)
        #expect(windows.shield?.frame == window.frame)
        #expect(keyed == [panel])

        windows.show(view, at: frame.offsetBy(dx: 10, dy: 0), in: window, dimmed: false)
        #expect(windows.panel === panel)
        #expect(panel.frame.minX == frame.minX + 10)
        let other = NSView()
        windows.show(other, at: frame, in: window, dimmed: false)
        #expect(panel.contentView === other)
        #expect(keyed.count == 1, "the panel took the key once")

        windows.tearDown(refocus: false)
        #expect(windows.panel == nil)
        #expect(window.childWindows?.isEmpty != false)
        window.close()
    }
}
