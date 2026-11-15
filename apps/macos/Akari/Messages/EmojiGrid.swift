import AkariKit
import AppKit

/// Where the picker's grid puts its sections: a header over each titled section, then rows of
/// `EmojiPickerModel.columns` cells; a search's results start without one.
struct EmojiGridLayout: Equatable {
    static let cell: CGFloat = 40
    static let header: CGFloat = 32
    /// Left of the first column (and right of the last, where the scroller is), above results
    /// and under the last row.
    static let inset: CGFloat = 12
    /// The emoji's box inside its cell.
    static let emoji: CGFloat = 32

    struct Section: Equatable {
        let top: CGFloat
        let itemsTop: CGFloat
        let count: Int
        let titled: Bool

        var bottom: CGFloat {
            itemsTop + CGFloat((count + EmojiPickerModel.columns - 1) / EmojiPickerModel.columns) * cell
        }
    }

    let sections: [Section]
    let height: CGFloat

    init(_ sections: [EmojiPickerModel.Section]) {
        var y: CGFloat = 0
        var laid: [Section] = []
        for section in sections {
            let titled = section.kind != .results
            let entry = Section(
                top: y, itemsTop: y + (titled ? Self.header : Self.inset), count: section.items.count,
                titled: titled)
            laid.append(entry)
            y = entry.bottom
        }
        self.sections = laid
        height = y + Self.inset
    }

    static var width: CGFloat { inset + CGFloat(EmojiPickerModel.columns) * cell }

    func rect(of position: EmojiPickerModel.Position) -> NSRect {
        let section = sections[position.section]
        let columns = EmojiPickerModel.columns
        return NSRect(
            x: Self.inset + CGFloat(position.item % columns) * Self.cell,
            y: section.itemsTop + CGFloat(position.item / columns) * Self.cell, width: Self.cell,
            height: Self.cell)
    }

    /// The header's band over a titled section.
    func headerRect(of section: Int) -> NSRect {
        NSRect(x: 0, y: sections[section].top, width: Self.width, height: Self.header)
    }

    func position(at point: NSPoint) -> EmojiPickerModel.Position? {
        guard point.x >= Self.inset, point.x < Self.width,
            let index = sections.lastIndex(where: { $0.itemsTop <= point.y }),
            point.y < sections[index].bottom
        else {
            return nil
        }
        let section = sections[index]
        let column = Int((point.x - Self.inset) / Self.cell)
        let row = Int((point.y - section.itemsTop) / Self.cell)
        let item = row * EmojiPickerModel.columns + column
        return item < section.count ? EmojiPickerModel.Position(section: index, item: item) : nil
    }

    /// The section that reaches the line at `y`: the last that starts at or above it.
    func section(at y: CGFloat) -> Int? {
        sections.lastIndex { $0.top <= y + 0.5 } ?? (sections.isEmpty ? nil : 0)
    }

    /// Every cell that `rect` touches.
    func positions(in rect: NSRect) -> [EmojiPickerModel.Position] {
        let columns = EmojiPickerModel.columns
        var found: [EmojiPickerModel.Position] = []
        for (index, section) in sections.enumerated()
        where section.count > 0 && section.bottom > rect.minY && section.itemsTop < rect.maxY {
            let first = max(0, Int((rect.minY - section.itemsTop) / Self.cell))
            let last = Int(((min(rect.maxY, section.bottom) - section.itemsTop) / Self.cell).rounded(.up)) - 1
            guard first <= last else {
                continue
            }
            for row in first...last {
                for column in 0..<columns where row * columns + column < section.count {
                    found.append(EmojiPickerModel.Position(section: index, item: row * columns + column))
                }
            }
        }
        return found
    }
}

