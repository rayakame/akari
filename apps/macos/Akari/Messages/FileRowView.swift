import AkariKit
import AppKit

/// An attachment shown as a file (D141): Discord's row, 432 wide at most and 74 tall, with
/// Akari's symbol for the type, the name as a link that opens the file in the browser, and the
/// size as Discord words it.
final class FileRowView: NSView {
    static let width: CGFloat = 432
    static let height: CGFloat = 74
    private static let padding: CGFloat = 16
    private static let icon = NSSize(width: 30, height: 40)
    private static let iconGap: CGFloat = 8

    private let symbol = NSImageView()
    let name = FileNameButton()
    private let size = CellText.label()
    private(set) var attachment: Attachment?
    var onOpen: () -> Void = {}

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        layer?.shadowOpacity = 1
        layer?.shadowRadius = 2
        layer?.shadowOffset = NSSize(width: 0, height: -1)
        symbol.imageScaling = .scaleProportionallyDown
        symbol.contentTintColor = Palette.textMuted
        name.target = self
        name.action = #selector(openClicked)
        size.font = TypeScale.small.nsFont
        size.textColor = Palette.textMuted
        size.lineBreakMode = .byTruncatingTail
        for view in [symbol, name, size] as [NSView] {
            addSubview(view)
        }
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    func configure(_ attachment: Attachment?) {
        self.attachment = attachment
        guard let attachment else {
            return
        }
        symbol.image = NSImage(
            systemSymbolName: FileType.symbol(forFileNamed: attachment.filename),
            accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .light))
        name.show(Self.shownName(attachment))
        size.stringValue = FileSize.text(attachment.size)
        name.setAccessibilityLabel("\(attachment.filename), \(FileSize.text(attachment.size))")
        needsLayout = true
    }

    /// The name Discord shows: the title with the file's extension when it gave one.
    static func shownName(_ attachment: Attachment) -> String {
        guard let title = attachment.title, !title.isEmpty else {
            return attachment.filename
        }
        let parts = attachment.filename.split(separator: ".", omittingEmptySubsequences: false)
        guard parts.count > 1, let last = parts.last else {
            return title
        }
        return "\(title).\(last)"
    }

    var shownSize: String { size.stringValue }
    var symbolName: String? { attachment.map { FileType.symbol(forFileNamed: $0.filename) } }

    @objc private func openClicked() {
        onOpen()
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
        layer?.shadowPath = CGPath(
            roundedRect: bounds, cornerWidth: 8, cornerHeight: 8, transform: nil)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Palette.surfaceHigh.cgColor
            layer?.borderColor = Palette.borderSubtle.cgColor
            // Discord's shadow-low.
            layer?.shadowColor = NSColor.black.withAlphaComponent(0.14).cgColor
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
