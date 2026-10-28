import AkariKit
import AppKit

/// The viewer's fixed colours: Discord forces its Onyx theme on it (module 947056), in both
/// themes.
enum ViewerColors {
    static let surface = NSColor(hex: "#17181b")
    static let border = NSColor(hex: "#97979f").withAlphaComponent(0.16)
    static let icon = NSColor(hex: "#96979e")
    static let iconHover = NSColor(hex: "#dcdcdf")
    static let buttonHover = NSColor(hex: "#97979f").withAlphaComponent(0.2)
    static let buttonPressed = NSColor(hex: "#97979f").withAlphaComponent(0.24)
    static let roundHover = NSColor(hex: "#97979f").withAlphaComponent(0.08)
    static let name = NSColor(hex: "#dcdcdf")
    static let time = NSColor(hex: "#81828a")
    static let scrim = NSColor.black.withAlphaComponent(0.92)
}

/// A button of the viewer's chrome: an SF Symbol, its tooltip and VoiceOver name the same words,
/// the hover's colours. The action bar's are 32 × 32 with 16 pt symbols; Close and the arrows are
/// 40 × 40 on the surface with 20 pt ones.
final class ViewerButton: NSButton {
    enum Style {
        case bar
        case round
    }

    private let style: Style
    private var area: NSTrackingArea?
    private(set) var isHovered = false {
        didSet { updateColors() }
    }
    var onPress: () -> Void = {}

    init(symbol: String, label: String, style: Style) {
        self.style = style
        super.init(frame: .zero)
        isBordered = false
        title = ""
        imagePosition = .imageOnly
        refusesFirstResponder = true
        wantsLayer = true
        layer?.cornerRadius = style == .bar ? 8 : 12
        layer?.borderWidth = 1
        target = self
        action = #selector(pressed)
        set(symbol: symbol, label: label)
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    func set(symbol: String, label: String) {
        image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: style == .bar ? 14 : 17, weight: .regular))
        toolTip = label
        setAccessibilityLabel(label)
    }

    var label: String { accessibilityLabel() ?? "" }

    // The first click of a window that isn't key acts.
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    @objc private func pressed() {
        onPress()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let area {
            removeTrackingArea(area)
        }
        let area = NSTrackingArea(
            rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect], owner: self)
        addTrackingArea(area)
        self.area = area
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
    }

    override func viewDidHide() {
        super.viewDidHide()
        isHovered = false
    }

    private func updateColors() {
        contentTintColor = isHovered ? ViewerColors.iconHover : ViewerColors.icon
        switch style {
        case .bar:
            layer?.backgroundColor = isHovered ? ViewerColors.buttonHover.cgColor : nil
            layer?.borderColor = isHovered ? ViewerColors.border.cgColor : NSColor.clear.cgColor
        case .round:
            let fill = isHovered ? ViewerColors.surface.blended(withFraction: 0.08, of: NSColor(hex: "#97979f")) : nil
            layer?.backgroundColor = (fill ?? ViewerColors.surface).cgColor
            layer?.borderColor = ViewerColors.border.cgColor
        }
    }
}

/// The top bar's left: the author's avatar, name and time (F-9). Clicks pass through it, as
/// Discord's `pointer-events: none` does.
final class ViewerAuthor: NSView {
    let avatar = ImageLayerView()
    let name = NSTextField(labelWithString: "")
    let time = NSTextField(labelWithString: "")

    init() {
        super.init(frame: .zero)
        avatar.placeholder = .disc
        avatar.cornerRadius = ViewerLayout.row / 2
        name.font = .systemFont(ofSize: TypeScale.size(16), weight: .medium)
        name.textColor = ViewerColors.name
        name.lineBreakMode = .byTruncatingTail
        time.font = .systemFont(ofSize: TypeScale.size(12))
        time.textColor = ViewerColors.time
        time.lineBreakMode = .byTruncatingTail
        for view in [avatar, name, time] as [NSView] {
            addSubview(view)
        }
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    // The time keeps its tooltip; nothing here takes a click.
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    /// `author` is the block beside the avatar, both in this view's space.
    func place(avatar frame: NSRect, author: NSRect) {
        avatar.frame = frame
        let nameHeight = ceil(name.intrinsicContentSize.height)
        let timeHeight = ceil(time.intrinsicContentSize.height)
        var y = author.minY + ((author.height - nameHeight - timeHeight) / 2).rounded()
        name.frame = NSRect(x: author.minX, y: y, width: author.width, height: nameHeight)
        y += nameHeight
        time.frame = NSRect(x: author.minX, y: y, width: author.width, height: timeHeight)
    }
}

/// The action bar (module 947056): its buttons on Discord's surface with a 1 pt border.
final class ViewerActionBar: NSView {
    private(set) var buttons: [ViewerButton] = []

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 12
        layer?.borderWidth = ViewerLayout.barBorder
        layer?.backgroundColor = ViewerColors.surface.cgColor
        layer?.borderColor = ViewerColors.border.cgColor
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    func set(_ buttons: [ViewerButton]) {
        for button in self.buttons where !buttons.contains(button) {
            button.removeFromSuperview()
        }
        for button in buttons where button.superview !== self {
            addSubview(button)
        }
        self.buttons = buttons
        needsLayout = true
    }