/// The picker's grid: views only for the cells that show (custom emoji from the image tier,
/// Unicode ones in the system's font), the headers of the sections that show, and the selection.
/// The pointer selects and a click picks; it never takes the keyboard's focus, which stays in the
/// search field.
final class EmojiGridView: NSView {
    private(set) var layout = EmojiGridLayout([])
    private(set) var sections: [EmojiPickerModel.Section] = []
    private(set) var selection: EmojiPickerModel.Position?
    private(set) var cells: [EmojiPickerModel.Position: PickerEmojiCell] = [:]
    private(set) var headers: [Int: NSTextField] = [:]
    let selectionView = NSView()
    private var spareCells: [PickerEmojiCell] = []
    private var spareHeaders: [NSTextField] = []
    private var area: NSTrackingArea?
    private var pressed: EmojiPickerModel.Position?
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                for cell in cells.values + spareCells {
                    cell.picture.images = images
                }
            }
        }
    }
    var motion = MotionPolicy.default
    /// The picker's window is key and the app active: custom emoji may play.
    var isKey: () -> Bool = { false }
    /// The pointer over a cell, or `nil` between them.
    var onPoint: (EmojiPickerModel.Position?) -> Void = { _ in }
    /// A click on a cell, with Shift as it was.
    var onClick: (EmojiPickerModel.Position, Bool) -> Void = { _, _ in }
    /// The list scrolled or changed: the section at the top and how far its sticky header is
    /// pushed up by the next one (0 or less); `nil` for a search's results.
    var onTop: ((section: Int, offset: CGFloat)?) -> Void = { _ in }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        selectionView.wantsLayer = true
        selectionView.layer?.cornerRadius = 6
        selectionView.isHidden = true
        addSubview(selectionView)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Emoji")
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }
    override var acceptsFirstResponder: Bool { false }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    func show(_ sections: [EmojiPickerModel.Section], selection: EmojiPickerModel.Position?) {
        self.sections = sections
        layout = EmojiGridLayout(sections)
        pressed = nil
        for (_, cell) in cells {
            cell.isHidden = true
            cell.picture.animates = false
            spareCells.append(cell)
        }
        cells = [:]
        for (_, header) in headers {
            header.isHidden = true
            spareHeaders.append(header)
        }
        headers = [:]
        setFrameSize(NSSize(width: max(frame.width, EmojiGridLayout.width), height: layout.height))
        select(selection)
        tile()
    }

    func select(_ position: EmojiPickerModel.Position?) {
        let before = selection
        selection = position.flatMap { contains($0) ? $0 : nil }
        if let selection {
            selectionView.frame = layout.rect(of: selection)
            selectionView.isHidden = false
        } else {
            selectionView.isHidden = true
        }
        for changed in [before, selection].compactMap({ $0 }) {
            cells[changed].map(updateAnimation)
        }
    }

    private func contains(_ position: EmojiPickerModel.Position) -> Bool {
        sections.indices.contains(position.section)
            && sections[position.section].items.indices.contains(position.item)
    }

    /// What a sticky header covers at the top of what shows.
    var covered: CGFloat {
        guard let top = top() else {
            return 0
        }
        return EmojiGridLayout.header + top.offset
    }

    private func top() -> (section: Int, offset: CGFloat)? {
        let visible = visibleRect
        guard let index = layout.section(at: visible.minY), layout.sections[index].titled else {
            return nil
        }
        let next = layout.sections.indices.contains(index + 1) ? layout.sections[index + 1].top : .infinity
        return (index, min(0, next - visible.minY - EmojiGridLayout.header))
    }

    /// Brings `position` into view: just enough for `.item`, its section's header to the top for
    /// `.section`.
    func reveal(_ position: EmojiPickerModel.Position, _ reveal: EmojiPickerModel.Reveal) {
        guard let clip = enclosingScrollView?.contentView, contains(position) else {
            return
        }
        let shown = clip.bounds
        var y = shown.minY
        switch reveal {
        case .none:
            return
        case .section:
            y = layout.sections[position.section].top
        case .item:
            let rect = layout.rect(of: position)
            let sticky = layout.sections[position.section].titled ? EmojiGridLayout.header : 0
            if rect.minY < shown.minY + sticky {
                y = rect.minY - sticky
            } else if rect.maxY > shown.maxY {
                y = rect.maxY - shown.height
            }
        }
        scroll(to: y)
    }

    func scroll(to y: CGFloat) {
        guard let scrollView = enclosingScrollView else {
            return
        }
        let clip = scrollView.contentView
        let most = max(0, frame.height - clip.bounds.height)
        let target = min(max(0, y), most).rounded()
        guard target != clip.bounds.minY else {
            return
        }
        clip.scroll(to: NSPoint(x: 0, y: target))
        scrollView.reflectScrolledClipView(clip)
        tile()
    }

    func tile() {
        let visible = visibleRect
        let wanted = Set(layout.positions(in: visible.insetBy(dx: 0, dy: -EmojiGridLayout.cell)))
        for (position, cell) in cells where !wanted.contains(position) {
            cell.isHidden = true
            cell.picture.animates = false
            spareCells.append(cell)
            cells[position] = nil
        }
        for position in wanted where cells[position] == nil {
            let cell = spareCells.popLast() ?? makeCell()
            cell.show(sections[position.section].items[position.item])
            cell.frame = layout.rect(of: position)
            cell.isHidden = false
            cell.position = position
            cell.onPress = { [weak self] in self?.onClick(position, false) }
            cells[position] = cell
            updateAnimation(of: cell)
        }
        let shownHeaders = Set(
            layout.sections.indices.filter { index in
                layout.sections[index].titled && layout.headerRect(of: index).intersects(visible)
            })
        for (index, header) in headers where !shownHeaders.contains(index) {
            header.isHidden = true
            spareHeaders.append(header)
            headers[index] = nil
        }
        for index in shownHeaders where headers[index] == nil {
            let header = spareHeaders.popLast() ?? makeHeader()
            header.stringValue = sections[index].title
            header.frame = Self.titleFrame(in: layout.headerRect(of: index))
            header.isHidden = false
            headers[index] = header
        }
        onTop(top())
    }

    /// A header's title in its band: where the first emoji's picture starts, over the row.
    static func titleFrame(in band: NSRect) -> NSRect {
        let x = EmojiGridLayout.inset + (EmojiGridLayout.cell - EmojiGridLayout.emoji) / 2
        return NSRect(x: band.minX + x, y: band.minY + 12, width: band.width - x - 8, height: 16)
    }

    static func makeTitle() -> NSTextField {
        let label = NSTextField(labelWithString: "")
        label.font = .systemFont(ofSize: TypeScale.size(12), weight: .semibold)
        label.textColor = Palette.textMuted
        label.lineBreakMode = .byTruncatingTail
        label.setAccessibilityRole(.staticText)
        return label
    }

    private func makeHeader() -> NSTextField {
        let label = Self.makeTitle()
        addSubview(label)
        return label
    }

    private func makeCell() -> PickerEmojiCell {
        let cell = PickerEmojiCell()
        cell.picture.images = images
        addSubview(cell)
        return cell
    }

    func updateAnimation() {
        cells.values.forEach(updateAnimation)
    }

    private func updateAnimation(of cell: PickerEmojiCell) {
        guard let position = cell.position, cells[position] === cell else {
            cell.picture.animates = false
            return
        }
        cell.picture.animates =
            sections[position.section].items[position.item].animated
            && motion.plays(
                .emoji,
                MotionPolicy.Context(windowKey: isKey(), appActive: isKey(), hovered: position == selection))
    }

    // MARK: The pointer

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area {
            removeTrackingArea(area)
        }
        let area = NSTrackingArea(
            rect: .zero, options: [.mouseMoved, .cursorUpdate, .activeInKeyWindow, .inVisibleRect],
            owner: self)
        addTrackingArea(area)
        self.area = area
    }

    /// The cell under `point`, outside what a sticky header covers.
    func position(at point: NSPoint) -> EmojiPickerModel.Position? {
        guard point.y >= visibleRect.minY + covered else {
            return nil
        }
        return layout.position(at: point)
    }

    func cursor(at point: NSPoint) -> NSCursor {
        guard let position = position(at: point) else {
            return .arrow
        }
        return sections[position.section].items[position.item].isLocked ? .operationNotAllowed : .pointingHand
    }

    /// `point` in the grid's coordinates.
    func pointerMoved(to point: NSPoint) {
        cursor(at: point).set()
        onPoint(position(at: point))
    }

    func press(at point: NSPoint) {
        pressed = position(at: point)
    }

    // As a button's click: dragging off the emoji pressed cancels the pick.
    func release(at point: NSPoint, shift: Bool) {
        let pressed = pressed
        self.pressed = nil
        if let pressed, position(at: point) == pressed {
            onClick(pressed, shift)
        }
    }

    func click(at point: NSPoint, shift: Bool) {
        press(at: point)
        release(at: point, shift: shift)
    }

    override func mouseMoved(with event: NSEvent) {
        pointerMoved(to: convert(event.locationInWindow, from: nil))
    }

    override func cursorUpdate(with event: NSEvent) {
        cursor(at: convert(event.locationInWindow, from: nil)).set()
    }

    override func mouseDown(with event: NSEvent) {
        press(at: convert(event.locationInWindow, from: nil))
    }

    override func mouseUp(with event: NSEvent) {
        release(at: convert(event.locationInWindow, from: nil), shift: event.modifierFlags.contains(.shift))
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            selectionView.layer?.backgroundColor = Palette.hoverBackground.cgColor
        }
    }

    override func accessibilityChildren() -> [Any]? {
        cells.sorted { ($0.key.section, $0.key.item) < ($1.key.section, $1.key.item) }.map(\.value)
    }
}

