import AkariKit
import AppKit

/// The emoji picker for reacting to a message, as a panel beside what opened it in the session
/// window's `ChildWindows` (key, over a clear shield: a press outside or Escape closes it).
@MainActor
final class EmojiPickerPresenter {
    /// Which side of its opener the picker stands on; its top is the opener's.
    enum Side {
        case left, right
    }

    static let gap: CGFloat = 8
    static let margin: CGFloat = 8

    let windows = ChildWindows(keyable: true)
    private(set) var model: EmojiPickerModel?
    private(set) var view: EmojiPickerView?
    /// The picker closed, however.
    var onClose: () -> Void = {}

    init() {
        windows.label = EmojiPickerModel.label
        windows.onClose = { [weak self] in self?.close() }
    }

    /// Shows `model` beside `anchor` (`window`'s coordinates), in place of a picker already open.
    func show(
        _ model: EmojiPickerModel, beside anchor: NSRect, on side: Side, in window: NSWindow,
        images: ImageCache?, motion: MotionPolicy
    ) {
        if let shown = self.model, shown !== model {
            shown.onClose = {}
            shown.close()
        }
        self.model = model
        model.onClose = { [weak self, weak model] in
            guard let self, let model, self.model === model else {
                return
            }
            self.finish()
        }
        let view = EmojiPickerView(model: model)
        view.images = images
        view.motion = motion
        view.isKey = { [weak view] in view?.window?.isKeyWindow == true && NSApp.isActive }
        self.view = view
        let frame = Self.frame(
            beside: window.convertToScreen(anchor), on: side, size: EmojiPickerView.size, in: window.frame)
        windows.show(view, at: frame, in: window, dimmed: false)
        // Laid out at once: the grid reveals its selection and the pointer finds its cells.
        view.layoutSubtreeIfNeeded()
        windows.panel?.makeFirstResponder(view.field.text)
    }

    /// Escape, a press outside, the list going.
    func close() {
        guard let model else {
            return
        }
        if model.isOpen {
            model.close()
        } else {
            finish()
        }
    }

    private func finish() {
        model = nil
        view = nil
        windows.tearDown(refocus: true)
        onClose()
    }

    /// The picker's frame beside `anchor` (both on the screen), its top at the anchor's, inside
    /// `bounds` by the margin: on the other side where `side` has no room.
    static func frame(beside anchor: NSRect, on side: Side, size: NSSize, in bounds: NSRect) -> NSRect {
        let left = anchor.minX - gap - size.width
        let right = anchor.maxX + gap
        let leftFits = left >= bounds.minX + margin
        let rightFits = right + size.width <= bounds.maxX - margin
        var x: CGFloat
        switch side {
        case .left: x = leftFits || !rightFits ? left : right
        case .right: x = rightFits || !leftFits ? right : left
        }
        x = min(max(x, bounds.minX + margin), bounds.maxX - margin - size.width)
        let y = min(max(anchor.maxY - size.height, bounds.minY + margin), bounds.maxY - margin - size.height)
        return NSRect(x: x.rounded(), y: y.rounded(), width: size.width, height: size.height)
    }
}

/// The picker: the search field over everything, the sections' strip on the left, the grid with
/// its sticky header, and the footer naming the selected emoji. It follows its model's changes;
/// the arrow keys and Enter reach it through the field, which keeps the keyboard's focus.
final class EmojiPickerView: NSView {
    static let size = NSSize(
        width: EmojiStripView.width + EmojiGridLayout.width + EmojiGridLayout.inset,
        height: header + 340 + footer)
    static let header: CGFloat = 52
    static let footer: CGFloat = 48
    /// Akari's words where a search found nothing.
    static let nothingFound = "No emoji match your search"

