import AkariKit
import AppKit

/// Selectable, not editable, laid out by TextKit 1: a message's body, and the other text Discord
/// lets the user select (a notice, a channel's start, a message's files).
final class MessageTextView: NSTextView {
    private var measuredWidth: CGFloat = -1
    private var padding: (top: CGFloat, bottom: CGFloat) = (0, 0)
    private var linkElements: [LinkElement]?
    var onLink: (URL) -> Void = { _ in }
    /// A click on a custom or Unicode emoji, with its box in window coordinates.
    var onExpression: (ExpressionTarget, NSRect) -> Void = { _, _ in }
    /// A message's own text, whose emoji show the hand, a hover card and a popout.
    var detectsExpressions = false
    /// On a link Discord underlines while the pointer is over it.
    static let underlinesOnHover = NSAttributedString.Key("akari.underlinesOnHover")
    private var underlined: NSRange?
    // The message's own items, above the text's.
    var messageMenu: () -> [NSMenuItem] = { [] }
    let decorations = MessageDecorations()
    // Above the text: the selection's fill is opaque, and an emoji stays visible when selected.
    let emojiOverlay = EmojiOverlay()
    /// Where the custom emoji's images come from.
    var images: ImageCache? {
        get { emojiOverlay.images }
        set { emojiOverlay.images = newValue }
    }
    /// The list's say: this message's custom emoji play.
    var animatesEmoji: Bool {
        get { emojiOverlay.animates }
        set { emojiOverlay.animates = newValue }
    }

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
        // Links keep their own colors: pills are links too. The list sets the cursor.
        linkTextAttributes = [:]
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
        emojiOverlay.frame = frame
        if newSize.width != measuredWidth {
            measuredWidth = newSize.width
            linkElements = nil
            invalidateIntrinsicContentSize()
            updateDecorations()
        }
    }

    // A selection survives a reread of the same message, not an edit or another message.
    func show(_ text: NSAttributedString, keepingSelection: Bool = false) {
        guard let storage = textStorage else {
            return
        }
        let padding = MessageText.padding(text)
        if padding != self.padding {
            self.padding = padding
            // Only its top counts: the height is the view's own, from the padding.
            textContainerInset = NSSize(width: 0, height: padding.top)
        }
        let kept = keepingSelection && storage.string == text.string ? selectedRanges : nil
        underline(at: nil)
        storage.setAttributedString(text)
        linkElements = nil
        selectedRanges = kept ?? [NSValue(range: NSRange(location: 0, length: 0))]
        invalidateIntrinsicContentSize()
        updateDecorations()
    }

    // NSTextView scrolls its insertion point into view when layout resizes it, which here would
    // move the list as if the user had.
    override func scrollToVisible(_ rect: NSRect) -> Bool {
        false
    }

    // Behind the text in the superview, in step with this view's frame.
    override func viewWillMove(toSuperview newSuperview: NSView?) {
        super.viewWillMove(toSuperview: newSuperview)
        decorations.removeFromSuperview()
        emojiOverlay.removeFromSuperview()
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        superview?.addSubview(decorations, positioned: .below, relativeTo: self)
        superview?.addSubview(emojiOverlay, positioned: .above, relativeTo: self)
        decorations.frame = frame
        emojiOverlay.frame = frame
    }

    override func setFrameOrigin(_ newOrigin: NSPoint) {
        super.setFrameOrigin(newOrigin)
        decorations.frame = frame
        emojiOverlay.frame = frame
    }

    override var isHidden: Bool {
        didSet {
            decorations.isHidden = isHidden
            emojiOverlay.isHidden = isHidden
        }
    }

    private func updateDecorations() {
        let shapes = (layoutManager as? MessageLayoutManager)?.blocks() ?? []
        decorations.show(shapes, at: textContainerOrigin)
        emojiOverlay.show(shapes, at: textContainerOrigin)
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

    // A hidden code block reveals on a click anywhere on its cover, its padding included. A
    // press on an emoji is tracked here: released on it, its popout opens; dragged away, the
    // press and what followed go to NSTextView, which selects.
    // PROBE (temporary): the press's reads and decisions, for CI's log.
    static var pressTrace: ((String) -> Void)?

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        if let spoiler = decorations.cover(at: point),
            let url = URL(string: "akari://spoiler/\(spoiler)")
        {
            onLink(url)
            return
        }
        Self.pressTrace?("down ts \(event.timestamp) w \(event.windowNumber) at \(point) clicks \(event.clickCount) hit \(expression(at: point).map { "\($0.rect)" } ?? "nil") window \(window?.windowNumber ?? -1)")
        if event.clickCount == 1, let hit = expression(at: point), let window {
            var seen: [NSEvent] = []
            var others: [NSEvent] = []
            defer {
                for other in others.reversed() {
                    NSApp.postEvent(other, atStart: true)
                }
            }
            while let next = window.nextEvent(matching: [.leftMouseUp, .leftMouseDragged]) {
                Self.pressTrace?("read type \(next.type.rawValue) ts \(next.timestamp) w \(next.windowNumber) in window \(next.locationInWindow) here \(convert(next.locationInWindow, from: nil))")
                // Another window's events go back to it; older ones were never this press's.
                if next.windowNumber != window.windowNumber {
                    others.append(next)
                    continue
                }
                guard next.timestamp >= event.timestamp else {
                    continue
                }
                seen.append(next)
                let at = convert(next.locationInWindow, from: nil)
                if next.type == .leftMouseUp {
                    Self.pressTrace?("up: inside \(hit.rect.contains(at))")
                    if hit.rect.contains(at) {
                        setSelectedRange(NSRange(location: selectedRange().location, length: 0))
                        onExpression(hit.target, convert(hit.rect, to: nil))
                    }
                    return
                }
                if hypot(at.x - point.x, at.y - point.y) > 3 {
                    break
                }
            }
            Self.pressTrace?("loop ended without a release: seen \(seen.count)")
            for followed in seen.reversed() {
                NSApp.postEvent(followed, atStart: true)
            }
            super.mouseDown(with: event)
            return
        }
        Self.pressTrace?("not an emoji press: super")
        // NSTextView would start dragging the selection; a browser follows the link.
        if event.clickCount == 1, let index = character(at: point),
            textStorage?.attribute(.link, at: index, effectiveRange: nil) != nil,
            selectedRanges.contains(where: { NSLocationInRange(index, $0.rangeValue) })
        {
            setSelectedRange(NSRange(location: index, length: 0))
        }
        super.mouseDown(with: event)
    }

    // The list sets the cursor for its rows (MessageTableController): NSTextView's own area
    // would set the I-beam anywhere in the view and race its neighbours'. Its tooltips' stays.
    override func addTrackingArea(_ area: NSTrackingArea) {
        if area.owner === self, area.options.contains(.cursorUpdate) {
            return
        }
        super.addTrackingArea(area)
    }

    override func cursorUpdate(with event: NSEvent) {}

    // A first responder gets mouse moves in a window that wants them; NSTextView's set the I-beam.
    override func mouseMoved(with event: NSEvent) {}

    override func resetCursorRects() {}

    /// Underlines the hover link under `point`, in this view's coordinates; `nil` takes it away.
    func underline(at point: NSPoint?) {
        let range = point.flatMap(hoverLink(at:))
        guard range != underlined, let layoutManager else {
            return
        }
        if let underlined {
            layoutManager.removeTemporaryAttribute(.underlineStyle, forCharacterRange: underlined)
        }
        if let range {
            layoutManager.addTemporaryAttribute(
                .underlineStyle, value: NSUnderlineStyle.single.rawValue, forCharacterRange: range)
        }
        underlined = range
    }

    private func hoverLink(at point: NSPoint) -> NSRange? {
        guard let index = character(at: point), let storage = textStorage else {
            return nil
        }
        var range = NSRange()
        guard storage.attribute(Self.underlinesOnHover, at: index, effectiveRange: &range) != nil
        else {
            return nil
        }
        return range
    }

    // The character whose glyph is under `point`, in this view's coordinates.
    private func character(at point: NSPoint) -> Int? {
        guard let layoutManager, let textContainer, let storage = textStorage, storage.length > 0
        else {
            return nil
        }
        let inside = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        let glyph = layoutManager.glyphIndex(for: inside, in: textContainer)
        let glyphRect = layoutManager.boundingRect(
            forGlyphRange: NSRange(location: glyph, length: 1), in: textContainer)
        guard glyphRect.contains(inside) else {
            return nil
        }
        return layoutManager.characterIndexForGlyph(at: glyph)
    }

    /// What the pointer shows at `point`, in this view's coordinates, as over Discord's message
    /// text: the pointing hand over a link or a hidden block, the I-beam over text, the arrow
    /// around it.
    func cursor(at point: NSPoint) -> NSCursor {
        if expression(at: point) != nil {
            return .pointingHand
        }
        return cursorAreas.last { $0.rect.contains(point) }?.cursor ?? .arrow
    }

    /// A custom or Unicode emoji under `point` (this view's coordinates), with its box: one in
    /// a link, inline code, a code block or a hidden spoiler is that, not an emoji.
    func expression(at point: NSPoint) -> (target: ExpressionTarget, rect: NSRect)? {
        guard detectsExpressions, let layoutManager, let textContainer, let storage = textStorage,
            storage.length > 0
        else {
            return nil
        }
        if let (box, rect) = emojiOverlay.box(at: point) {
            guard let index = character(at: point) ?? emojiIndex(near: point),
                storage.attribute(.link, at: index, effectiveRange: nil) == nil
            else {
                return nil
            }
            return (.customEmoji(id: box.id, name: box.name, animated: box.animated), rect)
        }
        guard let index = character(at: point) else {
            return nil
        }
        let attributes = storage.attributes(at: index, effectiveRange: nil)
        let code = (attributes[.akariBlock] as? MessageText.Block)?.contains(.code) == true
            || attributes[.akariPill] as? MessageText.Pill == .code
        guard attributes[.link] == nil, !code, attributes[.akariEmoji] == nil else {
            return nil
        }
        let range = (storage.string as NSString).rangeOfComposedCharacterSequence(at: index)
        let glyph = (storage.string as NSString).substring(with: range)
        // A lone ASCII character is never an emoji (a keycap is a digit and two more); the name
        // lookup crosses into Rust.
        let scalars = glyph.unicodeScalars
        guard scalars.count > 1 || scalars.first.map({ $0.value >= 0xA9 }) == true,
            let name = emojiShortcode(emoji: glyph)
        else {
            return nil
        }
        let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        let rect = layoutManager.boundingRect(forGlyphRange: glyphs, in: textContainer)
            .offsetBy(dx: textContainerOrigin.x, dy: textContainerOrigin.y)
        return (.unicodeEmoji(glyph, name: name), rect)
    }

    // The attachment character a custom emoji's box stands on, for a point in the box above the
    // attachment's own (shorter) glyph.
    private func emojiIndex(near point: NSPoint) -> Int? {
        guard let layoutManager, let textContainer else {
            return nil
        }
        let inside = NSPoint(x: point.x - textContainerOrigin.x, y: point.y - textContainerOrigin.y)
        return layoutManager.characterIndexForGlyph(
            at: layoutManager.glyphIndex(for: inside, in: textContainer))
    }

    // Each line's text runs, links apart, and the hidden blocks' covers, in this view's
    // coordinates; later ones lie over earlier ones.
    private var cursorAreas: [(rect: NSRect, cursor: NSCursor)] {
        var areas: [(rect: NSRect, cursor: NSCursor)] = []
        if let layoutManager, let textContainer, let storage = textStorage, storage.length > 0 {
            let origin = textContainerOrigin
            let glyphs = layoutManager.glyphRange(for: textContainer)
            layoutManager.enumerateLineFragments(forGlyphRange: glyphs) { _, used, _, line, _ in
                let characters = layoutManager.characterRange(
                    forGlyphRange: line, actualGlyphRange: nil)
                storage.enumerateAttribute(.link, in: characters) { link, range, _ in
                    let run = layoutManager.glyphRange(
                        forCharacterRange: range, actualCharacterRange: nil)
                    let glyphs = layoutManager.boundingRect(forGlyphRange: run, in: textContainer)
                    let left = max(glyphs.minX, used.minX)
                    let right = min(glyphs.maxX, used.maxX)
                    guard right > left else {
                        return
                    }
                    let rect = NSRect(
                        x: left, y: used.minY, width: right - left, height: used.height)
                    areas.append(
                        (rect.offsetBy(dx: origin.x, dy: origin.y),
                            link == nil ? .iBeam : .pointingHand))
                }
            }
        }
        return areas + decorations.covers.map { ($0, .pointingHand) }
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

    // Where the last line's glyphs sit, measured from the bottom, for the start's baselines.
    override var lastBaselineOffsetFromBottom: CGFloat {
        guard let layoutManager, let storage = textStorage, storage.length > 0 else {
            return 0
        }
        layoutManager.ensureLayout(forCharacterRange: NSRange(location: 0, length: storage.length))
        let last = layoutManager.glyphIndexForCharacter(at: storage.length - 1)
        let line = layoutManager.lineFragmentRect(forGlyphAt: last, effectiveRange: nil)
        let baseline = padding.top + line.minY + layoutManager.location(forGlyphAt: last).y
        return intrinsicContentSize.height - baseline
    }

    // A non-editable text view would swallow Escape; the list handles it.
    override func cancelOperation(_ sender: Any?) {
        nextResponder?.tryToPerform(#selector(NSResponder.cancelOperation(_:)), with: sender)
    }

    // Shift+Page Up would extend the selection; the list jumps to the first unread message.
    override func pageUpAndModifySelection(_ sender: Any?) {
        nextResponder?.tryToPerform(
            #selector(NSResponder.pageUpAndModifySelection(_:)), with: sender)
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

/// The custom emoji's image layers on their boxes (D139), over a message's text: one
/// `ImageLayerView` per box, `:name:` as its fallback. Takes no clicks.
final class EmojiOverlay: NSView {
    private var shapes: [MessageBlockShape] = []
    private var origin = NSPoint.zero
    private(set) var emojiViews: [ImageLayerView] = []
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                emojiViews.forEach { $0.images = images }
            }
        }
    }
    var animates = false {
        didSet {
            if animates != oldValue {
                emojiViews.forEach { $0.animates = animates }
            }
        }
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    /// The emoji whose box holds `point` (the text view's coordinates).
    func box(at point: NSPoint) -> (MessageText.EmojiBox, NSRect)? {
        for shape in shapes {
            if case .emoji(let rect, let box) = shape {
                let placed = rect.offsetBy(dx: origin.x, dy: origin.y)
                if placed.contains(point) {
                    return (box, placed)
                }
            }
        }
        return nil
    }

    func show(_ all: [MessageBlockShape], at origin: NSPoint) {
        let shapes = all.filter { if case .emoji = $0 { true } else { false } }
        guard shapes != self.shapes || origin != self.origin else {
            return
        }
        self.shapes = shapes
        self.origin = origin
        for (index, shape) in shapes.enumerated() {
            guard case .emoji(let rect, let box) = shape else {
                continue
            }
            let view: ImageLayerView
            if index < emojiViews.count {
                view = emojiViews[index]
            } else {
                view = ImageLayerView()
                view.fit = .contain
                view.images = images
                view.animates = animates
                emojiViews.append(view)
                addSubview(view)
            }
            view.frame = rect.offsetBy(dx: origin.x, dy: origin.y)
            view.fallbackText = ":\(box.name):"
            view.show(ImageLayerView.Content(key: box.key, canAnimate: box.animated))
        }
        for view in emojiViews.dropFirst(shapes.count) {
            view.show(nil)
            view.removeFromSuperview()
        }
        emojiViews.removeLast(emojiViews.count - shapes.count)
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

    func show(_ all: [MessageBlockShape], at origin: NSPoint) {
        let shapes = all.filter { if case .emoji = $0 { false } else { true } }
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
            case .emoji:
                layer.backgroundColor = nil
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
