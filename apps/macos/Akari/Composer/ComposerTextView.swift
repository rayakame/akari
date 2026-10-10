import AppKit

// Return, Shift+Return, input-method composition and an empty Up arrow need AppKit's key
// commands; SwiftUI's text editors can't tell them apart.
final class ComposerTextView: NSTextView {
    static let lineHeight: CGFloat = 22
    static let insets = NSSize(width: 16, height: 17)
    static let font = NSFont.systemFont(ofSize: 16)

    var onSubmit: () -> Void = {}
    var onEscape: () -> Void = {}
    var onEditLastMessage: (() -> Void)?
    var modifiers: () -> NSEvent.ModifierFlags = { NSApp.currentEvent?.modifierFlags ?? [] }

    static func scrollable() -> (NSScrollView, ComposerTextView) {
        let scrollView = NSScrollView()
        let textView = ComposerTextView(frame: .zero)
        textView.configure()
        scrollView.documentView = textView
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.horizontalScrollElasticity = .none
        return (scrollView, textView)
    }

    private func configure() {
        isRichText = false
        importsGraphics = false
        allowsUndo = true
        isAutomaticQuoteSubstitutionEnabled = false
        isAutomaticDashSubstitutionEnabled = false
        isAutomaticTextReplacementEnabled = false
        drawsBackground = false
        textContainerInset = Self.insets
        textContainer?.lineFragmentPadding = 0
        textContainer?.widthTracksTextView = true
        isVerticallyResizable = true
        isHorizontallyResizable = false
        autoresizingMask = [.width]
        minSize = NSSize(width: 0, height: 0)
        maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: .greatestFiniteMagnitude)
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = Self.lineHeight
        paragraph.maximumLineHeight = Self.lineHeight
        typingAttributes = [
            .font: Self.font, .foregroundColor: Palette.textDefault, .paragraphStyle: paragraph,
        ]
        font = Self.font
        textColor = Palette.textDefault
        insertionPointColor = Palette.textDefault
    }

    func fittingHeight(maxHeight: CGFloat) -> CGFloat {
        guard let layoutManager, let textContainer else {
            return Self.lineHeight + 2 * Self.insets.height
        }
        layoutManager.ensureLayout(for: textContainer)
        let used = max(layoutManager.usedRect(for: textContainer).height, Self.lineHeight)
        let height = used + 2 * Self.insets.height
        return min(height, max(maxHeight, Self.lineHeight + 2 * Self.insets.height))
    }

    override func doCommand(by selector: Selector) {
        if handle(selector) {
            return
        }
        super.doCommand(by: selector)
    }

    private func handle(_ selector: Selector) -> Bool {
        // Return during input-method composition commits the composition.
        guard !hasMarkedText() else {
            return false
        }
        switch selector {
        case #selector(NSResponder.insertNewline(_:)):
            // Shift+Return arrives as the same command; it adds a line.
            if modifiers().contains(.shift) {
                return false
            }
            onSubmit()
            return true
        case #selector(NSResponder.moveUp(_:)) where string.isEmpty:
            // Reserved for editing the last message.
            onEditLastMessage?()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            onEscape()
            return true
        default:
            return false
        }
    }
}
