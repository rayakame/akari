import AppKit
import SwiftUI

/// The cursor over what lies under it, from its own tracking: it works in a window that isn't
/// key (More Active Threads, a panel before it takes the key), where cursor rects don't. Leaving
/// it, the pointer shows what lies there (`CursorArea.cursor(at:in:)`), whichever of the two
/// tracking events comes first.
struct CursorArea: NSViewRepresentable {
    let cursor: NSCursor

    init(_ cursor: NSCursor) {
        self.cursor = cursor
    }

    final class Area: NSView {
        var cursor = NSCursor.pointingHand
        private var tracking: NSTrackingArea?

        private static let shown = NSHashTable<Area>.weakObjects()

        override init(frame: NSRect) {
            super.init(frame: frame)
            Self.shown.add(self)
        }

        required init?(coder: NSCoder) {
            nil
        }

        /// The cursor of the smallest area at `point` (window coordinates) in `window`.
        static func cursor(at point: NSPoint, in window: NSWindow) -> NSCursor? {
            shown.allObjects
                .filter { $0.window === window && !$0.isHiddenOrHasHiddenAncestor }
                .map { (area: $0, rect: $0.convert($0.bounds, to: nil)) }
                .filter { $0.rect.contains(point) }
                .min { $0.rect.width * $0.rect.height < $1.rect.width * $1.rect.height }?
                .area.cursor
        }

        // Clicks go to what it lies under.
        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if let tracking {
                removeTrackingArea(tracking)
            }
            let tracking = NSTrackingArea(
                rect: .zero,
                options: [.mouseEnteredAndExited, .mouseMoved, .activeInActiveApp, .inVisibleRect],
                owner: self)
            addTrackingArea(tracking)
            self.tracking = tracking
        }

        override func mouseEntered(with event: NSEvent) {
            show(event)
        }

        override func mouseMoved(with event: NSEvent) {
            show(event)
        }

        override func mouseExited(with event: NSEvent) {
            show(event)
        }

        private func show(_ event: NSEvent) {
            if let window {
                CursorArea.cursor(at: event.locationInWindow, in: window).set()
            }
        }
    }

    /// What the pointer shows at `point` (window coordinates): an area's cursor there, else the
    /// choice of a cursor authority over the view there (the message list), else the I-beam
    /// over editable text, else the arrow.
    @MainActor
    static func cursor(at point: NSPoint, in window: NSWindow) -> NSCursor {
        if let cursor = Area.cursor(at: point, in: window) {
            return cursor
        }
        guard let content = window.contentView,
            let hit = content.hitTest(content.superview?.convert(point, from: nil) ?? point)
        else {
            return .arrow
        }
        var view: NSView? = hit
        while let current = view {
            if let authority = current as? CursorAuthority {
                return authority.cursor(at: point) ?? .arrow
            }
            if let text = current as? NSTextView, text.isEditable {
                return .iBeam
            }
            if let field = current as? NSTextField, field.isEditable {
                return .iBeam
            }
            view = current.superview
        }
        return .arrow
    }

    func makeNSView(context: Context) -> Area {
        Area()
    }

    func updateNSView(_ area: Area, context: Context) {
        area.cursor = cursor
    }
}

/// A view that decides the cursor over itself, for what's under the pointer (the message list).
@MainActor
protocol CursorAuthority: NSView {
    /// `point` in window coordinates; `nil` outside its region.
    func cursor(at point: NSPoint) -> NSCursor?
}
