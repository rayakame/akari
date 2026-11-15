import AkariKit
import AppKit

// One per list, moved between cells, so its text, selection and undo outlive row reloads.
final class MessageEditorView: NSView {
    /// 8 below the header with the stack's 2, as the official text area's margin.
    static let top: CGFloat = 6
    static let radius: CGFloat = 8
    static let lineGap: CGFloat = 7
    // The text's room beside a count of three digits, as the official editor's.
    static let counterRoom: CGFloat = 32
    static let counterTrailing: CGFloat = 14
    /// Around the edited row as it scrolls into view, as the official list's 16 + 48.
    static let revealPadding: CGFloat = 64
    // Kept between the caret's line and the list's edge.
    static let caretMargin: CGFloat = 8
    static let cancelWords = ("escape to ", "cancel")
    static let saveWords = (" • enter to ", "save")

    let textView = ComposerTextView.plain()
    private let box = EditorBox()
    let counter = CellText.label()
    let problem = CellText.label()
    let hint = EditorHint()
    private let undo = UndoManager()
    var onText: (String) -> Void = { _ in }
    // After the counter came or went, which changes the text's width.
    var onResize: () -> Void = {}
    var onFocus: (Bool) -> Void = { _ in }
    var onSave: () -> Void = {} {
        didSet { hint.save.onClick = onSave }
    }
    var onCancel: () -> Void = {} {
        didSet { hint.cancel.onClick = onCancel }
    }
    var onCaret: () -> Void = {}
    var remaining: () -> Int? = { nil }
    // Measures as the text view lays out: TextKit 1, the same attributes and padding.
    private let measureStorage = NSTextStorage()
    private let measureLayout = NSLayoutManager()
    private let measureContainer = NSTextContainer()

    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        measureStorage.addLayoutManager(measureLayout)
        measureContainer.lineFragmentPadding = 0
        measureLayout.addTextContainer(measureContainer)
        textView.scrollsItself = false
        textView.editsAMessage = true
        textView.delegate = self
        textView.onFocus = { [weak self] focused in
            self?.box.focused = focused
            self?.onFocus(focused)
        }
        textView.onEscape = { [weak self] in self?.onCancel() }
        textView.onSubmit = { [weak self] in self?.onSave() }
        textView.setAccessibilityLabel("Edit Message")
        counter.font = .monospacedDigitSystemFont(
            ofSize: TypeScale.editorCounter.size, weight: TypeScale.editorCounter.weight.appKit)
        counter.alignment = .right
        counter.isHidden = true
        problem.font = TypeScale.small.nsFont
        problem.textColor = Palette.danger
        problem.lineBreakMode = .byWordWrapping
        problem.maximumNumberOfLines = 0
        problem.isHidden = true
        for view in [box, textView, counter, problem, hint] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = true
            addSubview(view)
        }
    }

    required init?(coder: NSCoder) {
        nil
    }

    func load(_ text: String) {
        textView.string = text
        textView.setSelectedRange(NSRange(location: (text as NSString).length, length: 0))
        undo.removeAllActions()
        refreshCounter()
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    func show(problem text: String?) {
        problem.stringValue = text ?? ""
        problem.isHidden = text == nil
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    var problemText: String? { problem.isHidden ? nil : problem.stringValue }

    func height(width: CGFloat) -> CGFloat {
        var height = Self.top + boxHeight(width: width)
        if !problem.isHidden {
            height += Self.lineGap + problemHeight(width: width)
        }
        return height + Self.lineGap + EditorHint.height + Self.lineGap
    }

    /// The text column's width, for the height asked before the editor is laid out.
    var columnWidth: CGFloat = 0

    override var intrinsicContentSize: NSSize {
        NSSize(
            width: NSView.noIntrinsicMetric,
            height: height(width: bounds.width > 0 ? bounds.width : columnWidth))
    }

    // The height depends on the width, which Auto Layout gives it only now.
    override func setFrameSize(_ newSize: NSSize) {
        let changed = newSize.width != frame.width
        super.setFrameSize(newSize)
        if changed {
            invalidateIntrinsicContentSize()
        }
    }

    // The table takes a row's view out of the window as it scrolls and moves the editor between
    // cells: typing goes on where nothing else took the focus meanwhile.
    private var refocus = false
    private var focusWhenShown = false

    func takeFocus() {
        refocus = false
        if let window {
            focusWhenShown = false
            window.makeFirstResponder(textView)
        } else {
            focusWhenShown = true
        }
    }

    override func viewWillMove(toWindow newWindow: NSWindow?) {
        super.viewWillMove(toWindow: newWindow)
        if newWindow == nil, window?.firstResponder === textView {
            refocus = true
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else {
            return
        }
        if focusWhenShown || refocus && (window.firstResponder === window || window.firstResponder == nil) {
            window.makeFirstResponder(textView)
        }
        focusWhenShown = false
        refocus = false
    }

    func forgetFocus() {
        refocus = false
        focusWhenShown = false
    }

    // Its empty parts are the row's.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let hit = super.hitTest(point)
        return hit === self ? nil : hit
    }

    private func boxHeight(width: CGFloat) -> CGFloat {
        let inset = ComposerTextView.insets
        let textWidth = max(1, width - 2 * inset.width - textRoom)
        let text = textView.string
        measureStorage.setAttributedString(
            NSAttributedString(string: text, attributes: ComposerTextView.attributes))
        measureContainer.size = NSSize(width: textWidth, height: .greatestFiniteMagnitude)
        measureLayout.ensureLayout(for: measureContainer)
        let used = max(measureLayout.usedRect(for: measureContainer).height, ComposerTextView.lineHeight)
        return ceil(used) + 2 * inset.height
    }

    // The label's cell, with its insets: its intrinsic width cut off the last digit's edge.
    private var counterSize: NSSize {
        let size = counter.cell?.cellSize ?? counter.intrinsicContentSize
        return NSSize(width: ceil(size.width), height: ceil(size.height))
    }

    // A wider count, a negative one past the limit, takes more.
    private var textRoom: CGFloat {
        guard !counter.isHidden else {
            return 0
        }
        return max(Self.counterRoom, counterSize.width + Self.counterTrailing + 2 - ComposerTextView.insets.width)
    }

    private func problemHeight(width: CGFloat) -> CGFloat {
        ceil(problem.cell?.cellSize(forBounds: NSRect(x: 0, y: 0, width: width, height: .greatestFiniteMagnitude)).height ?? 0)
    }

    override func layout() {
        super.layout()
        let width = bounds.width
        let boxFrame = NSRect(x: 0, y: Self.top, width: width, height: boxHeight(width: width))
        box.frame = boxFrame
        textView.frame = NSRect(
            x: boxFrame.minX, y: boxFrame.minY,
            width: boxFrame.width - textRoom, height: boxFrame.height)
        let counterSize = self.counterSize
        // Its middle on the last line's, as the composer's counter.
        let lastLine = boxFrame.maxY - ComposerTextView.insets.height - ComposerTextView.lineHeight / 2
        // A label's text ends 2 short of its frame: the digits end 14 in.
        counter.frame = NSRect(
            x: boxFrame.maxX - Self.counterTrailing + 2 - counterSize.width,
            y: (lastLine - counterSize.height / 2).rounded(), width: counterSize.width,
            height: counterSize.height)
        var y = boxFrame.maxY
        if !problem.isHidden {
            let height = problemHeight(width: width)
            // A label's text starts 2 into its frame.
            problem.frame = NSRect(x: -2, y: y + Self.lineGap, width: width + 4, height: height)
            y += Self.lineGap + height
        }
        hint.frame = NSRect(x: 0, y: y + Self.lineGap, width: width, height: EditorHint.height)
    }

    var caretRect: NSRect {
        guard let layout = textView.layoutManager, let container = textView.textContainer else {
            return textView.frame
        }
        let index = min(textView.selectedRange().location, (textView.string as NSString).length)
        let glyphs = layout.glyphRange(forCharacterRange: NSRange(location: index, length: 0), actualCharacterRange: nil)
        var rect: NSRect
        if index == (textView.string as NSString).length, layout.extraLineFragmentTextContainer != nil {
            rect = layout.extraLineFragmentRect
        } else {
            rect = layout.boundingRect(forGlyphRange: NSRange(location: glyphs.location, length: 0), in: container)
        }
        rect.size.height = max(rect.height, ComposerTextView.lineHeight)
        rect.origin.x += textView.textContainerOrigin.x
        rect.origin.y += textView.textContainerOrigin.y
        return textView.convert(rect, to: self)
    }

    private func refreshCounter() {
        let left = remaining()
        let shows = left != nil
        if counter.isHidden == shows {
            counter.isHidden = !shows
            needsLayout = true
        }
        guard let left else {
            return
        }
        counter.stringValue = left.formatted()
        counter.textColor = left < 0 ? Palette.danger : Palette.textDefault
        counter.toolTip = Self.counterTip(left)
    }

    static func counterTip(_ left: Int) -> String {
        if left < 0 {
            return "Message is too long"
        }
        return left == 0 ? "Message is at max length" : "\(left.formatted()) characters remaining"
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        refreshCounter()
    }
}

extension MessageEditorView: NSTextViewDelegate {
    func textDidChange(_ notification: Notification) {
        onText(textView.string)
        refreshCounter()
        invalidateIntrinsicContentSize()
        needsLayout = true
        onResize()
    }

    func textViewDidChangeSelection(_ notification: Notification) {
        onCaret()
    }

    func undoManager(for view: NSTextView) -> UndoManager? {
        undo
    }
}

final class EditorBox: NSView {
    var focused = false {
        didSet { needsDisplay = true }
    }

    override var wantsUpdateLayer: Bool { true }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = MessageEditorView.radius
        layer?.borderWidth = 1
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func updateLayer() {
        layer?.backgroundColor = Palette.composer.cgColor
        layer?.borderColor = (focused ? Palette.borderSubtle : Palette.borderMuted).cgColor
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

// One line under transparent buttons: buttons that drew their words padded the line.
final class EditorHint: NSView {
    static let height: CGFloat = 16

    let cancel = EditorLink(MessageEditorView.cancelWords.1)
    let save = EditorLink(MessageEditorView.saveWords.1)

    override var isFlipped: Bool { true }

    init() {
        super.init(frame: .zero)
        for link in [cancel, save] {
            link.translatesAutoresizingMaskIntoConstraints = true
            addSubview(link)
        }
    }

    required init?(coder: NSCoder) {
        nil
    }

    var text: String {
        pieces.map(\.text).joined()
    }

    private var pieces: [(text: String, link: EditorLink?)] {
        [
            (MessageEditorView.cancelWords.0, nil), (cancel.word, cancel),
            (MessageEditorView.saveWords.0, nil), (save.word, save),
        ]
    }

    private func drawn(_ text: String, link: EditorLink?) -> NSAttributedString {
        var attributes: [NSAttributedString.Key: Any] = [
            .font: TypeScale.small.nsFont,
            .foregroundColor: link == nil ? Palette.textDefault : Palette.textLink,
        ]
        if link?.isHovered == true {
            attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
        }
        return NSAttributedString(string: text, attributes: attributes)
    }

    override func layout() {
        super.layout()
        var x: CGFloat = 0
        for (text, link) in pieces {
            let size = drawn(text, link: link).size()
            link?.frame = NSRect(x: x, y: 0, width: ceil(size.width), height: Self.height)
            x += size.width
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        var x: CGFloat = 0
        for (text, link) in pieces {
            let string = drawn(text, link: link)
            let size = string.size()
            string.draw(at: NSPoint(x: x, y: ((Self.height - size.height) / 2).rounded()))
            x += size.width
        }
    }
}

final class EditorLink: NSButton {
    let word: String
    var onClick: () -> Void = {}
    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                superview?.needsDisplay = true
            }
        }
    }

    init(_ word: String) {
        self.word = word
        super.init(frame: .zero)
        title = word
        isBordered = false
        isTransparent = true
        refusesFirstResponder = true
        target = self
        action = #selector(clicked)
    }

    required init?(coder: NSCoder) {
        nil
    }

    var underlined: Bool { isHovered }

    @objc private func clicked() {
        onClick()
    }
}