    let model: EmojiPickerModel
    let field = PickerSearchField()
    let strip = EmojiStripView()
    let scrollView = NSScrollView()
    let grid = EmojiGridView()
    let sticky = FlippedView()
    let stickyTitle = EmojiGridView.makeTitle()
    let footerView = EmojiFooterView()
    let empty = NSTextField(labelWithString: EmojiPickerView.nothingFound)
    private let headerLine = NSView()
    private let footerLine = NSView()
    private let stickyBand = FlippedView()
    var images: ImageCache? {
        didSet {
            grid.images = images
            strip.images = images
            footerView.images = images
        }
    }
    var motion = MotionPolicy.default {
        didSet {
            grid.motion = motion
            footerView.motion = motion
            updateAnimation()
        }
    }
    /// The picker's window is key and the app active: custom emoji may play.
    var isKey: () -> Bool = { false } {
        didSet {
            grid.isKey = isKey
            footerView.isKey = isKey
        }
    }
    /// Shift at a key press, for Enter.
    var modifiers: () -> NSEvent.ModifierFlags = { NSApp.currentEvent?.modifierFlags ?? [] }
    nonisolated(unsafe) private var observers: [NSObjectProtocol] = []

    init(model: EmojiPickerModel) {
        self.model = model
        super.init(frame: NSRect(origin: .zero, size: Self.size))
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        layer?.masksToBounds = true
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(EmojiPickerModel.label)
        field.placeholder = EmojiPickerModel.placeholder
        field.onChange = { [weak self] text in self?.model.search(text) }
        field.onCommand = { [weak self] command in self?.perform(command) ?? false }
        strip.onJump = { [weak self] kind in self?.model.jump(to: kind) }
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.hasHorizontalScroller = false
        scrollView.horizontalScrollElasticity = .none
        scrollView.documentView = grid
        scrollView.contentView.postsBoundsChangedNotifications = true
        observers.append(
            NotificationCenter.default.addObserver(
                forName: NSView.boundsDidChangeNotification, object: scrollView.contentView, queue: nil
            ) { [weak self] _ in
                MainActor.assumeIsolated { self?.grid.tile() }
            })
        grid.onPoint = { [weak self] position in self?.model.select(position) }
        grid.onClick = { [weak self] position, shift in self?.model.pick(position, keepOpen: shift) }
        grid.onTop = { [weak self] top in self?.showTop(top) }
        sticky.wantsLayer = true
        sticky.layer?.masksToBounds = true
        stickyBand.wantsLayer = true
        stickyBand.addSubview(stickyTitle)
        sticky.addSubview(stickyBand)
        empty.font = .systemFont(ofSize: TypeScale.size(14))
        empty.textColor = Palette.textMuted
        empty.alignment = .center
        empty.isHidden = true
        for line in [headerLine, footerLine] {
            line.wantsLayer = true
        }
        for view in [strip, scrollView, sticky, empty, footerView, headerLine, footerLine, field] as [NSView] {
            addSubview(view)
        }
        model.onChange = { [weak self] change in self?.apply(change) }
        apply(.sections(fromTop: true))
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    deinit {
        observers.forEach(NotificationCenter.default.removeObserver)
    }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        let width = bounds.width
        let body = Self.header
        field.frame = NSRect(x: 12, y: 10, width: width - 24, height: 32)
        headerLine.frame = NSRect(x: 0, y: body - 1, width: width, height: 1)
        strip.frame = NSRect(x: 0, y: body, width: EmojiStripView.width, height: max(0, bounds.height - body))
        let gridX = EmojiStripView.width
        let gridHeight = max(0, bounds.height - body - Self.footer)
        scrollView.frame = NSRect(x: gridX, y: body, width: width - gridX, height: gridHeight)
        grid.frame.size.width = scrollView.contentSize.width
        sticky.frame = NSRect(x: gridX, y: body, width: EmojiGridLayout.width, height: EmojiGridLayout.header)
        stickyBand.frame.size = sticky.bounds.size
        stickyTitle.frame = EmojiGridView.titleFrame(in: sticky.bounds)
        footerLine.frame = NSRect(x: gridX, y: bounds.height - Self.footer, width: width - gridX, height: 1)
        footerView.frame = NSRect(
            x: gridX, y: bounds.height - Self.footer + 1, width: width - gridX, height: Self.footer - 1)
        let emptyHeight = empty.fittingSize.height
        empty.frame = NSRect(
            x: gridX + 16, y: body + (gridHeight - emptyHeight) / 2, width: width - gridX - 32,
            height: emptyHeight)
        grid.tile()
    }

