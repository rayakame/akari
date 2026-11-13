import AkariKit
import AppKit

/// A message's reactions under it: pills side by side, wrapping onto further lines, and the add
/// button at the end. Every frame comes from the pills' counts, so the row's height is final
/// before anything loads. The list lights the pill under the pointer and decides the cursor.
final class ReactionRowView: NSView {
    static let pillHeight: CGFloat = 30
    static let gap: CGFloat = 4
    // Discord's row: 1 above the first line of pills, 2 under the last.
    static let above: CGFloat = 1
    static let below: CGFloat = 2

    private(set) var pills: [ReactionPillButton] = []
    let add = ReactionAddButton()
    /// A forum's default reaction, before the pills: the emoji without a number.
    let offer = ReactionPillButton()
    /// "+3" after the pills a post's bar has room for.
    let more = ReactionMoreButton()
    private(set) var row: ReactionRow?
    /// A click on a pill.
    var onPill: (ReactionPill) -> Void = { _ in }
    var onOffer: (ReactionEmoji) -> Void = { _ in }
    var onMore: () -> Void = {}
    /// A click on the add button, with the button.
    var onAdd: (NSView) -> Void = { _ in }
    /// A pill's tooltip, asked when the row is shown and when its names arrive.
    var tooltip: (ReactionPill) -> String? = { _ in nil }
    /// Whether a count that changed may roll: the window is key and motion isn't reduced.
    var rolls: () -> Bool = { false }
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                pills.forEach { $0.images = images }
            }
        }
    }
    /// The list's say: the message's custom emoji play.
    var animatesEmoji = false {
        didSet {
            if animatesEmoji != oldValue {
                pills.forEach { $0.animates = animatesEmoji }
            }
        }
    }
    /// Above the first pills, on top of the row's own point.
    var topInset: CGFloat = 0 {
        didSet {
            if topInset != oldValue {
                invalidateIntrinsicContentSize()
                needsLayout = true
            }
        }
    }
    /// The column's width, which the cell sets before its layout: the height follows from it.
    var preferredWidth: CGFloat = 0 {
        didSet {
            if preferredWidth != oldValue {
                invalidateIntrinsicContentSize()
            }
        }
    }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        add.target = self
        add.action = #selector(addClicked)
        add.isHidden = true
        offer.target = self
        offer.action = #selector(offerClicked)
        offer.isHidden = true
        more.target = self
        more.action = #selector(moreClicked)
        more.isHidden = true
        for view in [add, offer, more] as [NSView] {
            addSubview(view)
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(ReactionWords.window)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override var intrinsicContentSize: NSSize {
        NSSize(
            width: NSView.noIntrinsicMetric,
            height: row.map { Self.height($0, width: preferredWidth, topInset: topInset) } ?? 0)
    }

    /// Where the pills and, last, the add button go in a column `width` wide; a pill that
    /// doesn't fit beside the ones before starts a line.
    static func frames(_ widths: [CGFloat], width: CGFloat, topInset: CGFloat = 0) -> [NSRect] {
        var x: CGFloat = 0
        var y = topInset + above
        return widths.map { item in
            if x > 0, x + item > width {
                x = 0
                y += pillHeight + gap
            }
            defer { x += item + gap }
            return NSRect(x: x, y: y, width: item, height: pillHeight)
        }
    }

    static func widths(_ row: ReactionRow) -> [CGFloat] {
        row.pills.map { ReactionPillButton.width(count: $0.count) }
            + (row.adds ? [ReactionAddButton.width] : [])
    }

    /// The row's height at `width`, as `layout()` places everything.
    static func height(_ row: ReactionRow, width: CGFloat, topInset: CGFloat = 0) -> CGFloat {
        let last = frames(widths(row), width: width, topInset: topInset).last
        return (last?.maxY ?? topInset + above) + below
    }

    /// A post bar's line: the forum's default reaction while the post has none, the pills, and
    /// "+N" for those the bar has no room for.
    func configure(_ row: ReactionRow?, offer emoji: ReactionEmoji?, more hidden: Int, animated: Bool) {
        configure(row, animated: animated)
        offer.isHidden = emoji == nil
        if let emoji {
            offer.images = images
            offer.showOffer(emoji)
        }
        more.isHidden = hidden <= 0
        more.show(hidden)
        setAccessibilityChildren(shownButtons)
    }

    // Left to right, as laid out.
    private var shownButtons: [NSView] {
        (offer.isHidden ? [] : [offer]) + pills + (more.isHidden ? [] : [more])
            + (add.isHidden ? [] : [add]) as [NSView]
    }

    private var shownWidths: [CGFloat] {
        (offer.isHidden ? [] : [ReactionPillButton.width(characters: 0)])
            + pills.map { ReactionPillButton.width(characters: $0.countText.count) }
            + (more.isHidden ? [] : [more.fittingWidth]) + (add.isHidden ? [] : [ReactionAddButton.width])
    }

    /// Where the last button ends: what follows a post bar's reactions starts after it.
    var contentWidth: CGFloat {
        let widths = shownWidths
        return widths.reduce(0, +) + Self.gap * CGFloat(max(widths.count - 1, 0))
    }

    /// Takes `row` for its height alone, without a pill's view: the sizing cell's.
    func measure(_ row: ReactionRow?) {
        self.row = row
    }

    /// Shows `row`; pills that stay keep their views, and a count that changed rolls when
    /// `animated`.
    func configure(_ row: ReactionRow?, animated: Bool = false) {
        self.row = row
        let shown = row?.pills ?? []
        var held = Dictionary(pills.compactMap { pill in pill.pill.map { ($0.id, pill) } }) { first, _ in first }
        var next: [ReactionPillButton] = []
        for pill in shown {
            let button = held.removeValue(forKey: pill.id) ?? makePill()
            button.show(pill, animated: animated)
            button.isEnabled = row?.toggles ?? false
            button.toolTip = tooltip(pill)
            next.append(button)
        }
        for button in pills where !next.contains(where: { $0 === button }) {
            button.removeFromSuperview()
        }
        pills = next
        add.isHidden = row?.adds != true
        setAccessibilityChildren(pills + (add.isHidden ? [] : [add]))
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    /// Reads every pill's tooltip again: its names arrived.
    func refreshTooltips() {
        for button in pills {
            button.toolTip = button.pill.flatMap(tooltip)
        }
    }

    private func makePill() -> ReactionPillButton {
        let button = ReactionPillButton()
        button.images = images
        button.animates = animatesEmoji
        button.target = self
        button.action = #selector(pillClicked(_:))
        button.rolls = { [weak self] in self?.rolls() ?? false }
        addSubview(button)
        return button
    }

    override func layout() {
        super.layout()
        let frames = Self.frames(shownWidths, width: bounds.width, topInset: topInset)
        for (button, frame) in zip(shownButtons, frames) {
            button.frame = frame
        }
    }

    // The space between the pills is the row's: a click there is the message's.
    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        return shownButtons.first { $0.frame.contains(local) }
    }

    @objc private func offerClicked() {
        if let emoji = offer.pill?.emoji {
            onOffer(emoji)
        }
    }

    @objc private func moreClicked() {
        onMore()
    }

    @objc private func pillClicked(_ sender: ReactionPillButton) {
        if let pill = sender.pill {
            onPill(pill)
        }
    }

    @objc private func addClicked() {
        onAdd(add)
    }
}

