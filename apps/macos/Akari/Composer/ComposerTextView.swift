import AkariKit
import AppKit

// Return, Shift+Return, input-method composition and an empty Up arrow need AppKit's key
// commands; SwiftUI's text editors can't tell them apart.
final class ComposerTextView: NSTextView {
    static let lineHeight = TypeScale.body.line
    static let insets = NSSize(width: 16, height: 17)
    static let font = TypeScale.body.nsFont

    var onSubmit: () -> Void = {}
    var onEscape: () -> Void = {}
    var onShiftEscape: () -> Void = {}
    var onShiftPageUp: () -> Void = {}
    var onEditLastMessage: (() -> Void)?
    var onWindow: () -> Void = {}
    var onFocus: (Bool) -> Void = { _ in }
    /// Off in a message's row: the list scrolls its caret into view once the row has its height.
    var scrollsItself = true
    /// A message's editor in its row, which the composer leaves the focus to.
    var editsAMessage = false
    var currentEvent: () -> NSEvent? = { NSApp.currentEvent }

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

    // TextKit 1, as the editor's own measure lays the text out.
    static func plain() -> ComposerTextView {
        let storage = NSTextStorage()
        let layout = NSLayoutManager()
        storage.addLayoutManager(layout)
        let container = NSTextContainer(size: NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        layout.addTextContainer(container)
        let textView = ComposerTextView(frame: .zero, textContainer: container)
        textView.configure()
        return textView
    }

    static var attributes: [NSAttributedString.Key: Any] {
        let paragraph = NSMutableParagraphStyle()
        paragraph.minimumLineHeight = lineHeight
        paragraph.maximumLineHeight = lineHeight
        return [.font: font, .foregroundColor: Palette.textDefault, .paragraphStyle: paragraph]
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
        typingAttributes = Self.attributes
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
            onFocus(true)
        }
        return became
    }

    override func resignFirstResponder() -> Bool {
        let resigned = super.resignFirstResponder()
        if resigned {
            onFocus(false)
        }
        return resigned
    }

    override func scrollToVisible(_ rect: NSRect) -> Bool {
        scrollsItself ? super.scrollToVisible(rect) : true
    }

    // A selection in a shown message or a message's editor keeps the focus.
    func takeFocus() {
        guard let window, isEditable else {
            return
        }
        if let editor = window.firstResponder as? ComposerTextView, editor.editsAMessage,
            editor.window === window
        {
            return
        }
        if let editor = window.firstResponder as? NSTextView, editor !== self, editor.isFieldEditor,
            (editor.delegate as? NSView)?.window === window
        {
            return
        }
        if let message = window.firstResponder as? MessageTextView, message.window === window,
            message.selectedRange().length > 0
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
            if currentEvent()?.modifierFlags.contains(.shift) == true {
                return false
            }
            onSubmit()
            return true
        case #selector(NSResponder.insertLineBreak(_:)):
            // Ctrl+Return would insert a line separator (U+2028).
            insertNewlineIgnoringFieldEditor(nil)
            return true
        case #selector(NSResponder.moveUp(_:)) where string.isEmpty:
            onEditLastMessage?()
            return true
        case #selector(NSResponder.cancelOperation(_:)):
            // Shift+Escape and Cmd+. arrive as the same command; Cmd+. does nothing here, as in
            // the official client.
            guard let event = currentEvent(), event.isEscapeKey else {
                return true
            }
            if event.modifierFlags.contains(.shift) {
                onShiftEscape()
            } else {
                onEscape()
            }
            return true
        case #selector(NSResponder.pageUpAndModifySelection(_:)):
            // Shift+Page Up, or Shift+Fn+Up on a laptop.
            onShiftPageUp()
            return true
        default:
            return false
        }
    }
}
