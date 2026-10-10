import AkariKit
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
    var onWindow: () -> Void = {}
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
        isAutomaticSpellingCorrectionEnabled = false
        isContinuousSpellCheckingEnabled = true
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

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindow()
    }

    override func becomeFirstResponder() -> Bool {
        let became = super.becomeFirstResponder()
        if became {
            LaunchLog.mark("composer focused")
        }
        return became
    }

    /// Becomes first responder unless the user is selecting message text that's still shown.
    func takeFocus() {
        guard let window, isEditable else {
            return
        }
        if let editor = window.firstResponder as? NSTextView, editor !== self, editor.isFieldEditor,
            (editor.delegate as? NSView)?.window === window
        {
            return
        }
        window.makeFirstResponder(self)
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
        case #selector(NSResponder.insertLineBreak(_:)):
            // Ctrl+Return would insert a line separator (U+2028).
            insertNewlineIgnoringFieldEditor(nil)
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