/// One reaction: the emoji, the count, and the user's own in the brand's colors. A button; one
/// that only shows is disabled and gets the not-allowed cursor from the list.
final class ReactionPillButton: NSButton {
    static let emojiSide: CGFloat = 20
    // The border and the padding inside it.
    static let inset: CGFloat = 7
    static let countGap: CGFloat = 6
    static let digitWidth: CGFloat = 12
    static let rollDuration: TimeInterval = 0.3
    static let rollKey = "roll"

    private(set) var pill: ReactionPill?
    /// What the count shows: the number, "+3" for a forum bar's rest, nothing for a forum's
    /// default reaction.
    private(set) var countText = ""
    private let glyph = EmojiGlyphView()
    let picture = ImageLayerView()
    private let countClip = CALayer()
    let countLayer = CATextLayer()
    // The number a roll takes away.
    let ghostLayer = CATextLayer()
    var images: ImageCache? {
        get { picture.images }
        set { picture.images = newValue }
    }
    var animates: Bool {
        get { picture.animates }
        set { picture.animates = newValue }
    }
    /// The list's say: the pointer is over the pill.
    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateColors(animated: true)
            }
        }
    }
    private(set) var isPressed = false {
        didSet { updateColors(animated: true) }
    }
    /// Whether a count's change may roll: the window is key and motion isn't reduced.
    var rolls: () -> Bool = { false }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        refusesFirstResponder = true
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        picture.fit = .contain
        picture.placeholder = .none
        for view in [glyph, picture] as [NSView] {
            addSubview(view)
        }
        countClip.masksToBounds = true
        for text in [ghostLayer, countLayer] {
            text.font = Self.countFont
            text.fontSize = Self.countFont.pointSize
            text.alignmentMode = .center
            text.contentsScale = 2
            text.actions = ["contents": NSNull(), "bounds": NSNull(), "position": NSNull(), "foregroundColor": NSNull()]
            countClip.addSublayer(text)
        }
        ghostLayer.isHidden = true
        layer?.addSublayer(countClip)
        updateColors(animated: false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    static var countFont: NSFont {
        .monospacedDigitSystemFont(ofSize: TypeScale.size(14), weight: .semibold)
    }

    /// A pill's width for a count: Discord gives each digit 12 points.
    static func width(count: UInt32) -> CGFloat {
        width(characters: String(count).count)
    }

    /// The width with `characters` in the count's place; without any, the emoji alone.
    static func width(characters: Int) -> CGFloat {
        2 * inset + emojiSide
            + (characters > 0 ? countGap + digitWidth * CGFloat(characters) : 0)
    }

    func show(_ pill: ReactionPill, animated: Bool = false) {
        show(pill, count: String(pill.count), animated: animated)
        setAccessibilityLabel(pill.accessibilityLabel)
    }

    /// A forum's default reaction: the emoji without a number.
    func showOffer(_ emoji: ReactionEmoji) {
        show(ReactionPill(emoji: emoji, normal: 0, mine: false), count: "", animated: false)
        setAccessibilityLabel("\(ReactionPill.name(of: emoji)), press to react")
    }

    private func show(_ pill: ReactionPill, count: String, animated: Bool) {
        let before = self.pill
        self.pill = pill
        if before?.emoji != pill.emoji {
            if let key = pill.emoji.key {
                glyph.isHidden = true
                picture.isHidden = false
                picture.fallbackText = pill.name
                picture.show(ImageLayerView.Content(key: key))
            } else {
                picture.show(nil)
                picture.isHidden = true
                glyph.isHidden = false
                glyph.emoji = pill.emoji.name ?? ""
            }
        }
        let rolling = animated && before?.id == pill.id && countText != count && !countText.isEmpty
        let old = countText
        countText = count
        countLayer.string = count
        needsLayout = true
        layoutSubtreeIfNeeded()
        if rolling, rolls() {
            roll(from: old, increased: (before?.count ?? 0) < pill.count)
        }
        if before?.mine != pill.mine {
            updateColors(animated: animated)
        }
    }

    // Discord's roll: on a higher number the old one leaves upward and the new one comes from
    // below, the other way on a lower one; 0.3 s with a slight overshoot at both ends.
    private func roll(from old: String, increased: Bool) {
        let height = countClip.bounds.height
        let rest = countLayer.position.y
        // The view is flipped: up is toward smaller y.
        let leave = increased ? -height : height
        ghostLayer.string = old
        ghostLayer.frame = countLayer.frame
        ghostLayer.foregroundColor = countLayer.foregroundColor
        for (layer, from, to) in [(ghostLayer, rest, rest + leave), (countLayer, rest - leave, rest)] {
            let move = CABasicAnimation(keyPath: "position.y")
            move.fromValue = from
            move.toValue = to
            move.duration = Self.rollDuration
            move.timingFunction = CAMediaTimingFunction(controlPoints: 0.68, -0.55, 0.265, 1.55)
            layer.add(move, forKey: Self.rollKey)
        }
        let shown = CABasicAnimation(keyPath: "hidden")
        shown.fromValue = false
        shown.toValue = false
        shown.duration = Self.rollDuration
        ghostLayer.add(shown, forKey: "shown")
    }

    override func layout() {
        super.layout()
        let box = NSRect(
            x: Self.inset, y: (bounds.height - Self.emojiSide) / 2, width: Self.emojiSide,
            height: Self.emojiSide)
        glyph.frame = box
        picture.frame = box
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        let width = max(0, bounds.width - box.maxX - Self.countGap - Self.inset)
        countClip.frame = NSRect(x: box.maxX + Self.countGap, y: 1, width: width, height: bounds.height - 2)
        let font = Self.countFont
        let line = (font.ascender - font.descender).rounded(.up)
        // The digits' middle on the pill's: capitals and digits share their height.
        let top = countClip.bounds.midY - font.capHeight / 2 - (font.ascender - font.capHeight)
        let frame = NSRect(x: 0, y: top, width: width, height: line)
        countLayer.frame = frame
        ghostLayer.frame = frame
        CATransaction.commit()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 2
        countLayer.contentsScale = scale
        ghostLayer.contentsScale = scale
    }

    override var isEnabled: Bool {
        didSet {
            if isEnabled != oldValue {
                updateColors(animated: false)
            }
        }
    }

    override func mouseDown(with event: NSEvent) {
        guard isEnabled else {
            return
        }
        isPressed = true
        super.mouseDown(with: event)
        isPressed = false
    }

    // The emoji and its image take no clicks of their own.
    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors(animated: false)
    }

    /// The fill, the border and the count's color as shown, for tests.
    var colors: (fill: NSColor, border: NSColor, count: NSColor) {
        // The user's own pill doesn't change under the pointer, as in the official client.
        if pill?.mine == true {
            return (Palette.reactionMine, Palette.brand, Palette.reactionMineText)
        }
        if isPressed {
            return (Palette.selectedBackground, Palette.reactionBorderActive, Palette.interactiveTextActive)
        }
        if isHovered, isEnabled {
            return (Palette.interactiveBackgroundActive, Palette.borderMuted, Palette.interactiveTextActive)
        }
        return (Palette.hoverBackground, .clear, Palette.interactiveText)
    }

    private func updateColors(animated: Bool) {
        guard let layer else {
            return
        }
        let shown = colors
        var fill = CGColor.clear
        var border = CGColor.clear
        effectiveAppearance.performAsCurrentDrawingAppearance {
            fill = shown.fill.cgColor
            border = shown.border.cgColor
            let text = shown.count.cgColor
            countLayer.foregroundColor = text
            ghostLayer.foregroundColor = text
        }
        if animated {
            for (key, from, to) in [
                ("backgroundColor", layer.backgroundColor as Any, fill as Any),
                ("borderColor", layer.borderColor as Any, border as Any),
            ] {
                let change = CABasicAnimation(keyPath: key)
                change.fromValue = from
                change.toValue = to
                change.duration = 0.1
                change.timingFunction = CAMediaTimingFunction(controlPoints: 0.25, 0.1, 0.25, 1)
                layer.add(change, forKey: key)
            }
        }
        layer.backgroundColor = fill
        layer.borderColor = border
    }
}

