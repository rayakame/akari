import AkariKit
import AppKit

/// One image or video of a message's mosaic (D141–D144): its picture from the cache at the cell's
/// pixels over Discord's placeholder, a spinner after 2 s, the failure line, our own GIF badge, a
/// video's Play, and the covers. A button: a click opens the media, reveals a spoiler or retries;
/// the list decides the cursor.
final class MediaTileView: NSButton {
    /// What covers the media now, the session's reveals applied.
    enum Cover: Equatable {
        case spoiler
        /// Waiting for Discord's scan: black, nothing to click.
        case pending
        /// Flagged; the eye button shows it and hides it again.
        case sensitive(shown: Bool)
    }

    enum Action {
        case open
        case reveal
        case toggleSensitive
        case altText
    }

    struct Content: Equatable {
        let attachment: Attachment
        var cover: Cover?
        /// The ALT button for media with a description (`view_image_descriptions`).
        var showsAltText = false
        /// Alone in its message: a sensitive cover says why where it fits.
        var single = false
    }

    static let spinnerDelay: Duration = .seconds(2)
    static let failureText = "Image failed to load."
    static let sensitiveText = "May contain sensitive content"
    static let spoilerText = "SPOILER"
    // Discord says why only where the cover is at least this large.
    static let sensitiveTextMinimum = NSSize(width: 146, height: 212)

