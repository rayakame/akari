import AppKit
import SwiftUI

// A cursor rect, not a pushed cursor: it goes with its view, so a link that disappears under
// the pointer (the header's, once clicked) leaves no pointing hand behind.
struct CursorArea: NSViewRepresentable {
    let cursor: NSCursor

    init(_ cursor: NSCursor) {
        self.cursor = cursor
    }

    final class Area: NSView {
        var cursor = NSCursor.pointingHand

        /// Its bounds, unless a popout or modal's shield is over it.
        var shownRect: NSRect? {
            let center = convert(NSPoint(x: bounds.midX, y: bounds.midY), to: nil)
            return isShielded(at: center) ? nil : bounds
        }

        override func resetCursorRects() {
            if let shownRect {
                addCursorRect(shownRect, cursor: cursor)
            }
        }

        // Clicks go to what it lies under.
        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }
    }

    func makeNSView(context: Context) -> Area {
        Area()
    }

    func updateNSView(_ area: Area, context: Context) {
        area.cursor = cursor
        area.window?.invalidateCursorRects(for: area)
    }
}
