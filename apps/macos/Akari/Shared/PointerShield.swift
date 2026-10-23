import AppKit

/// The layer under a popout or a modal, over the rest of its window: it takes every press
/// outside them and shows the arrow, and while it's up the views behind it ignore the pointer and
/// take no focus, as nothing behind Discord's popouts and modals reacts.
class PointerShield: NSView {
    /// Posted with the window when a shield goes up or down in it.
    static let changed = Notification.Name("akari.pointerShieldChanged")

    private static let raised = NSHashTable<PointerShield>.weakObjects()
    private var area: NSTrackingArea?

    static func isUp(in window: NSWindow?) -> Bool {
        guard let window else {
            return false
        }
        return raised.allObjects.contains { $0.window === window }
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        if newWindow == nil, let window {
            Self.announce(in: window.windowNumber)
        }
        super.viewWillMove(toWindow: newWindow)
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else {
            return
        }
        Self.raised.add(self)
        NSCursor.arrow.set()
        Self.announce(in: window.windowNumber)
    }

    // Next turn: SwiftUI has laid out what's in front, and a shield going down is gone by then. A
    // closing window may be deallocating, so it's looked up by number, never referenced.
    private static func announce(in number: Int) {
        Task { @MainActor in
            guard let window = NSApp.window(withWindowNumber: number) else {
                return
            }
            if isUp(in: window), window.firstResponder is any BehindShields {
                window.makeFirstResponder(nil)
            }
            window.resetCursorRects()
            NotificationCenter.default.post(name: changed, object: window)
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area {
            removeTrackingArea(area)
        }
        let area = NSTrackingArea(
            rect: .zero, options: [.cursorUpdate, .activeInKeyWindow, .inVisibleRect], owner: self)
        addTrackingArea(area)
        self.area = area
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.arrow.set()
    }

    // The wheel scrolls nothing behind either.
    override func scrollWheel(with event: NSEvent) {}
}

/// A view that's always behind a popout or modal (the message list, the composer): while a shield
/// is up it ignores the pointer and takes no focus, and gives up the focus it has.
protocol BehindShields: NSView {}

extension NSView {
    /// A popout or modal's shield is up in this view's window; see `BehindShields`.
    var isBehindShield: Bool {
        PointerShield.isUp(in: window)
    }

    /// The view the window hits at `point`, in window coordinates, is a shield: a view that may
    /// also be in front of one (a popout's own) asks this.
    func isShielded(at point: NSPoint) -> Bool {
        guard let content = window?.contentView else {
            return false
        }
        let inSuperview = content.superview?.convert(point, from: nil) ?? point
        return content.hitTest(inSuperview) is PointerShield
    }
}