/// One emoji of the grid: the picture in its 32-point box, dimmed when it can't be used here.
final class PickerEmojiCell: NSView {
    let glyph = EmojiGlyphView()
    let picture = ImageLayerView()
    private(set) var item: EmojiPickerModel.Item?
    var position: EmojiPickerModel.Position?
    /// VoiceOver's press.
    var onPress: () -> Void = {}

    init() {
        super.init(frame: NSRect(x: 0, y: 0, width: EmojiGridLayout.cell, height: EmojiGridLayout.cell))
        picture.fit = .contain
        picture.placeholder = .none
        let inset = (EmojiGridLayout.cell - EmojiGridLayout.emoji) / 2
        let box = NSRect(x: inset, y: inset, width: EmojiGridLayout.emoji, height: EmojiGridLayout.emoji)
        glyph.frame = box
        picture.frame = box
        addSubview(glyph)
        addSubview(picture)
        setAccessibilityElement(true)
        setAccessibilityRole(.button)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    func show(_ item: EmojiPickerModel.Item) {
        guard item != self.item else {
            return
        }
        self.item = item
        if let key = item.key {
            glyph.isHidden = true
            glyph.emoji = ""
            picture.isHidden = false
            picture.fallbackText = item.name
            picture.show(ImageLayerView.Content(key: key))
        } else {
            picture.show(nil)
            picture.isHidden = true
            glyph.isHidden = false
            glyph.emoji = item.glyph ?? ""
        }
        alphaValue = item.isLocked ? 0.35 : 1
        setAccessibilityLabel(item.spokenName)
        setAccessibilityHelp(item.reason)
        setAccessibilityEnabled(!item.isLocked)
    }

    override func accessibilityPerformPress() -> Bool {
        onPress()
        return true
    }
}

/// The picker's left column: a button per section (recently used, each server by its icon, a
/// line, Unicode's groups by symbols); the section at the top of the grid is lit.
final class EmojiStripView: NSView {
    static let width: CGFloat = 48
    static let button: CGFloat = 32
    static let spacing: CGFloat = 4

