import AppKit

// In the space a group leaves above its first message; that message gives up its own (MessageCell).
final class NewMessagesDivider: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("newMessages")
    static let height: CGFloat = 16

    private static let inset: CGFloat = 16
    private static let tagHeight: CGFloat = 13
    private static let tagPadding: CGFloat = 4
    private static let text = "NEW"
    private static let textWidth = CTLineGetTypographicBounds(
        CTLineCreateWithAttributedString(
            NSAttributedString(string: text, attributes: [.font: TypeScale.newTag.nsFont])),
        nil, nil, nil
    ).rounded(.up)

    override var isFlipped: Bool { true }

    /// 1 pt, from the list's left inset to the tag.
    var line: NSRect {
        NSRect(x: Self.inset, y: middle, width: max(0, tagFrame.minX - Self.inset), height: 1)
    }

    /// The "NEW" tag at the line's right end, centered on it.
    var tagFrame: NSRect {
        let width = Self.textWidth + 2 * Self.tagPadding
        return NSRect(
            x: bounds.maxX - Self.inset - width, y: middle + 0.5 - Self.tagHeight / 2,
            width: width, height: Self.tagHeight)
    }

    private var middle: CGFloat {
        bounds.midY.rounded(.down)
    }

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        // The table sizes the row; a required height would conflict while it moves row views.
        let height = heightAnchor.constraint(equalToConstant: Self.height)
        height.priority = .init(999)
        height.isActive = true
        setAccessibilityElement(true)
        setAccessibilityRole(.staticText)
        setAccessibilityLabel("New messages")
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        Palette.newMessages.setFill()
        line.fill()
        let tag = tagFrame
        NSBezierPath(roundedRect: tag, xRadius: 3, yRadius: 3).fill()
        guard let context = NSGraphicsContext.current?.cgContext else {
            return
        }
        let font = TypeScale.newTag.nsFont
        let text = CTLineCreateWithAttributedString(
            NSAttributedString(
                string: Self.text,
                attributes: [.font: font, .foregroundColor: Palette.newMessagesText]))
        let width = CTLineGetTypographicBounds(text, nil, nil, nil)
        // Centered by the capitals' height, as the APP tag's (MessageCell).
        context.saveGState()
        context.textMatrix = CGAffineTransform(scaleX: 1, y: -1)
        context.textPosition = CGPoint(
            x: tag.midX - width / 2, y: tag.midY + font.capHeight / 2)
        CTLineDraw(text, context)
        context.restoreGState()
    }
}