    let picture = ImageLayerView()
    // Above the picture: AppKit keeps a subview's layer over layers added beside it.
    private let overlay = OverlayView()
    private let placeholderLayer = CALayer()
    private let coverLayer = CALayer()
    private let pill = Pill()
    private let warning = NSImageView()
    private let warningText = CellText.label()
    private let spinner = Spinner()
    private let failure = NSImageView()
    private let failureLabel = CellText.label()
    private let badge = Pill()
    private let play = CALayer()
    private let playSymbol = CALayer()
    let eye = OverlayButton()
    let alt = OverlayButton()
    private(set) var content: Content?
    private var spinnerTask: Task<Void, Never>?
    private(set) var spinnerShown = false
    var placeholders: PlaceholderCache?
    /// The mosaic's inner corners (2) or a single item's (8); a hidden spoiler's are 8.
    var cornerRadius: CGFloat = 0 {
        didSet { update() }
    }
    var onAction: (Action) -> Void = { _ in }
    /// Set by the list, which knows what's under the pointer.
    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateHover()
            }
        }
    }
    /// The list's say: a GIF plays (the policy, visibility, the hover).
    var animates = false {
        didSet {
            if animates != oldValue {
                updateMotion()
            }
        }
    }
    var images: ImageCache? {
        get { picture.images }
        set { picture.images = newValue }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        target = self
        action = #selector(clicked)
        refusesFirstResponder = true
        wantsLayer = true
        layer?.masksToBounds = true
        picture.tier = .media
        picture.fadesIn = true
        picture.placeholder = .none
        picture.fit = .cover
        picture.onStateChange = { [weak self] _ in self?.imageChanged() }
        for sublayer in [placeholderLayer, coverLayer, play, playSymbol] {
            sublayer.actions = Self.noActions
        }
        placeholderLayer.contentsGravity = .resizeAspectFill
        placeholderLayer.masksToBounds = true
        coverLayer.contentsGravity = .resizeAspectFill
        coverLayer.masksToBounds = true
        layer?.addSublayer(placeholderLayer)
        addSubview(picture)
        play.backgroundColor = NSColor.black.cgColor
        play.cornerRadius = 24
        play.opacity = 0.6
        playSymbol.contentsGravity = .center
        badge.text = "GIF"
        badge.font = .systemFont(ofSize: 11, weight: .bold)
        badge.padding = NSEdgeInsets(top: 4, left: 6, bottom: 4, right: 6)
        badge.layer.cornerRadius = 4
        pill.text = Self.spoilerText
        pill.font = .systemFont(ofSize: 15, weight: .semibold)
        pill.kerning = 0.5
        pill.padding = NSEdgeInsets(top: 8, left: 12, bottom: 8, right: 12)
        pill.layer.cornerRadius = 20
        overlay.wantsLayer = true
        addSubview(overlay)
        for sublayer in [coverLayer, play, playSymbol, pill.layer, badge.layer, spinner.layer] {
            overlay.layer?.addSublayer(sublayer)
        }
        warning.image = NSImage(
            systemSymbolName: "exclamationmark.triangle", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .regular))
        warning.contentTintColor = .white
        warningText.stringValue = Self.sensitiveText
        warningText.font = .systemFont(ofSize: TypeScale.size(14))
        warningText.textColor = .white
        warningText.alignment = .center
        failure.image = NSImage(
            systemSymbolName: "photo.badge.exclamationmark", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 28, weight: .regular))
        failure.contentTintColor = Palette.textMuted
        failureLabel.stringValue = Self.failureText
        failureLabel.font = .systemFont(ofSize: TypeScale.size(14), weight: .medium)
        failureLabel.textColor = Palette.textMuted
        failureLabel.alignment = .center
        eye.target = self
        eye.action = #selector(eyeClicked)
        eye.frameSize = NSSize(width: 32, height: 32)
        eye.cornerRadius = 8
        alt.target = self
        alt.action = #selector(altClicked)
        alt.text = "ALT"
        alt.cornerRadius = 4
        alt.setAccessibilityLabel("Show alt text")
        for view in [warning, warningText, failure, failureLabel, eye, alt] as [NSView] {
            view.isHidden = true
            addSubview(view)
        }
        update()
    }

    required init?(coder: NSCoder) {
        nil
    }

    private static let noActions: [String: CAAction] = [
        "contents": NSNull(), "hidden": NSNull(), "bounds": NSNull(), "position": NSNull(),
        "opacity": NSNull(), "backgroundColor": NSNull(), "cornerRadius": NSNull(),
    ]

    override var isFlipped: Bool { true }

    /// The overlay buttons take their own clicks; everything else is the tile's.
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard frame.contains(point) else {
            return nil
        }
        let local = convert(point, from: superview)
        for button in [eye, alt] where !button.isHidden && button.frame.contains(local) {
            return button
        }
        return self
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    /// `nil` lets the media go: no ask, no pin, no player.
    func configure(_ content: Content?) {
        guard content != self.content else {
            return
        }
        let sameMedia = content?.attachment == self.content?.attachment
        self.content = content
        if !sameMedia {
            hideSpinner()
        }
        let attachment = content?.attachment
        let covered = content.map { Self.hides($0.cover) } ?? true
        picture.show(
            attachment.flatMap { covered ? nil : ImageLayerView.Content(key: $0.key, canAnimate: $0.animated) })
        let thumbhash = attachment?.placeholder.flatMap { placeholders?.image($0) }
        placeholderLayer.contents = thumbhash
        setAccessibilityLabel(attachment.map(Self.accessibilityLabel))
        update()
        if !sameMedia {
            scheduleSpinner()
        }
    }

    static func hides(_ cover: Cover?) -> Bool {
        switch cover {
        case .spoiler, .pending, .sensitive(shown: false): true
        case nil, .sensitive(shown: true): false
        }
    }

    private static func accessibilityLabel(_ attachment: Attachment) -> String {
        let kind = attachment.kind == .video ? "Video" : "Image"
        return attachment.description.map { "\(kind), \($0)" } ?? kind
    }

    var isVideo: Bool { content?.attachment.kind == .video }
    var showsGifBadge: Bool { !badge.isHidden }
    var showsPlay: Bool { !play.isHidden }
    var coverShown: Cover? { content.flatMap { Self.hides($0.cover) ? $0.cover : nil } }
    var failureShown: Bool { !failureLabel.isHidden }
    var spoilerPillBackground: CGColor? { pill.backgroundColor }
    var spoilerPillShown: Bool { !pill.isHidden }
    var spoilerPillFrame: NSRect { pill.layer.frame }
    var sensitiveWarningShown: Bool { !warning.isHidden }
    var sensitiveTextShown: Bool { !warningText.isHidden }
    var placeholderShown: Bool { !placeholderLayer.isHidden && placeholderLayer.contents != nil }
    var spinnerFrame: NSRect { spinner.frame }
    var failureText: String { failureLabel.stringValue }

    @objc private func clicked() {
        guard let content else {
            return
        }
        switch content.cover {
        case .spoiler:
            onAction(.reveal)
        case .pending, .sensitive(shown: false):
            break
        case nil, .sensitive(shown: true):
            if picture.failed {
                picture.retry()
            } else {
                onAction(.open)
            }
        }
    }

    @objc private func eyeClicked() {
        onAction(.toggleSensitive)
    }

    @objc private func altClicked() {
        onAction(.altText)
    }

    // Only once the tile has its size: a single item says why where the sentence fits.
    private func updateSensitiveText() {
        warningText.isHidden =
            content?.cover != .sensitive(shown: false) || content?.single != true
            || bounds.width < Self.sensitiveTextMinimum.width
            || bounds.height < Self.sensitiveTextMinimum.height
    }

    override func layout() {
        super.layout()
        updateSensitiveText()
        picture.frame = bounds
        overlay.frame = bounds
        placeholderLayer.frame = bounds
        coverLayer.frame = bounds
        let center = NSPoint(x: bounds.midX, y: bounds.midY)
        play.frame = NSRect(x: center.x - 24, y: center.y - 24, width: 48, height: 48)
        playSymbol.frame = play.frame.offsetBy(dx: 1, dy: 0)
        pill.layer.position = center
        badge.layer.anchorPoint = .zero
        badge.layer.position = NSPoint(x: 6, y: 6)
        spinner.frame = NSRect(x: bounds.maxX - 4 - 24, y: 4, width: 24, height: 24)
        eye.frame = NSRect(x: bounds.maxX - 4 - 32, y: bounds.maxY - 4 - 32, width: 32, height: 32)
        let altSize = alt.intrinsicContentSize
        alt.frame = NSRect(
            x: 4.5, y: bounds.maxY - 4 - altSize.height, width: altSize.width, height: altSize.height)
        layoutCentered(warning.isHidden ? [] : [warning] + (warningText.isHidden ? [] : [warningText]), gap: 4)
        layoutCentered(failure.isHidden ? [] : [failure, failureLabel], gap: 8)
    }

    // A centered column, as Discord's overlays and failure line are.
    private func layoutCentered(_ views: [NSView], gap: CGFloat) {
        let width = bounds.width - 16
        let sizes = views.map { view -> NSSize in
            let size = view.intrinsicContentSize
            return NSSize(width: min(size.width, width), height: size.height)
        }
        let total = sizes.map(\.height).reduce(0, +) + gap * CGFloat(max(views.count - 1, 0))
        var y = (bounds.height - total) / 2
        for (view, size) in zip(views, sizes) {
            view.frame = NSRect(x: (bounds.width - size.width) / 2, y: y, width: size.width, height: size.height)
            y += size.height + gap
        }
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        update()
    }

    override func viewDidChangeBackingProperties() {
        super.viewDidChangeBackingProperties()
        let scale = window?.backingScaleFactor ?? 2
        for sublayer in [placeholderLayer, coverLayer, playSymbol] {
            sublayer.contentsScale = scale
        }
        pill.contentsScale = scale
        badge.contentsScale = scale
        update()
    }

    private func imageChanged() {
        if !picture.isLoading {
            hideSpinner()
        }
        update()
    }

    // The black 4 % box shows while nothing has loaded (Discord's image wrapper).
    private func update() {
        let attachment = content?.attachment
        let cover = content.flatMap { Self.hides($0.cover) ? $0.cover : nil }
        let failed = picture.failed && cover == nil
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = NSColor.black.withAlphaComponent(0.04).cgColor
            pill.backgroundColor = NSColor.black.withAlphaComponent(isHovered ? 0.88 : 0.6).cgColor
            pill.foregroundColor = isHovered ? .white : NSColor(hex: "#e4e4e6")
            badge.backgroundColor = NSColor.black.withAlphaComponent(0.6).cgColor
            badge.foregroundColor = .white
            switch cover {
            case .spoiler:
                coverLayer.backgroundColor = Palette.spoilerHidden.cgColor
            case .pending, .sensitive:
                coverLayer.backgroundColor = NSColor.black.cgColor
            case nil:
                coverLayer.backgroundColor = nil
            }
        }
        placeholderLayer.isHidden = attachment?.placeholder == nil || cover != nil
        coverLayer.isHidden = cover == nil
        coverLayer.contents = cover == .spoiler ? placeholderLayer.contents : nil
        pill.isHidden = cover != .spoiler
        let sensitive = cover == .sensitive(shown: false)
        warning.isHidden = !sensitive
        updateSensitiveText()
        eye.isHidden = !isSensitive
        eye.symbol = sensitive ? "eye.slash" : "eye"
        eye.setAccessibilityLabel(sensitive ? "Reveal sensitive content" : "Hide sensitive content")
        failure.isHidden = !failed
        failureLabel.isHidden = !failed
        let video = attachment?.kind == .video
        play.isHidden = !video || cover != nil || failed
        playSymbol.isHidden = play.isHidden
        playSymbol.contents = Self.playImage
        badge.isHidden = attachment?.animated != true || video || cover != nil || failed || animates
        alt.isHidden =
            content?.showsAltText != true || attachment?.description?.isEmpty != false || cover != nil
        // Hidden cells take Discord's 8 pt corners; shown ones the mosaic's.
        layer?.cornerRadius = cover == .spoiler ? 8 : cornerRadius
        isEnabled = attachment != nil && (cover == nil || cover == .spoiler || cover == .sensitive(shown: true))
        needsLayout = true
    }

    private var isSensitive: Bool {
        if case .sensitive = content?.cover { true } else { false }
    }

    private func updateHover() {
        update()
        updateMotion()
    }

    private func updateMotion() {
        picture.animates = animates && content?.attachment.animated == true
        update()
    }

    private static let playImage: CGImage? = {
        let configuration = NSImage.SymbolConfiguration(pointSize: 20, weight: .regular)
            .applying(.init(paletteColors: [.white]))
        guard let symbol = NSImage(systemSymbolName: "play.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(configuration)
        else {
            return nil
        }
        var rect = NSRect(x: 0, y: 0, width: 24, height: 24)
        return symbol.cgImage(forProposedRect: &rect, context: nil, hints: [.ctm: AffineTransform(scale: 2)])
    }()

    // Discord's spinner shows only for a load that takes over 2 s.
    private func scheduleSpinner() {
        spinnerTask?.cancel()
        guard picture.isLoading else {
            return
        }
        spinnerTask = Task { [weak self] in
            try? await Task.sleep(for: Self.spinnerDelay)
            guard let self, !Task.isCancelled, self.picture.isLoading else {
                return
            }
            self.spinnerShown = true
            self.spinner.show()
        }
    }

    private func hideSpinner() {
        spinnerTask?.cancel()
        spinnerTask = nil
        if spinnerShown {
            spinnerShown = false
            spinner.hide()
        }
    }
}

