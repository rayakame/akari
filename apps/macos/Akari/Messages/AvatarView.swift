import AppKit

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
        Self.draw(name: name, userId: userId, in: bounds)
    }

    static func draw(name: String, userId: UInt64, in rect: NSRect) {
        InitialsAvatar.color(userId).setFill()
        NSBezierPath(ovalIn: rect).fill()
        let letters = InitialsAvatar.letters(name) as NSString
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: rect.height * 0.4, weight: .semibold),
            .foregroundColor: NSColor.white,
        ]
        let size = letters.size(withAttributes: attributes)
        letters.draw(
            at: NSPoint(x: rect.midX - size.width / 2, y: rect.midY - size.height / 2),
            withAttributes: attributes)
    }
}
