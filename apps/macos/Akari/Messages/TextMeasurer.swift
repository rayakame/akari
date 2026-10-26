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

    /// The widest line of `text` when nothing wraps it, its indents included: what it needs to
    /// sit on one line each.
    func width(_ text: NSAttributedString) -> CGFloat {
        let storage = NSTextStorage(attributedString: text)
        let layoutManager = NSLayoutManager()
        let container = NSTextContainer(
            size: NSSize(width: Self.unbounded, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
        layoutManager.ensureLayout(for: container)
        var widest: CGFloat = 0
        let glyphs = NSRange(location: 0, length: layoutManager.numberOfGlyphs)
        layoutManager.enumerateLineFragments(forGlyphRange: glyphs) { _, used, _, range, _ in
            let index = layoutManager.characterIndexForGlyph(at: range.location)
            let style = storage.attribute(.paragraphStyle, at: index, effectiveRange: nil) as? NSParagraphStyle
            // A negative tail indent is room kept on the right (a code block's padding).
            widest = max(widest, used.maxX + max(0, -(style?.tailIndent ?? 0)))
        }
        return ceil(widest)
    }

    private static let unbounded: CGFloat = 100_000

    static func usedHeight(_ layoutManager: NSLayoutManager, _ container: NSTextContainer)
        -> CGFloat
    {
        layoutManager.ensureLayout(for: container)
        return ceil(layoutManager.usedRect(for: container).height)
    }
}