// Hosts the covers, the badge, Play and the spinner; takes no clicks.
private final class OverlayView: NSView {
    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

/// A rounded text label in a layer: the spoiler pill, the GIF badge.
final class Pill {
    let layer = CALayer()
    private let textLayer = CATextLayer()
    var text = "" {
        didSet { relayout() }
    }
    var font = NSFont.systemFont(ofSize: 12) {
        didSet { relayout() }
    }
    var kerning: CGFloat = 0 {
        didSet { relayout() }
    }
    var padding = NSEdgeInsetsZero {
        didSet { relayout() }
    }
    var foregroundColor: NSColor = .white {
        didSet { relayout() }
    }
    var backgroundColor: CGColor? {
        get { layer.backgroundColor }
        set { layer.backgroundColor = newValue }
    }
    var isHidden: Bool {
        get { layer.isHidden }
        set { layer.isHidden = newValue }
    }
    var contentsScale: CGFloat = 2 {
        didSet { textLayer.contentsScale = contentsScale }
    }

    init() {
        let still: [String: CAAction] = [
            "contents": NSNull(), "hidden": NSNull(), "bounds": NSNull(), "position": NSNull(),
            "backgroundColor": NSNull(),
        ]
        layer.actions = still
        textLayer.actions = still
        textLayer.alignmentMode = .center
        layer.addSublayer(textLayer)
    }