    // MARK: The model's changes

    private func apply(_ change: EmojiPickerModel.Change) {
        switch change {
        case .sections(let fromTop):
            let searching = model.sections.first?.kind == .results
            if !searching {
                strip.show(model.sections)
            }
            if model.query.isEmpty, !field.text.stringValue.isEmpty {
                field.clear()
            }
            let offset = scrollView.contentView.bounds.minY
            grid.show(model.sections, selection: model.selection)
            grid.scroll(to: fromTop ? 0 : offset)
            grid.tile()
            empty.isHidden = !model.foundNothing
            footerView.show(model.selected, detail: model.selected.flatMap(model.detail(of:)))
        case .selection(let reveal):
            grid.select(model.selection)
            if let selection = model.selection {
                grid.reveal(selection, reveal)
            }
            footerView.show(model.selected, detail: model.selected.flatMap(model.detail(of:)))
        }
    }

    private func showTop(_ top: (section: Int, offset: CGFloat)?) {
        let searching = model.sections.first?.kind == .results
        sticky.isHidden = top == nil
        if let top {
            stickyTitle.stringValue = grid.sections[top.section].title
            stickyBand.frame.origin.y = top.offset
        }
        strip.light(searching ? nil : top?.section)
    }

    // The field's arrow keys and Enter: the grid's, as in the official picker.
    private func perform(_ command: Selector) -> Bool {
        switch command {
        case #selector(NSResponder.moveUp(_:)): model.move(.up)
        case #selector(NSResponder.moveDown(_:)): model.move(.down)
        case #selector(NSResponder.moveLeft(_:)): model.move(.left)
        case #selector(NSResponder.moveRight(_:)): model.move(.right)
        case #selector(NSResponder.insertNewline(_:)), #selector(NSResponder.insertNewlineIgnoringFieldEditor(_:)):
            model.pick(keepOpen: modifiers().contains(.shift))
        case #selector(NSResponder.cancelOperation(_:)): model.close()
        default: return false
        }
        return true
    }

    func updateAnimation() {
        grid.updateAnimation()
        footerView.updateAnimation()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        guard let window else {
            return
        }
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            observers.append(
                NotificationCenter.default.addObserver(forName: name, object: window, queue: nil) {
                    [weak self] _ in
                    MainActor.assumeIsolated { self?.updateAnimation() }
                })
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Palette.surfaceHigh.cgColor
            layer?.borderColor = Palette.borderSubtle.cgColor
            stickyBand.layer?.backgroundColor = Palette.surfaceHigh.cgColor
            headerLine.layer?.backgroundColor = Palette.borderMuted.cgColor
            footerLine.layer?.backgroundColor = Palette.borderMuted.cgColor
        }
    }
}

/// The picker's search field: the magnifier, the text, and the X that clears it. It keeps the
/// keyboard's focus for the whole picker.
final class PickerSearchField: NSView, NSTextFieldDelegate {
    let text = NSTextField()
    let clearButton = NSButton()
    private let glass = NSImageView()
    var onChange: (String) -> Void = { _ in }
    /// A key's command (the arrows, Enter); `true` when it was taken.
    var onCommand: (Selector) -> Bool = { _ in false }
    var placeholder = "" {
        didSet {
            text.placeholderAttributedString = NSAttributedString(
                string: placeholder,
                attributes: [.foregroundColor: Palette.textMuted, .font: Self.font])
            text.setAccessibilityPlaceholderValue(placeholder)
            text.setAccessibilityLabel(placeholder)
        }
    }

