import AppKit

/// A message's body: selectable, not editable, laid out by TextKit 1.
final class MessageTextView: NSTextView {
    private var measuredWidth: CGFloat = -1
    private var padding: (top: CGFloat, bottom: CGFloat) = (0, 0)
    private var linkElements: [LinkElement]?
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
        if #available(macOS 15.0, *) {
            writingToolsBehavior = .none
        }
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
            linkElements = nil
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
        linkElements = nil
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
        window?.invalidateCursorRects(for: self)
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

    // NSTextView's own menu selects the word under the pointer and offers to look it up, which
    // would read a hidden spoiler.
    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()
        for item in messageMenu() {
            menu.addItem(item)
        }
        if selectedRange().length > 0 {
            if !menu.items.isEmpty {
                menu.addItem(.separator())
            }
            let copy = NSMenuItem(title: "Copy", action: #selector(copy(_:)), keyEquivalent: "")
            copy.target = self
            menu.addItem(copy)
        }
        return menu.items.isEmpty ? nil : menu
    }

    override func quickLook(with event: NSEvent) {
        guard !looksUpHiddenText(at: convert(event.locationInWindow, from: nil)) else {
            return
        }
        super.quickLook(with: event)
    }

    // The glyph under the point, not the nearest insertion point, which can be past a spoiler.
    func looksUpHiddenText(at point: NSPoint) -> Bool {
        guard let layoutManager, let textContainer, let storage = textStorage, storage.length > 0
        else {
            return false
        }
        let inContainer = NSPoint(
            x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyph = layoutManager.glyphIndex(for: inContainer, in: textContainer)
        let index = layoutManager.characterIndexForGlyph(at: glyph)
        var ranges = [
            NSRange(location: index, length: 1),
            selectionRange(
                forProposedRange: NSRange(location: index, length: 0), granularity: .selectByWord),
        ]
        if NSLocationInRange(index, selectedRange()) {
            ranges.append(selectedRange())
        }
        return ranges.contains { hidesText($0, storage) }
    }

    private func hidesText(_ range: NSRange, _ storage: NSTextStorage) -> Bool {
        let range = NSIntersectionRange(range, NSRange(location: 0, length: storage.length))
        var hides = false
        storage.enumerateAttribute(.akariSpoiler, in: range) { value, _, stop in
            if (value as? MessageText.Spoiler)?.hidden == true {
                hides = true
                stop.pointee = true
            }
        }
        return hides
    }

    // A hidden code block reveals on a click anywhere on its cover, its padding included.
    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if let spoiler = decorations.cover(at: point),
            let url = URL(string: "akari://spoiler/\(spoiler)")
        {
            onLink(url)
            return
        }
        super.mouseDown(with: event)
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        for rect in decorations.covers {
            addCursorRect(rect, cursor: .pointingHand)
        }
    }

    override func accessibilityValue() -> String? {
        textStorage.map(MessageText.accessible)
    }

    override func accessibilityString(for range: NSRange) -> String? {
        guard let storage = textStorage, NSMaxRange(range) <= storage.length else {
            return nil
        }
        return MessageText.accessible(storage.attributedSubstring(from: range))
    }

    override func accessibilityAttributedString(for range: NSRange) -> NSAttributedString? {
        accessibilityString(for: range).map { NSAttributedString(string: $0) }
    }

    override func accessibilitySelectedText() -> String? {
        accessibilityString(for: selectedRange())
    }

    override func accessibilityRTF(for range: NSRange) -> Data? {
        accessibilityAttributedString(for: range).flatMap { text in
            text.rtf(from: NSRange(location: 0, length: text.length))
        }
    }

    // NSTextView's link elements carry their text, a hidden spoiler's too, and can't be filtered:
    // asking one anything asks the view for its children.
    override func accessibilityChildren() -> [Any]? {
        links()
    }

    nonisolated override func accessibilityHitTest(_ point: NSPoint) -> Any? {
        // AppKit asks on the main thread; the element isn't Sendable.
        nonisolated(unsafe) var hit: Any?
        MainActor.assumeIsolated {
            hit = links().first { $0.accessibilityFrame().contains(point) } ?? self
        }
        return hit
    }

    private func links() -> [LinkElement] {
        if let linkElements {
            return linkElements
        }
        guard let storage = textStorage, let layoutManager, let textContainer else {
            return []
        }
        var elements: [LinkElement] = []
        var spoilers: Set<UInt16> = []
        storage.enumerateAttribute(.link, in: NSRange(location: 0, length: storage.length)) {
            value, range, _ in
            guard let url = value as? URL ?? (value as? String).flatMap(URL.init(string:)) else {
                return
            }
            let spoiler = storage.attribute(.akariSpoiler, at: range.location, effectiveRange: nil)
            let hidden = (spoiler as? MessageText.Spoiler).flatMap { $0.hidden ? $0.index : nil }
            if let hidden, !spoilers.insert(hidden).inserted {
                return
            }
            let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
            let rect = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
                .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
            // An element's frame counts from its parent's bottom left, flipped or not.
            let frame = NSRect(
                x: rect.minX, y: bounds.height - rect.maxY, width: rect.width, height: rect.height)
            elements.append(
                LinkElement(
                    label: hidden == nil ? MessageText.plain(storage, range) : "Spoiler",
                    frame: frame, parent: self
                ) { [weak self] in self?.onLink(url) })
        }
        linkElements = elements
        return elements
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

// A link in a message as assistive apps see it; pressing it is a click.
nonisolated final class LinkElement: NSAccessibilityElement {
    private let press: @MainActor () -> Void

    init(label: String, frame: NSRect, parent: NSView, press: @escaping @MainActor () -> Void) {
        self.press = press
        super.init()
        setAccessibilityRole(.link)
        setAccessibilityLabel(label)
        setAccessibilityParent(parent)
        setAccessibilityFrameInParentSpace(frame)
    }

    override func accessibilityPerformPress() -> Bool {
        let press = press
        MainActor.assumeIsolated { press() }
        return true
    }
}

/// Code boxes, quote bars, pills and spoilers' backgrounds as layers behind a message's text,
/// kept out of the text's bitmap (apps/macos/README.md, Performance).
final class MessageDecorations: NSView {
    private var shapes: [MessageBlockShape] = []
    private var origin = NSPoint.zero

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    var covers: [NSRect] {
        shapes.compactMap { shape in
            guard case .cover(let rect, _) = shape else {
                return nil
            }
            return rect.offsetBy(dx: origin.x, dy: origin.y)
        }
    }

    func cover(at point: NSPoint) -> UInt16? {
        for shape in shapes {
            if case .cover(let rect, let spoiler) = shape,
                rect.offsetBy(dx: origin.x, dy: origin.y).contains(point)
            {
                return spoiler
            }
        }
        return nil
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
            case .revealed:
                layer.backgroundColor = Palette.spoilerRevealed.cgColor
                layer.borderWidth = 0
                layer.cornerRadius = 3
            case .pill(_, let pill):
                layer.borderWidth = 0
                layer.cornerRadius = 3
                switch pill {
                case .mention:
                    layer.backgroundColor = Palette.mentionBackground.cgColor
                case .role(let color):
                    layer.backgroundColor = NSColor(rgb: color, alpha: 0.1).cgColor
                case .timestamp:
                    layer.backgroundColor = Palette.timestampBackground.cgColor
                case .code:
                    layer.backgroundColor = Palette.codeBackground.cgColor
                    layer.borderColor = Palette.codeBorder.cgColor
                    layer.borderWidth = 1
                    layer.cornerRadius = 4
                }
            }
        }
    }
}
