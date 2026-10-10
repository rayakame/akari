import AppKit

enum CellText {
    static func label(wrapping: Bool = false) -> NSTextField {
        let label =
            wrapping ? NSTextField(wrappingLabelWithString: "") : NSTextField(labelWithString: "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.drawsBackground = false
        label.isBordered = false
        if wrapping {
            label.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        }
        return label
    }

    static func body(_ text: String, color: NSColor) -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = 22
        paragraph.maximumLineHeight = 22
        return NSAttributedString(
            string: text,
            attributes: [
                .font: NSFont.systemFont(ofSize: 16), .foregroundColor: color,
                .paragraphStyle: paragraph, .baselineOffset: 3,
            ])
    }

    static func symbolLine(_ symbol: String, _ text: String, size: CGFloat = 14)
        -> NSAttributedString
    {
        let line = NSMutableAttributedString()
        let font = NSFont.systemFont(ofSize: size)
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: size, weight: .regular))
        {
            let attachment = NSTextAttachment()
            attachment.image = image
            line.append(NSAttributedString(attachment: attachment))
            line.append(NSAttributedString(string: " "))
        }
        line.append(NSAttributedString(string: text))
        line.addAttributes(
            [.font: font, .foregroundColor: Palette.textMuted],
            range: NSRange(location: 0, length: line.length))
        return line
    }

    static func editedMark() -> NSAttributedString {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = 22
        paragraph.maximumLineHeight = 22
        return NSAttributedString(
            string: " (edited)",
            attributes: [
                .font: NSFont.systemFont(ofSize: 12), .foregroundColor: Palette.chatTextMuted,
                .paragraphStyle: paragraph, .baselineOffset: 3,
            ])
    }
}
