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
        NSAttributedString(
            string: text,
            attributes: [
                .font: TypeScale.body.nsFont, .foregroundColor: color,
                .paragraphStyle: bodyLine, .baselineOffset: TypeScale.size(3),
            ])
    }

    static func symbolLine(_ symbol: String, _ text: String, style: TextStyle = TypeScale.detail)
        -> NSAttributedString
    {
        let line = NSMutableAttributedString()
        let font = style.nsFont
        if let image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: style.size, weight: .regular))
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
        NSAttributedString(
            string: " (edited)",
            attributes: [
                .font: TypeScale.small.nsFont, .foregroundColor: Palette.chatTextMuted,
                .paragraphStyle: bodyLine, .baselineOffset: TypeScale.size(3),
            ])
    }

    private static var bodyLine: NSParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = TypeScale.body.line
        paragraph.maximumLineHeight = TypeScale.body.line
        return paragraph
    }
}
