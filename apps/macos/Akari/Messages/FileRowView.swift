import AkariKit
import AppKit

/// An attachment shown as a file (D141): Discord's row, 432 wide at most and 74 tall, with
/// Akari's symbol for the type, the name as a link that opens the file in the browser, and the
/// size as Discord words it; under the pointer, Download past its top right corner.
final class FileRowView: NSView {
    static let width: CGFloat = 432
    static let height: CGFloat = 74
    private static let padding: CGFloat = 16
    private static let icon = NSSize(width: 30, height: 40)
    private static let iconGap: CGFloat = 8
    // Discord's `.nonMediaMosaicItem{top:-8px;inset-inline-end:-8px}`.
    static let downloadOverhang: CGFloat = 8

    // The card's fill, border and shadow under the subviews, so Download covers its corner.
    private let card = CALayer()
    private let symbol = NSImageView()
    let name = FileNameButton()
    private let size = CellText.label()
    let download = HoverActionButton(
        symbol: "arrow.down.to.line", label: HoverActionButton.download, ringed: true)
    private(set) var attachment: Attachment?
    var onOpen: () -> Void = {}
    var onDownload: () -> Void = {}
    /// Set by the list, which knows what's under the pointer: the card or its Download.
    var isHovered = false {
        didSet { download.isShown = isHovered }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        card.actions = ["bounds": NSNull(), "position": NSNull(), "shadowPath": NSNull()]
        card.cornerRadius = 8
        card.borderWidth = 1
        card.shadowOpacity = 1
        card.shadowRadius = 2
        card.shadowOffset = NSSize(width: 0, height: -1)
        layer?.addSublayer(card)
        symbol.imageScaling = .scaleProportionallyDown
        symbol.contentTintColor = Palette.textMuted
        name.target = self
        name.action = #selector(openClicked)
        size.font = TypeScale.small.nsFont
        size.textColor = Palette.textMuted
        size.lineBreakMode = .byTruncatingTail
        download.target = self
        download.action = #selector(downloadClicked)
        download.isHidden = true
        for view in [symbol, name, size, download] as [NSView] {
            addSubview(view)
        }
        updateColors()
        // Download overhangs the corner, and the shadow lies outside too; set last, as adding the
        // subviews turned it back on.
        clipsToBounds = false
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    /// `downloadable`: Download among its hover buttons.
    func configure(_ attachment: Attachment?, downloadable: Bool = false) {
        self.attachment = attachment
        download.isHidden = attachment == nil || !downloadable
        guard let attachment else {
            return
        }
        symbol.image = NSImage(
            systemSymbolName: FileType.symbol(forFileNamed: attachment.filename),
            accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .light))
        name.show(FileType.shownName(attachment))
        size.stringValue = FileSize.text(attachment.size)
        name.setAccessibilityLabel("\(attachment.filename), \(FileSize.text(attachment.size))")
        needsLayout = true
    }

    var shownSize: String { size.stringValue }
    var symbolName: String? { attachment.map { FileType.symbol(forFileNamed: $0.filename) } }
    var showsDownload: Bool { !download.isHidden && download.isShown }

    /// Where the card takes the pointer in its superview: itself, and its Download past the
    /// corner.
    func takes(_ point: NSPoint) -> NSView? {
        guard !isHidden else {
            return nil
        }
        let local = convert(point, from: superview)
        if !download.isHidden, download.frame.contains(local) {
            return download
        }
        return bounds.contains(local) ? hitTest(point) ?? self : nil
    }

    @objc private func openClicked() {
        onOpen()
    }

    @objc private func downloadClicked() {
        onDownload()
    }

    override func layout() {
        super.layout()
        symbol.frame = NSRect(
            x: Self.padding + 1, y: (bounds.height - Self.icon.height) / 2, width: Self.icon.width,
            height: Self.icon.height)
        let x = symbol.frame.maxX + Self.iconGap
        let width = max(0, bounds.width - x - Self.padding - 1)
        let nameHeight = name.intrinsicContentSize.height
        let sizeHeight = size.intrinsicContentSize.height
        let top = (bounds.height - nameHeight - sizeHeight) / 2
        name.frame = NSRect(
            x: x, y: top, width: min(width, name.intrinsicContentSize.width), height: nameHeight)
        size.frame = NSRect(x: x, y: top + nameHeight, width: width, height: sizeHeight)
        let box = HoverActionButton.size
        download.frame = download.frame(
            box: NSRect(
                x: bounds.maxX + Self.downloadOverhang - box, y: -Self.downloadOverhang, width: box,
                height: box))
        card.frame = bounds
        card.shadowPath = CGPath(
            roundedRect: bounds, cornerWidth: 8, cornerHeight: 8, transform: nil)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            card.backgroundColor = Palette.surfaceHigh.cgColor
            card.borderColor = Palette.borderSubtle.cgColor
            // Discord's shadow-low.
            card.shadowColor = NSColor.black.withAlphaComponent(0.14).cgColor
        }
    }
}

/// A file's name as a link: Discord's link color, underlined while hovered.
final class FileNameButton: NSButton {
    private var shown = ""
    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                update()
            }
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        refusesFirstResponder = true
        lineBreakMode = .byTruncatingTail
        (cell as? NSButtonCell)?.lineBreakMode = .byTruncatingTail
        update()
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ name: String) {
        shown = name
        update()
    }

    var underlined: Bool { isHovered }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        update()
    }

    private func update() {
        let paragraph = NSMutableParagraphStyle()
        paragraph.lineBreakMode = .byTruncatingTail
        var attributes: [NSAttributedString.Key: Any] = [
            .font: TypeScale.body.nsFont, .foregroundColor: Palette.textLink,
            .paragraphStyle: paragraph,
        ]
        if isHovered {
            attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
        }
        attributedTitle = NSAttributedString(string: shown, attributes: attributes)
    }
}