    let scrollView = NSScrollView()
    private let content = FlippedView()
    private(set) var buttons: [EmojiStripButton] = []
    let line = NSView()
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                buttons.forEach { $0.picture.images = images }
            }
        }
    }
    /// A button's click: its section.
    var onJump: (EmojiPickerModel.Section.Kind) -> Void = { _ in }
    private(set) var lit: Int?

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = false
        scrollView.hasHorizontalScroller = false
        scrollView.verticalScrollElasticity = .allowed
        scrollView.documentView = content
        addSubview(scrollView)
        line.wantsLayer = true
        line.isHidden = true
        content.addSubview(line)
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel("Sections")
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    /// The browsing sections' buttons; a search's results have none.
    func show(_ sections: [EmojiPickerModel.Section]) {
        let shown = sections.filter { $0.kind != .results }
        guard
            shown.map(\.id) != buttons.map(\.kind) || shown.map(\.icon) != buttons.map(\.icon)
                || shown.map(\.title) != buttons.map(\.name)
        else {
            return
        }
        buttons.forEach { $0.removeFromSuperview() }
        line.isHidden = true
        var y = Self.spacing * 2
        let x = (Self.width - Self.button) / 2
        let firstGroup = shown.firstIndex { if case .unicode = $0.kind { true } else { false } }
        buttons = shown.enumerated().map { index, section in
            if index == firstGroup, index > 0 {
                line.frame = NSRect(x: 12, y: y + Self.spacing / 2, width: Self.width - 24, height: 1)
                line.isHidden = false
                y += 2 * Self.spacing + 1
            }
            let button = EmojiStripButton(section)
            button.picture.images = images
            button.frame = NSRect(x: x, y: y, width: Self.button, height: Self.button)
            button.target = self
            button.action = #selector(clicked(_:))
            content.addSubview(button)
            y += Self.button + Self.spacing
            return button
        }
        content.frame = NSRect(x: 0, y: 0, width: Self.width, height: y + Self.spacing)
        lit = nil
        setAccessibilityChildren(buttons)
    }

    /// `index` among the browsing sections.
    func light(_ index: Int?) {
        guard index != lit else {
            return
        }
        lit = index
        for (offset, button) in buttons.enumerated() {
            button.isLit = offset == index
        }
        if let index, buttons.indices.contains(index) {
            content.scrollToVisible(buttons[index].frame.insetBy(dx: 0, dy: -Self.spacing))
        }
    }

    override func layout() {
        super.layout()
        scrollView.frame = bounds
        content.frame.size.width = bounds.width
    }

    @objc private func clicked(_ sender: EmojiStripButton) {
        onJump(sender.kind)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Palette.baseLowest.cgColor
            line.layer?.backgroundColor = Palette.borderSubtle.cgColor
        }
    }

}