    override func layout() {
        super.layout()
        let inset = ViewerLayout.barPadding + ViewerLayout.barBorder
        for (index, button) in buttons.enumerated() {
            button.frame = NSRect(
                x: inset + CGFloat(index) * (ViewerLayout.button + ViewerLayout.buttonGap), y: inset,
                width: ViewerLayout.button, height: ViewerLayout.button)
        }
    }

    // The bar's own padding is the bar's: a click there doesn't close the viewer.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, frame.contains(point) else {
            return nil
        }
        return super.hitTest(point) ?? self
    }

    override func mouseDown(with event: NSEvent) {}
}

/// The strip under the media (module 947056): a 40 pt square per item, corners 2, the strip's
/// ends 8, all but the current dimmed by black at 50 %; scrolls when it's wider than its room.
final class ViewerStrip: NSView {
    final class Item: NSButton {
        let thumbnail = ImageLayerView()
        // A subview, so AppKit keeps it over the thumbnail's layer.
        private let dim = PassiveView()
        var dimmed = true {
            didSet { dim.isHidden = !dimmed }
        }

        init() {
            super.init(frame: .zero)
            isBordered = false
            title = ""
            refusesFirstResponder = true
            wantsLayer = true
            layer?.cornerRadius = 2
            layer?.masksToBounds = true
            layer?.backgroundColor = NSColor.black.cgColor
            thumbnail.tier = .media
            addSubview(thumbnail)
            dim.wantsLayer = true
            dim.layer?.backgroundColor = NSColor.black.withAlphaComponent(0.5).cgColor
            addSubview(dim)
        }

        required init?(coder: NSCoder) {
            nil
        }

        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func layout() {
            super.layout()
            thumbnail.frame = bounds
            dim.frame = bounds
        }
    }

    final class PassiveView: NSView {
        override var isFlipped: Bool { true }

        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }
    }

    final class Row: NSView {
        override var isFlipped: Bool { true }
    }

    private let scroll = NSScrollView()
    private let row = Row()
    /// Holds the squares and rounds the strip's two ends.
    let ends = Row()
    private(set) var items: [Item] = []
    var onSelect: (Int) -> Void = { _ in }

    init() {
        super.init(frame: .zero)
        scroll.drawsBackground = false
        scroll.hasHorizontalScroller = false
        scroll.hasVerticalScroller = false
        scroll.horizontalScrollElasticity = .allowed
        scroll.verticalScrollElasticity = .none
        scroll.documentView = row
        ends.wantsLayer = true
        ends.layer?.cornerRadius = ViewerLayout.stripEnd
        ends.layer?.masksToBounds = true
        row.addSubview(ends)
        addSubview(scroll)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    func set(_ media: [ViewerModel.Item], images: ImageCache?) {
        for item in items {
            item.removeFromSuperview()
        }
        items = media.enumerated().map { index, media in
            let item = Item()
            item.thumbnail.images = images
            item.thumbnail.show(ImageLayerView.Content(key: media.key, canAnimate: false))
            item.target = self
            item.action = #selector(clicked(_:))
            item.tag = index
            ends.addSubview(item)
            return item
        }
        needsLayout = true
    }

    func select(_ index: Int) {
        for (offset, item) in items.enumerated() {
            item.dimmed = offset != index
            let label = "Item \(offset + 1) of \(items.count)"
            item.setAccessibilityLabel(offset == index ? "\(label) (current)" : label)
        }
        if items.indices.contains(index) {
            layoutSubtreeIfNeeded()
            row.scrollToVisible(items[index].convert(items[index].bounds, to: row).insetBy(dx: -20, dy: 0))
        }
    }

    @objc private func clicked(_ sender: Item) {
        onSelect(sender.tag)
    }

    override func layout() {
        super.layout()
        scroll.frame = bounds
        let size = ViewerLayout.thumbnail
        var x: CGFloat = 0
        for item in items {
            item.frame = NSRect(x: x, y: 0, width: size, height: size)
            x += size + ViewerLayout.thumbnailGap
        }
        let width = max(0, x - ViewerLayout.thumbnailGap)
        ends.frame = NSRect(x: ViewerLayout.stripEnd, y: 0, width: width, height: size)
        row.frame = NSRect(x: 0, y: 0, width: max(width + 2 * ViewerLayout.stripEnd, bounds.width), height: size)
    }
}