    var size: NSSize { layer.bounds.size }

    private func relayout() {
        let string = NSAttributedString(
            string: text,
            attributes: [.font: font, .foregroundColor: foregroundColor, .kern: kerning])
        textLayer.string = string
        let size = string.size()
        layer.bounds = NSRect(
            x: 0, y: 0, width: ceil(size.width) + padding.left + padding.right,
            height: ceil(size.height) + padding.top + padding.bottom)
        textLayer.frame = NSRect(
            x: padding.left, y: padding.top, width: ceil(size.width), height: ceil(size.height))
    }
}

/// Discord's loading spinner: a 24 pt black .48 disc with a turning white arc, fading in.
final class Spinner {
    let layer = CALayer()
    private let arc = CAShapeLayer()

    init() {
        layer.actions = ["bounds": NSNull(), "position": NSNull(), "hidden": NSNull()]
        layer.cornerRadius = 12
        layer.backgroundColor = NSColor.black.withAlphaComponent(0.48).cgColor
        layer.opacity = 0
        layer.isHidden = true
        arc.actions = ["bounds": NSNull(), "position": NSNull(), "path": NSNull()]
        arc.fillColor = nil
        arc.strokeColor = NSColor.white.withAlphaComponent(0.75).cgColor
        arc.lineWidth = 2
        arc.lineCap = .round
        arc.strokeEnd = 0.75
        layer.addSublayer(arc)
    }

