import AppKit

/// Heights of message text without a view, as `MessageTextView` lays it out.
@MainActor
final class TextMeasurer {
    static let shared = TextMeasurer()

    // String drawing breaks lines as TextKit 1 does at half its cost; MessageTextViewTests hold
    // the two to the same heights.
    func height(_ text: NSAttributedString, width: CGFloat) -> CGFloat {
        let bounds = text.boundingRect(
            with: NSSize(width: width, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin])
        let padding = MessageText.padding(text)
        return ceil(bounds.height) + padding.top + padding.bottom
    }

    /// The widest line of `text` when nothing wraps it: what it needs to sit on one line each.
    func width(_ text: NSAttributedString) -> CGFloat {
        let bounds = text.boundingRect(
            with: NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin])
        return ceil(bounds.width)
    }

    static func usedHeight(_ layoutManager: NSLayoutManager, _ container: NSTextContainer)
        -> CGFloat
    {
        layoutManager.ensureLayout(for: container)
        return ceil(layoutManager.usedRect(for: container).height)
    }
}
