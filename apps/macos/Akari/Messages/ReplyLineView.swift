import AkariKit
import AppKit

/// The line over a reply: the answered message's author and its text on one line. The text is a
/// button that jumps to the message; the list's controller says when the pointer is over it.
final class ReplyLineView: NSView {
    static let height: CGFloat = 18
    /// Between the line and the header under it.
    static let gapBelow: CGFloat = 4
    static let avatarSize: CGFloat = 16
    static let gap: CGFloat = 4
    static let font = NSFont.systemFont(ofSize: TypeScale.size(14))
    static let nameFont = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .medium)
    static let placeholderFont: NSFont = {
        let italic = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        return italic
    }()

    let avatar = ImageLayerView()
    let content = ReplyContentButton()
    private let badge = NSImageView()
    private(set) var line: ReplyLine?
    private var name: NSAttributedString?

    var images: ImageCache? {
        get { avatar.images }
        set {
            avatar.images = newValue
            content.line.images = newValue
        }
    }
    /// The list's say for the reply: the line's emoji play with it.
    var animatesEmoji: Bool {
        get { content.line.animatesEmoji }
        set { content.line.animatesEmoji = newValue }
    }
    /// The list sets it while the pointer is over the text or the spine.
    var hovered: Bool {
        get { content.hovered }
        set { content.hovered = newValue }
    }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        avatar.placeholder = .disc
        avatar.cornerRadius = Self.avatarSize / 2
        avatar.isHidden = true
        badge.image = NSImage(
            systemSymbolName: "arrowshape.turn.up.left.fill", accessibilityDescription: nil)
        badge.symbolConfiguration = .init(pointSize: 8, weight: .semibold)
        badge.contentTintColor = Palette.textDefault
        badge.imageScaling = .scaleNone
        badge.isHidden = true
        for view in [avatar, badge, content] as [NSView] {
            addSubview(view)
        }
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: Self.height)
    }

    /// `text` is the line's content as it draws (`ReplyLineView.text`); `jumps` off where the
    /// line is only shown, as in the New Thread form.
    func configure(_ line: ReplyLine, text: NSAttributedString, jumps: Bool = true) {
        self.line = line
        avatar.isHidden = line.author == nil
        avatar.show(line.author.map { ImageLayerView.Content(key: $0.avatar) })
        badge.isHidden = line.author != nil
        name = line.name.map {
            NSAttributedString(
                string: $0, attributes: [.font: Self.nameFont, .foregroundColor: Palette.replyName])
        }
        content.configure(line, text: text, jumps: jumps && line.jumps)
        needsLayout = true
        needsDisplay = true
    }

    /// The cell shows a message that answers nothing: the line's pictures go.
    func clear() {
        guard line != nil else {
            return
        }
        line = nil
        name = nil
        avatar.show(nil)
        content.clear()
    }

    /// The name's place, from the view's top left; `nil` without one.
    var nameFrame: NSRect? {
        guard let name else {
            return nil
        }
        let start = Self.avatarSize + Self.gap
        let width = min(ceil(name.size().width), max(0, bounds.width - start))
        return NSRect(x: start, y: 0, width: width, height: Self.height)
    }

    var shownName: String? { name?.string }

    var nameText: NSAttributedString? { name }

    var showsBadge: Bool { !badge.isHidden }

    override func layout() {
        super.layout()
        let box = NSRect(
            x: 0, y: (Self.height - Self.avatarSize) / 2, width: Self.avatarSize,
            height: Self.avatarSize)
        avatar.frame = box
        badge.frame = box
        let start = nameFrame.map { $0.maxX + Self.gap } ?? Self.avatarSize + Self.gap
        let room = max(0, bounds.width - start)
        content.frame = NSRect(
            x: start, y: 0, width: min(room, content.fittingWidth), height: Self.height)
    }

    override func draw(_ dirtyRect: NSRect) {
        if !badge.isHidden {
            Palette.baseLowest.setFill()
            NSBezierPath(ovalIn: badge.frame).fill()
        }
        if let name, let frame = nameFrame {
            let baseline = PreviewLineView.baseline(in: Self.height, font: Self.nameFont)
            name.draw(
                with: NSRect(x: frame.minX, y: baseline, width: frame.width, height: 0),
                options: [.truncatesLastVisibleLine], context: nil)
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    /// The content as the line draws it: the message's text on one line, with "(edited)", or the
    /// official client's words in italics.
    static func text(
        of line: ReplyLine, now: Date, times: TimestampText
    ) -> NSAttributedString {
        build(line, now: now, times: times).text
    }

    /// `text(of:now:times:)`, and when a relative time in it reads differently.
    static func build(
        _ line: ReplyLine, now: Date, times: TimestampText
    ) -> (text: NSAttributedString, nextChange: Date?) {
        switch line.content {
        case .placeholder(let words):
            let text = NSAttributedString(
                string: words,
                attributes: [.font: placeholderFont, .foregroundColor: Palette.textDefault])
            return (text, nil)
        case .text(let text, let edited):
            let body = MessageText.build(
                MessageText.Source(
                    text, base: .body, color: Palette.textDefault, spoilersShown: false),
                revealed: [], now: now, times: times)
            let built = NSMutableAttributedString(attributedString: body.text)
            if edited {
                built.append(CellText.editedMark())
            }
            let size = TypeScale.size(14)
            let shown = ThreadBoxView.content(built, size: size, emojiSide: (size * 1.25).rounded())
            return (shown, body.nextChange)
        }
    }
}

/// A reply line's text with its symbols: one button, lit by the list's controller.
final class ReplyContentButton: NSButton {
    static let iconSize: CGFloat = 16
    static let iconGap: CGFloat = 4
    // Italics lean past their advance.
    private static let placeholderPadding: CGFloat = 2

    let line = PreviewLineView()
    private let leading = NSImageView()
    private let trailing = NSImageView()
    private var text = NSAttributedString()
    private var isPlaceholder = false
    private(set) var jumps = false

    var hovered = false {
        didSet {
            if hovered != oldValue {
                show()
            }
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        line.font = ReplyLineView.font
        line.onReflow = { [weak self] in
            self?.superview?.needsLayout = true
        }
        addSubview(line)
        for icon in [leading, trailing] {
            icon.symbolConfiguration = .init(pointSize: 12, weight: .medium)
            icon.imageScaling = .scaleProportionallyDown
            icon.isHidden = true
            addSubview(icon)
        }
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    func configure(_ reply: ReplyLine, text: NSAttributedString, jumps: Bool) {
        self.text = text
        self.jumps = jumps
        isEnabled = jumps
        if case .placeholder = reply.content {
            isPlaceholder = true
        } else {
            isPlaceholder = false
        }
        Self.set(leading, reply.leadingSymbol)
        Self.set(trailing, reply.trailingSymbol)
        setAccessibilityLabel(reply.accessibilityLabel)
        setAccessibilityHelp(jumps ? "Jump To Reply" : nil)
        show()
    }

    func clear() {
        text = NSAttributedString()
        line.text = text
        Self.set(leading, nil)
        Self.set(trailing, nil)
    }

    private static func set(_ icon: NSImageView, _ symbol: String?) {
        icon.isHidden = symbol == nil
        icon.image = symbol.flatMap { NSImage(systemSymbolName: $0, accessibilityDescription: nil) }
    }

    // The text brightens under the pointer; mentions, links and code keep their colors.
    private func show() {
        let color = hovered && jumps ? Palette.textStrong : Palette.textDefault
        let shown = NSMutableAttributedString(attributedString: text)
        if hovered && jumps {
            text.enumerateAttribute(
                .foregroundColor, in: NSRange(location: 0, length: text.length)
            ) { value, range, _ in
                if value as? NSColor === Palette.textDefault {
                    shown.addAttribute(.foregroundColor, value: color, range: range)
                }
            }
        }
        line.text = shown
        leading.contentTintColor = color
        trailing.contentTintColor = color
        needsLayout = true
    }

    var shownText: NSAttributedString { line.shown }

    var symbols: (leading: Bool, trailing: Bool) { (!leading.isHidden, !trailing.isHidden) }

    private var leadingWidth: CGFloat {
        leading.isHidden ? 0 : Self.iconSize + Self.iconGap
    }

    private var trailingWidth: CGFloat {
        trailing.isHidden ? 0 : Self.iconSize + Self.iconGap
    }

    private var textWidth: CGFloat {
        line.textWidth + (isPlaceholder ? Self.placeholderPadding : 0)
    }

    /// The whole line's width; the owner gives it less when the row is narrower.
    var fittingWidth: CGFloat {
        leadingWidth + textWidth + trailingWidth
    }

    /// The text's place in the button.
    var textFrame: NSRect { line.frame }

    var trailingFrame: NSRect? { trailing.isHidden ? nil : trailing.frame }

    override func layout() {
        super.layout()
        let top = (bounds.height - Self.iconSize) / 2
        leading.frame = NSRect(x: 0, y: top, width: Self.iconSize, height: Self.iconSize)
        let width = max(0, min(textWidth, bounds.width - leadingWidth - trailingWidth))
        line.frame = NSRect(x: leadingWidth, y: 0, width: width, height: bounds.height)
        trailing.frame = NSRect(
            x: line.frame.maxX + Self.iconGap, y: top, width: Self.iconSize, height: Self.iconSize)
    }

    // The symbols and the line take no clicks of their own.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let superview, !isHidden else {
            return nil
        }
        return bounds.contains(convert(point, from: superview)) ? self : nil
    }

    override func draw(_ dirtyRect: NSRect) {}

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        jumps ? .button : .staticText
    }
}