    static var font: NSFont { .systemFont(ofSize: TypeScale.size(14)) }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        glass.image = NSImage(systemSymbolName: "magnifyingglass", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 12, weight: .semibold))
        glass.contentTintColor = Palette.interactiveTextActive
        text.isBordered = false
        text.drawsBackground = false
        text.focusRingType = .none
        text.font = Self.font
        text.textColor = Palette.textDefault
        text.cell?.isScrollable = true
        text.cell?.wraps = false
        text.delegate = self
        clearButton.isBordered = false
        clearButton.title = ""
        clearButton.image = NSImage(systemSymbolName: "xmark.circle.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 12, weight: .regular))
        clearButton.contentTintColor = Palette.interactiveText
        clearButton.refusesFirstResponder = true
        clearButton.setAccessibilityLabel("Clear")
        clearButton.target = self
        clearButton.action = #selector(clearClicked)
        clearButton.isHidden = true
        for view in [glass, text, clearButton] as [NSView] {
            addSubview(view)
        }
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        glass.frame = NSRect(x: 10, y: (bounds.height - 16) / 2, width: 16, height: 16)
        let height = text.fittingSize.height
        text.frame = NSRect(
            x: 32, y: ((bounds.height - height) / 2).rounded(), width: bounds.width - 32 - 30, height: height)
        clearButton.frame = NSRect(x: bounds.width - 26, y: (bounds.height - 18) / 2, width: 18, height: 18)
    }

    /// Empties the field without a search.
    func clear() {
        text.stringValue = ""
        clearButton.isHidden = true
    }

    @objc private func clearClicked() {
        clear()
        onChange("")
        window?.makeFirstResponder(text)
    }

    func controlTextDidChange(_ notification: Notification) {
        clearButton.isHidden = text.stringValue.isEmpty
        onChange(text.stringValue)
    }

    func control(_ control: NSControl, textView: NSTextView, doCommandBy commandSelector: Selector) -> Bool {
        onCommand(commandSelector)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Palette.inputBackground.cgColor
            layer?.borderColor = Palette.borderSubtle.cgColor
        }
    }
}

/// Under the grid: the selected emoji large, its `:name:`, and why it is locked or which server
/// it is from.
final class EmojiFooterView: NSView {
    static let emoji: CGFloat = 28

    let glyph = EmojiGlyphView()
    let picture = ImageLayerView()
    let name = NSTextField(labelWithString: "")
    let detail = NSTextField(labelWithString: "")
    private(set) var item: EmojiPickerModel.Item?
    var images: ImageCache? {
        get { picture.images }
        set { picture.images = newValue }
    }
    var motion = MotionPolicy.default
    var isKey: () -> Bool = { false }

    init() {
        super.init(frame: .zero)
        picture.fit = .contain
        picture.placeholder = .none
        name.font = .systemFont(ofSize: TypeScale.size(14), weight: .semibold)
        name.textColor = Palette.textStrong
        detail.font = .systemFont(ofSize: TypeScale.size(12))
        detail.textColor = Palette.textMuted
        for label in [name, detail] {
            label.lineBreakMode = .byTruncatingTail
        }
        for view in [glyph, picture, name, detail] as [NSView] {
            addSubview(view)
        }
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    func show(_ item: EmojiPickerModel.Item?, detail text: String?) {
        self.item = item
        name.stringValue = item?.name ?? ""
        detail.stringValue = text ?? ""
        detail.isHidden = text == nil
        if let key = item?.key {
            glyph.isHidden = true
            picture.isHidden = false
            picture.fallbackText = item?.name
            picture.show(ImageLayerView.Content(key: key))
        } else {
            picture.show(nil)
            picture.isHidden = true
            glyph.isHidden = item == nil
            glyph.emoji = item?.glyph ?? ""
        }
        updateAnimation()
        needsLayout = true
    }

    func updateAnimation() {
        picture.animates =
            item?.animated == true
            && motion.plays(.emoji, MotionPolicy.Context(windowKey: isKey(), appActive: isKey(), hovered: true))
    }

    override func layout() {
        super.layout()
        let box = NSRect(x: 12, y: (bounds.height - Self.emoji) / 2, width: Self.emoji, height: Self.emoji)
        glyph.frame = box
        picture.frame = box
        let x = box.maxX + 12
        let width = bounds.width - x - 12
        let nameHeight = name.fittingSize.height
        let detailHeight = detail.isHidden ? 0 : detail.fittingSize.height
        let top = ((bounds.height - nameHeight - detailHeight) / 2).rounded()
        name.frame = NSRect(x: x, y: top, width: width, height: nameHeight)
        detail.frame = NSRect(x: x, y: top + nameHeight, width: width, height: detailHeight)
    }
}

final class FlippedView: NSView {
    override var isFlipped: Bool { true }
}