/// "+3" at the end of a post bar's reactions: the ones it has no room for; a click shows them
/// all.
final class ReactionMoreButton: NSButton {
    private(set) var count = 0

    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                show(count)
            }
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        refusesFirstResponder = true
        (cell as? NSButtonCell)?.highlightsBy = []
        wantsLayer = true
        layer?.cornerRadius = 8
        toolTip = "View Reactions"
    }

    required init?(coder: NSCoder) {
        nil
    }

    var fittingWidth: CGFloat {
        2 * ReactionPillButton.inset + ReactionPillButton.digitWidth * CGFloat("+\(count)".count)
    }

    func show(_ count: Int) {
        self.count = count
        attributedTitle = NSAttributedString(
            string: "+\(count)",
            attributes: [
                .font: ReactionPillButton.countFont,
                .foregroundColor: isHovered ? Palette.interactiveTextActive : Palette.interactiveText,
            ])
        setAccessibilityLabel("\(count) more reactions")
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor =
                (isHovered ? Palette.interactiveBackgroundActive : Palette.hoverBackground).cgColor
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        show(count)
    }
}

/// The add button at a reaction row's end: always there where the user may react.
final class ReactionAddButton: NSButton {
    static let width: CGFloat = 34

    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateColors()
            }
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        refusesFirstResponder = true
        image = NSImage(systemSymbolName: "face.smiling", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 14, weight: .medium))
        imagePosition = .imageOnly
        imageScaling = .scaleNone
        (cell as? NSButtonCell)?.highlightsBy = []
        toolTip = ReactionWords.add
        setAccessibilityLabel(ReactionWords.add)
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = (isHovered ? Palette.panel : Palette.chat).cgColor
            layer?.borderColor = isHovered ? NSColor.white.withAlphaComponent(0.2).cgColor : .clear
        }
        contentTintColor = isHovered ? Palette.interactiveTextActive : Palette.interactiveText
    }
}