/// The elbow from the avatar's middle up into a reply's line: the official client's spine, a
/// layer, and a second place to click for the jump.
final class ReplySpineView: NSButton {
    /// From the cell's left edge; the stroke's middle is 6 in, on the avatar's center.
    static let leading: CGFloat = 30
    static let size = NSSize(width: 38, height: ReplyLineView.height)
    static let stroke: CGFloat = 2
    private static let strokeX: CGFloat = 6
    private static let radius: CGFloat = 5

    let shape = CAShapeLayer()

    var hovered = false {
        didSet {
            if hovered != oldValue {
                recolor()
            }
        }
    }

    override var isEnabled: Bool {
        didSet { recolor() }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        shape.fillColor = nil
        shape.lineWidth = Self.stroke
        shape.lineCap = .butt
        shape.path = Self.path
        layer?.addSublayer(shape)
        setAccessibilityElement(false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    // Up from the line's bottom, round the corner, along the line's middle to 4 before the
    // small avatar.
    static var path: CGPath {
        let middle = ReplyLineView.height / 2
        let path = CGMutablePath()
        path.move(to: CGPoint(x: strokeX, y: size.height))
        path.addLine(to: CGPoint(x: strokeX, y: middle + radius))
        path.addArc(
            tangent1End: CGPoint(x: strokeX, y: middle),
            tangent2End: CGPoint(x: strokeX + radius, y: middle), radius: radius)
        path.addLine(to: CGPoint(x: size.width, y: middle))
        return path
    }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        shape.frame = bounds
        CATransaction.commit()
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        recolor()
    }

    override func draw(_ dirtyRect: NSRect) {}

    private func recolor() {
        let color = hovered && isEnabled ? Palette.interactiveText : Palette.spine
        effectiveAppearance.performAsCurrentDrawingAppearance {
            CATransaction.begin()
            CATransaction.setAnimationDuration(0.05)
            shape.strokeColor = color.cgColor
            CATransaction.commit()
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }
}