    var frame: NSRect {
        get { layer.frame }
        set {
            layer.frame = newValue
            arc.frame = layer.bounds.insetBy(dx: 3, dy: 3)
            arc.path = CGPath(ellipseIn: arc.bounds.insetBy(dx: 1, dy: 1), transform: nil)
        }
    }

    var isShown: Bool { !layer.isHidden }

    func show() {
        layer.isHidden = false
        let fade = CABasicAnimation(keyPath: "opacity")
        fade.fromValue = 0
        fade.toValue = 1
        fade.duration = 0.8
        layer.opacity = 1
        layer.add(fade, forKey: "fade")
        let turn = CABasicAnimation(keyPath: "transform.rotation.z")
        turn.fromValue = 0
        turn.toValue = -2 * Double.pi
        turn.duration = 1
        turn.repeatCount = .infinity
        arc.add(turn, forKey: "turn")
    }

    func hide() {
        layer.removeAllAnimations()
        arc.removeAllAnimations()
        layer.opacity = 0
        layer.isHidden = true
    }
}

/// A small black button over media: the eye toggle, the ALT button.
final class OverlayButton: NSButton {
    var cornerRadius: CGFloat = 4 {
        didSet { layer?.cornerRadius = cornerRadius }
    }
    var frameSize: NSSize?
    var symbol: String? {
        didSet {
            guard symbol != oldValue else {
                return
            }
            image = symbol.flatMap {
                NSImage(systemSymbolName: $0, accessibilityDescription: nil)?
                    .withSymbolConfiguration(.init(pointSize: 18, weight: .regular))
            }
            imagePosition = .imageOnly
        }
    }
    var text: String? {
        didSet {
            attributedTitle = NSAttributedString(
                string: text ?? "",
                attributes: [
                    .font: NSFont.systemFont(ofSize: 14, weight: .semibold),
                    .foregroundColor: NSColor.white,
                ])
        }
    }
    var isHovered = false {
        didSet { updateFill() }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        refusesFirstResponder = true
        contentTintColor = .white
        wantsLayer = true
        layer?.cornerRadius = cornerRadius
        updateFill()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        if let frameSize {
            return frameSize
        }
        let size = attributedTitle.size()
        return NSSize(width: ceil(size.width) + 16, height: 32)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    private func updateFill() {
        layer?.backgroundColor = NSColor.black.withAlphaComponent(isHovered ? 0.8 : 0.6).cgColor
    }
}
