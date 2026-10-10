import AppKit

/// The table's version of `InitialsAvatar`.
final class AvatarView: NSView {
    var name = "" {
        didSet { needsDisplay = true }
    }
    var userId: UInt64 = 0 {
        didSet { needsDisplay = true }
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: 40, height: 40)
    }

    override func draw(_ dirtyRect: NSRect) {
        InitialsAvatar.color(userId).setFill()
        NSBezierPath(ovalIn: bounds).fill()
        let letters = InitialsAvatar.letters(name) as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: bounds.height * 0.4, weight: .semibold),
            .foregroundColor: NSColor.white,
        ]
        let size = letters.size(withAttributes: attributes)
        letters.draw(
            at: NSPoint(x: bounds.midX - size.width / 2, y: bounds.midY - size.height / 2),
            withAttributes: attributes)
    }
}
