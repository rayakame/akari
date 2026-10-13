import AppKit

/// A message's body: selectable, not editable, laid out by TextKit 1.
final class MessageTextView: NSTextView {
    private var measuredWidth: CGFloat = -1
    private var padding: (top: CGFloat, bottom: CGFloat) = (0, 0)
    var onLink: (URL) -> Void = { _ in }
    // The message's own items, above the text's.
    var messageMenu: () -> [NSMenuItem] = { [] }
    let decorations = MessageDecorations()

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
        decorations.frame = frame
        if newSize.width != measuredWidth {
            measuredWidth = newSize.width
            invalidateIntrinsicContentSize()
            updateDecorations()
        }
    }

    // A selection survives a reconfigure that shows the same text, not an edit.
    func show(_ text: NSAttributedString) {
        guard let storage = textStorage else {
            return
        }
        let padding = MessageText.padding(text)
        if padding != self.padding {
            self.padding = padding
            // Only its top counts: the height is the view's own, from the padding.
            textContainerInset = NSSize(width: 0, height: padding.top)
        }
        let kept = storage.string == text.string ? selectedRanges : nil
        storage.setAttributedString(text)
        selectedRanges = kept ?? [NSValue(range: NSRange(location: 0, length: 0))]
        invalidateIntrinsicContentSize()
        updateDecorations()
    }

    // Behind the text in the superview, in step with this view's frame.
    override func viewWillMove(toSuperview newSuperview: NSView?) {
        super.viewWillMove(toSuperview: newSuperview)
        decorations.removeFromSuperview()
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        superview?.addSubview(decorations, positioned: .below, relativeTo: self)
        decorations.frame = frame
    }

    override func setFrameOrigin(_ newOrigin: NSPoint) {
        super.setFrameOrigin(newOrigin)
        decorations.frame = frame
    }

    override var isHidden: Bool {
        didSet { decorations.isHidden = isHidden }
    }

    private func updateDecorations() {
        let shapes = (layoutManager as? MessageLayoutManager)?.blocks() ?? []
        decorations.show(shapes, at: textContainerOrigin)
    }

    // The table routes Akari's own links and asks before masked ones; super would open them.
    override func clicked(onLink link: Any, at charIndex: Int) {
        if let url = link as? URL ?? (link as? String).flatMap(URL.init(string:)) {
            onLink(url)
        }
    }

    // Copying and dragging write what the selection shows, as plain text only (D63).
    override var writablePasteboardTypes: [NSPasteboard.PasteboardType] {
        [.string]
    }

    override func writeSelection(
        to pboard: NSPasteboard, types: [NSPasteboard.PasteboardType]
    ) -> Bool {
        guard types.contains(.string) else {
            return false
        }
        pboard.declareTypes([.string], owner: nil)
        return writeSelection(to: pboard, type: .string)
    }

    override func writeSelection(to pboard: NSPasteboard, type: NSPasteboard.PasteboardType)
        -> Bool
    {
        guard type == .string, let storage = textStorage else {
            return false
        }
        let text = selectedRanges.map { MessageText.plain(storage, $0.rangeValue) }
            .joined(separator: "\n")
        return pboard.setString(text, forType: .string)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = super.menu(for: event) ?? NSMenu()
        let items = messageMenu()
        if !items.isEmpty {
            menu.insertItem(.separator(), at: 0)
            for item in items.reversed() {
                menu.insertItem(item, at: 0)
            }
        }
        return menu
    }

    override func accessibilityValue() -> String? {
        textStorage.map(MessageText.accessible)
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

/// Code boxes, quote bars and hidden code blocks' covers, as colored layers behind a
/// message's text: drawn into the text view, a full-width box made its bitmap large enough
/// for Core Animation's costly path (about 40,000 pt²).
final class MessageDecorations: NSView {
    private var shapes: [MessageBlockShape] = []
    private var origin = NSPoint.zero

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    func show(_ shapes: [MessageBlockShape], at origin: NSPoint) {
        guard shapes != self.shapes || origin != self.origin else {
            return
        }
        self.shapes = shapes
        self.origin = origin
        for (index, shape) in shapes.enumerated() {
            let view: BlockView
            if index < subviews.count, let reused = subviews[index] as? BlockView {
                view = reused
            } else {
                view = BlockView()
                addSubview(view)
            }
            view.shape = shape
            view.frame = shape.rect.offsetBy(dx: origin.x, dy: origin.y)
        }
        for view in subviews.dropFirst(shapes.count) {
            view.removeFromSuperview()
        }
    }
}

// A shape's colors come from the theme, applied again when the appearance changes.
private final class BlockView: NSView {
    var shape = MessageBlockShape.codeBox(.zero) {
        didSet { applyColors() }
    }

    override init(frame: NSRect) {
        super.init(frame: frame)
        wantsLayer = true
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var wantsUpdateLayer: Bool { true }

    override func updateLayer() {
        applyColors()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        applyColors()
    }

    private func applyColors() {
        guard let layer else {
            return
        }
        effectiveAppearance.performAsCurrentDrawingAppearance {
            switch shape {
            case .codeBox:
                layer.backgroundColor = Palette.codeBackground.cgColor
                layer.borderColor = Palette.codeBorder.cgColor
                layer.borderWidth = 1
                layer.cornerRadius = 4
            case .quoteBar:
                layer.backgroundColor = Palette.quoteBar.cgColor
                layer.borderWidth = 0
                layer.cornerRadius = 2
            case .cover:
                layer.backgroundColor = Palette.spoilerHidden.cgColor
                layer.borderWidth = 0
                layer.cornerRadius = 4
            }
        }
    }
}
