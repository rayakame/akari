import AkariKit
import AppKit

/// The buttons over a hovered message, as the official client's hover bar: one view for the
/// whole list, placed and lit by the table's controller.
final class MessageHoverBar: NSView {
    static let height: CGFloat = 34
    // The border and the padding inside it.
    static let padding: CGFloat = 3
    /// From the row's right edge to the bar's.
    static let trailing: CGFloat = 14
    /// How far the bar's top lies above its row's box; less over a group's first message.
    static let overhang: CGFloat = 25
    static let headerOverhang: CGFloat = 16
    static let label = "Message Actions"

    private(set) var items: [MessageActionItem] = []
    private(set) var buttons: [HoverBarButton] = []
    /// A button's click, with the button.
    var onAction: (MessageAction, HoverBarButton) -> Void = { _, _ in }

    /// The pointer is over the bar: the stronger border and shadow.
    var isPointerInside = false {
        didSet {
            if isPointerInside != oldValue {
                updateColors(animated: true)
            }
        }
    }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        layer?.masksToBounds = false
        layer?.zPosition = 1
        isHidden = true
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(Self.label)
        updateColors(animated: false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    static func width(buttons: Int) -> CGFloat {
        CGFloat(buttons) * HoverBarButton.size + 2 * padding
    }

    func show(_ items: [MessageActionItem]) {
        guard items != self.items else {
            return
        }
        self.items = items
        for button in buttons {
            button.removeFromSuperview()
        }
        buttons = items.enumerated().map { index, item in
            let button = HoverBarButton(item)
            button.frame = NSRect(
                x: Self.padding + CGFloat(index) * HoverBarButton.size, y: Self.padding,
                width: HoverBarButton.size, height: HoverBarButton.size)
            button.target = self
            button.action = #selector(clicked(_:))
            addSubview(button)
            return button
        }
        setAccessibilityChildren(buttons)
    }

    func button(_ action: MessageAction) -> HoverBarButton? {
        buttons.first { $0.item.action == action }
    }

    @objc private func clicked(_ sender: HoverBarButton) {
        onAction(sender.item.action, sender)
    }

    // A click beside the buttons stays the bar's: the row under it gets nothing.
    override func mouseDown(with event: NSEvent) {}

    override func rightMouseDown(with event: NSEvent) {}

    override func menu(for event: NSEvent) -> NSMenu? {
        nil
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors(animated: false)
    }

    private func updateColors(animated: Bool) {
        guard let layer else {
            return
        }
        let dark = effectiveAppearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
        var border = CGColor.clear
        var background = CGColor.clear
        effectiveAppearance.performAsCurrentDrawingAppearance {
            background = Palette.surfaceHigh.cgColor
            border = (isPointerInside ? Palette.borderSubtle : Palette.borderMuted).cgColor
        }
        // Discord's shadow-low and shadow-medium.
        let opacity: Float = isPointerInside ? (dark ? 0.16 : 0.08) : (dark ? 0.14 : 0.08)
        let radius: CGFloat = isPointerInside ? 4 : 2
        let offset = CGSize(width: 0, height: isPointerInside ? -4 : -1)
        if animated {
            for (key, from, to) in [
                ("borderColor", layer.borderColor as Any, border as Any),
                ("shadowOpacity", layer.shadowOpacity as Any, opacity as Any),
                ("shadowRadius", layer.shadowRadius as Any, radius as Any),
                ("shadowOffset", layer.shadowOffset as Any, offset as Any),
            ] {
                let animation = CABasicAnimation(keyPath: key)
                animation.fromValue = from
                animation.toValue = to
                animation.duration = 0.1
                animation.timingFunction = CAMediaTimingFunction(name: .easeOut)
                layer.add(animation, forKey: key)
            }
        }
        layer.backgroundColor = background
        layer.borderColor = border
        layer.shadowColor = .black
        layer.shadowOpacity = opacity
        layer.shadowRadius = radius
        layer.shadowOffset = offset
    }
}

/// One button of the hover bar; the controller says when the pointer is over it.
final class HoverBarButton: NSButton {
    static let size: CGFloat = 28

    let item: MessageActionItem

    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateColors()
            }
        }
    }

    /// Its menu is open.
    var isSelected = false {
        didSet {
            if isSelected != oldValue {
                updateColors()
            }
        }
    }

    private(set) var isPressed = false {
        didSet { updateColors() }
    }

    init(_ item: MessageActionItem) {
        self.item = item
        super.init(frame: .zero)
        isBordered = false
        title = ""
        refusesFirstResponder = true
        image = Self.icon(item.symbol)
        imagePosition = .imageOnly
        imageScaling = .scaleNone
        (cell as? NSButtonCell)?.highlightsBy = []
        isEnabled = item.enabled
        alphaValue = item.enabled ? 1 : 0.5
        toolTip = item.title
        setAccessibilityLabel(item.title)
        wantsLayer = true
        layer?.cornerRadius = 6
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// One size for every symbol: at 16 their ink stays inside Discord's 20 point icon box.
    static func icon(_ symbol: String) -> NSImage? {
        NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 16, weight: .medium))
    }

    override func mouseDown(with event: NSEvent) {
        isPressed = true
        super.mouseDown(with: event)
        isPressed = false
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        nil
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    private func updateColors() {
        let fill: NSColor? =
            if isSelected {
                Palette.selectedBackground
            } else if isPressed {
                Palette.interactiveBackgroundActive
            } else if isHovered && isEnabled {
                Palette.hoverBackground
            } else {
                nil
            }
        effectiveAppearance.performAsCurrentDrawingAppearance {
            layer?.backgroundColor = fill?.cgColor
        }
        contentTintColor =
            item.destructive
            ? Palette.textError
            : (isHovered && isEnabled) || isSelected ? Palette.interactiveTextActive : Palette.interactiveText
    }
}
