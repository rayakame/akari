import AppKit

/// A message's body: selectable, not editable, laid out by TextKit 1.
final class MessageTextView: NSTextView {
    private var measuredWidth: CGFloat = -1
    private var padding: (top: CGFloat, bottom: CGFloat) = (0, 0)

    init() {
        let storage = NSTextStorage()
        let layoutManager = MessageLayoutManager()
        storage.addLayoutManager(layoutManager)
        let container = NSTextContainer(
            size: NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude))
        container.lineFragmentPadding = 0
        container.widthTracksTextView = true
        layoutManager.addTextContainer(container)
        super.init(frame: .zero, textContainer: container)
        translatesAutoresizingMaskIntoConstraints = false
        isEditable = false
        isSelectable = true
        drawsBackground = false
        isRichText = true
        importsGraphics = false
        allowsUndo = false
        textContainerInset = .zero
        // Links keep their own colors: pills are links too.
        linkTextAttributes = [.cursor: NSCursor.pointingHand]
        isVerticallyResizable = false
        isHorizontallyResizable = false
        setContentHuggingPriority(.required, for: .vertical)
        setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        guard let layoutManager, let textContainer else {
            return NSSize(width: NSView.noIntrinsicMetric, height: 0)
        }
        return NSSize(
            width: NSView.noIntrinsicMetric,
            height: TextMeasurer.usedHeight(layoutManager, textContainer) + padding.top
                + padding.bottom)
    }


    override func setFrameSize(_ newSize: NSSize) {
        super.setFrameSize(newSize)
        if newSize.width != measuredWidth {
            measuredWidth = newSize.width
            invalidateIntrinsicContentSize()
        }
    }

    func show(_ text: NSAttributedString) {
        let padding = MessageText.padding(text)
        if padding != self.padding {
            self.padding = padding
            // Only its top counts: the height is the view's own, from the padding.
            textContainerInset = NSSize(width: 0, height: padding.top)
        }
        textStorage?.setAttributedString(text)
        invalidateIntrinsicContentSize()
    }

    override func draw(_ dirtyRect: NSRect) {
        (layoutManager as? MessageLayoutManager)?.drawBlocks(at: textContainerOrigin)
        super.draw(dirtyRect)
    }

    // Where the first line's glyphs sit, after the paragraph's baseline offset.
    override var firstBaselineOffsetFromTop: CGFloat {
        guard let layoutManager, let storage = textStorage, storage.length > 0 else {
            return 0
        }
        layoutManager.ensureLayout(forCharacterRange: NSRange(location: 0, length: 1))
        let line = layoutManager.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil)
        return padding.top + line.minY + layoutManager.location(forGlyphAt: 0).y
    }

    // A non-editable text view would swallow Escape; the list handles it.
    override func cancelOperation(_ sender: Any?) {
        nextResponder?.tryToPerform(#selector(NSResponder.cancelOperation(_:)), with: sender)
    }
}
