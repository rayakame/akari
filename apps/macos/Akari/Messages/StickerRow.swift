import AkariKit
import AppKit

/// A message's stickers under its text (D140): 160 × 160 boxes in a row, 8 apart. A sticker
/// takes a click (its popout); the list decides the cursor, the row the menu.
final class StickerRow: NSView {
    static let height = StickerView.side
    static let gap: CGFloat = 8
    private(set) var stickers: [StickerView] = []
    var images: ImageCache? {
        didSet {
            if images !== oldValue {
                stickers.forEach { $0.images = images }
            }
        }
    }
    /// A click on a sticker, with its box in window coordinates.
    var onSticker: (Sticker, NSRect) -> Void = { _, _ in }
    /// The list's say: the message's stickers play.
    var animates = false {
        didSet {
            if animates != oldValue {
                stickers.forEach { $0.animates = animates }
            }
        }
    }

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: Self.height)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        let local = convert(point, from: superview)
        return stickers.first { !$0.isHidden && $0.frame.contains(local) }
    }

    func configure(_ shown: [Sticker]) {
        for (index, sticker) in shown.enumerated() {
            let view: StickerView
            if index < stickers.count {
                view = stickers[index]
            } else {
                view = StickerView()
                view.images = images
                view.animates = animates
                view.onClick = { [weak self, weak view] sticker in
                    guard let self, let view else {
                        return
                    }
                    self.onSticker(sticker, view.convert(view.bounds, to: nil))
                }
                stickers.append(view)
                addSubview(view)
            }
            view.isHidden = false
            view.configure(sticker)
        }
        for view in stickers.dropFirst(shown.count) {
            view.isHidden = true
            view.configure(nil)
        }
        needsLayout = true
    }

    override func layout() {
        super.layout()
        for (index, view) in stickers.enumerated() {
            view.frame = NSRect(
                x: CGFloat(index) * (StickerView.side + Self.gap), y: 0, width: StickerView.side,
                height: StickerView.side)
        }
    }
}

/// One sticker: its image from the cache in the sticker's format, animated while the row says
/// so; a Lottie sticker (no renderer yet) a box with a face and the sticker's name; a failed one
/// a warning and "Uh Oh!", as Discord words it. A button: a click opens its popout.
final class StickerView: NSButton {
    static let side = CGFloat(stickerSize().width)
    /// Akari's sticker symbol: the Lottie box's, the hover card's.
    static let symbol = "face.smiling"
    private static let symbolSize: CGFloat = 40
    let picture = ImageLayerView()
    private let symbol = NSImageView()
    private let label = CellText.label()
    private(set) var symbolName: String?
    private(set) var sticker: Sticker?
    var onClick: (Sticker) -> Void = { _ in }
    var images: ImageCache? {
        get { picture.images }
        set { picture.images = newValue }
    }
    var animates: Bool {
        get { picture.animates }
        set { picture.animates = newValue }
    }

    /// The box's text: the Lottie sticker's name, or "Uh Oh!"; `nil` while the image shows.
    var caption: String? {
        label.isHidden ? nil : label.stringValue
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        target = self
        action = #selector(clicked)
        // The messages aren't Tab stops; VoiceOver presses it.
        refusesFirstResponder = true
        wantsLayer = true
        layer?.cornerRadius = 8
        picture.placeholder = .none
        picture.fit = .contain
        label.font = TypeScale.detail.nsFont
        label.textColor = Palette.textMuted
        label.alignment = .center
        label.lineBreakMode = .byTruncatingTail
        label.translatesAutoresizingMaskIntoConstraints = true
        symbol.imageScaling = .scaleNone
        symbol.contentTintColor = Palette.textMuted
        for view in [picture, symbol, label] as [NSView] {
            addSubview(view)
        }
        picture.onStateChange = { [weak self] _ in self?.updateBox() }
        updateBox()
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    // The image, the symbol and the caption take no clicks of their own.
    override func hitTest(_ point: NSPoint) -> NSView? {
        frame.contains(point) ? self : nil
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    @objc private func clicked() {
        if let sticker {
            onClick(sticker)
        }
    }

    func configure(_ sticker: Sticker?) {
        self.sticker = sticker
        setAccessibilityLabel(sticker.map { "Sticker, \($0.name)" })
        guard let sticker, sticker.format != .lottie else {
            picture.show(nil)
            updateBox()
            return
        }
        picture.show(ImageLayerView.Content(key: sticker.key))
        updateBox()
    }

    override func layout() {
        super.layout()
        picture.frame = bounds
        symbol.frame = NSRect(
            x: 0, y: (bounds.height - Self.symbolSize) / 2 - 14, width: bounds.width,
            height: Self.symbolSize)
        label.frame = NSRect(x: 8, y: symbol.frame.maxY + 8, width: bounds.width - 16, height: 20)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateBox()
    }

    private func updateBox() {
        let box: (symbol: String, text: String)? =
            if let sticker, sticker.format == .lottie {
                (Self.symbol, sticker.name)
            } else if picture.failed {
                ("exclamationmark.triangle", "Uh Oh!")
            } else {
                nil
            }
        symbolName = box?.symbol
        picture.isHidden = box != nil || sticker == nil
        symbol.isHidden = box == nil
        label.isHidden = box == nil
        guard let box else {
            layer?.backgroundColor = nil
            return
        }
        symbol.image = NSImage(systemSymbolName: box.symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: Self.symbolSize, weight: .regular))
        label.stringValue = box.text
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = Palette.codeBackground.cgColor
        }
    }
}
