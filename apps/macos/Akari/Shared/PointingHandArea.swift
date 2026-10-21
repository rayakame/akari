import AppKit
import SwiftUI

// A cursor rect, not a pushed cursor: it goes with its view, so a link that disappears under
// the pointer (the header's, once clicked) leaves no pointing hand behind.
struct PointingHandArea: NSViewRepresentable {
    final class Area: NSView {
        override func resetCursorRects() {
            addCursorRect(bounds, cursor: .pointingHand)
        }

        // Clicks go to the link it lies under.
        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }
    }

    func makeNSView(context: Context) -> Area {
        Area()
    }

    func updateNSView(_ area: Area, context: Context) {
        area.window?.invalidateCursorRects(for: area)
    }
}