/// One section's button in the strip: an SF Symbol, or a server's icon over its initials.
final class EmojiStripButton: NSButton {
    static let symbolSize: CGFloat = 16
    static let icon: CGFloat = 24

    let kind: EmojiPickerModel.Section.Kind
    let icon: ImageKey?
    let name: String
    let picture = ImageLayerView()
    let initials = NSTextField(labelWithString: "")
    private let tile = NSView()

    var isLit = false {
        didSet {
            if isLit != oldValue {
                updateColors()
            }
        }
    }
    private var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateColors()
            }
        }
    }
    private var area: NSTrackingArea?

    init(_ section: EmojiPickerModel.Section) {
        kind = section.kind
        icon = section.icon
        name = section.title
        super.init(frame: .zero)
        isBordered = false
        title = ""
        refusesFirstResponder = true
        (cell as? NSButtonCell)?.highlightsBy = []
        imagePosition = .imageOnly
        imageScaling = .scaleNone
        wantsLayer = true
        layer?.cornerRadius = 8
        toolTip = section.title
        setAccessibilityLabel(section.title)
        if let symbol = section.symbol {
            image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
                .withSymbolConfiguration(.init(pointSize: Self.symbolSize, weight: .medium))
        } else {
            tile.wantsLayer = true
            tile.layer?.cornerRadius = 6
            initials.stringValue = Initials.of(section.title)
            initials.font = .systemFont(ofSize: 9, weight: .medium)
            initials.alignment = .center
            initials.lineBreakMode = .byClipping
            initials.textColor = Palette.textDefault
            picture.cornerRadius = 6
            picture.placeholder = .none
            picture.onStateChange = { [weak self] picture in
                self?.tile.isHidden = !picture.failed && picture.content != nil
            }
            for view in [tile, initials, picture] as [NSView] {
                addSubview(view)
            }
            tile.isHidden = section.icon != nil
            picture.show(section.icon.map { ImageLayerView.Content(key: $0, canAnimate: false) })
        }
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func layout() {
        super.layout()
        let box = NSRect(
            x: (bounds.width - Self.icon) / 2, y: (bounds.height - Self.icon) / 2, width: Self.icon,
            height: Self.icon)
        tile.frame = box
        picture.frame = box
        let height = initials.fittingSize.height
        initials.frame = NSRect(x: box.minX, y: box.midY - height / 2, width: box.width, height: height)
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area {
            removeTrackingArea(area)
        }
        let area = NSTrackingArea(
            rect: .zero, options: [.mouseEnteredAndExited, .cursorUpdate, .activeInKeyWindow, .inVisibleRect],
            owner: self)
        addTrackingArea(area)
        self.area = area
    }

    override func mouseEntered(with event: NSEvent) { isHovered = true }
    override func mouseExited(with event: NSEvent) { isHovered = false }
    override func cursorUpdate(with event: NSEvent) { NSCursor.pointingHand.set() }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        let fill: NSColor? = isLit ? Palette.selectedBackground : isHovered ? Palette.hoverBackground : nil
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = fill?.cgColor
            tile.layer?.backgroundColor = Palette.hoverBackground.cgColor
        }
        contentTintColor = isLit || isHovered ? Palette.interactiveTextActive : Palette.interactiveText
    }
}
