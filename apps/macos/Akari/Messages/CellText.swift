import AppKit

enum CellText {
    // Chrome Discord doesn't let the user select. Selectable text goes in a MessageTextView, which
    // a click doesn't redraw as a selectable label's field editor does.
    static func label() -> NSTextField {
        let label = RowLabel(labelWithString: "")
        label.translatesAutoresizingMaskIntoConstraints = false
        label.drawsBackground = false
        label.isBordered = false
        label.isSelectable = false
        return label
    }

    static func plain(_ text: String, style: TextStyle, color: NSColor) -> NSAttributedString {
        NSAttributedString(string: text, attributes: [.font: style.nsFont, .foregroundColor: color])
    }

    // How far body text sits above its line's baseline, centering it on the 22 line.
    static var bodyRaise: CGFloat { TypeScale.size(3) }

    static func body(_ text: String, color: NSColor, style: TextStyle = TypeScale.body)
        -> NSAttributedString
    {
        NSAttributedString(
            string: text,
            attributes: [
                .font: style.nsFont, .foregroundColor: color,
                .paragraphStyle: bodyLine, .baselineOffset: bodyRaise,
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

    static func editedMark(tooltip: String? = nil) -> NSAttributedString {
        let attributes: [NSAttributedString.Key: Any] = [
            .font: TypeScale.edited.nsFont, .foregroundColor: Palette.chatTextMuted,
            .paragraphStyle: bodyLine, .baselineOffset: TypeScale.size(3),
        ]
        let mark = NSMutableAttributedString(string: " (edited)", attributes: attributes)
        if let tooltip {
            mark.addAttribute(.toolTip, value: tooltip, range: NSRange(location: 1, length: mark.length - 1))
        }
        return mark
    }

    private static var bodyLine: NSParagraphStyle {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = TypeScale.body.line
        paragraph.maximumLineHeight = TypeScale.body.line
        return paragraph
    }
}