/// A Unicode emoji drawn with the system's emoji font at the size where a face's picture is as
/// wide as the view's box: every emoji at the same scale, as in a line of text, whatever the
/// font's own metrics.
final class EmojiGlyphView: NSView {
    var emoji = "" {
        didSet {
            if emoji != oldValue {
                needsDisplay = true
            }
        }
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        guard !emoji.isEmpty, bounds.width > 0, let context = NSGraphicsContext.current?.cgContext
        else {
            return
        }
        Self.draw(emoji, in: bounds, context: context, flipped: true)
    }

    // A face's picture at 16 points, from its line's origin: the measure for every emoji.
    private static let face: CGRect = {
        let space = CGColorSpaceCreateDeviceRGB()
        guard
            let context = CGContext(
                data: nil, width: 64, height: 64, bitsPerComponent: 8, bytesPerRow: 0, space: space,
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)
        else {
            return CGRect(x: 0, y: -3, width: 20, height: 20)
        }
        context.textPosition = .zero
        let ink = CTLineGetImageBounds(line("\u{1F600}", size: 16), context)
        return ink.width > 0 ? ink : CGRect(x: 0, y: -3, width: 20, height: 20)
    }()

    /// Draws `emoji` in `box`: a face fills it, and other emoji stand where a line of faces
    /// would have them.
    static func draw(_ emoji: String, in box: NSRect, context: CGContext, flipped: Bool) {
        let scale = min(box.width, box.height) / max(face.width, face.height)
        let line = line(emoji, size: 16 * scale)
        let advance = CTLineGetTypographicBounds(line, nil, nil, nil)
        // The face's middle above the baseline, at this size.
        let rise = face.midY * scale
        context.saveGState()
        context.textMatrix = flipped ? CGAffineTransform(scaleX: 1, y: -1) : .identity
        context.textPosition = CGPoint(
            x: box.midX - advance / 2, y: flipped ? box.midY + rise : box.midY - rise)
        CTLineDraw(line, context)
        context.restoreGState()
    }

    private static func line(_ emoji: String, size: CGFloat) -> CTLine {
        CTLineCreateWithAttributedString(
            NSAttributedString(
                string: emoji, attributes: [.font: NSFont.systemFont(ofSize: size)]))
    }
}
