import AppKit

/// Discord's hover button on media and files (module 96782, CSS 830560): 32 square on base-low
/// with corners 5, a 20 pt symbol in the interactive text color, the hover fill and text under
/// the pointer. It shows while its media or file card is under the pointer, which the list says;
/// on a file card it overhangs the corner, ringed 1 pt in the chat's color.
final class HoverActionButton: NSButton {
    static let size: CGFloat = 32
    static let download = "Download"

    private let ringed: Bool

    /// Under the pointer: the hover fill and text.
    var isHovered = false {
        didSet {
            if isHovered != oldValue {
                updateColors()
            }
        }
    }

    /// Its media or card is under the pointer; hidden, it still takes the pointer, as Discord's
    /// transparent group does.
    var isShown = false {
        didSet { alphaValue = isShown ? 1 : 0 }
    }

    init(symbol: String, label: String, ringed: Bool = false) {
        self.ringed = ringed
        super.init(frame: .zero)
        isBordered = false
        title = ""
        refusesFirstResponder = true
        image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 16, weight: .medium))
        imagePosition = .imageOnly
        imageScaling = .scaleNone
        toolTip = label
        setAccessibilityLabel(label)
        wantsLayer = true
        layer?.cornerRadius = ringed ? 6 : 5
        layer?.borderWidth = ringed ? 1 : 0
        layer?.masksToBounds = true
        alphaValue = 0
        updateColors()
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// Its frame for a 32 box at `box`: the ring takes a point around it.
    func frame(box: NSRect) -> NSRect {
        ringed ? box.insetBy(dx: -1, dy: -1) : box
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateColors()
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }

    // The hover fill is translucent over base-low: blended, so the symbol stays on top.
    private func updateColors() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            let base = Palette.panel.usingColorSpace(.sRGB) ?? Palette.panel
            let hover = Palette.hoverBackground.usingColorSpace(.sRGB)
            let fill =
                isHovered
                ? hover.flatMap { base.blended(withFraction: $0.alphaComponent, of: $0.withAlphaComponent(1)) }
                    ?? base : base
            layer?.backgroundColor = fill.cgColor
            layer?.borderColor = Palette.chat.cgColor
        }
        contentTintColor = isHovered ? Palette.interactiveTextActive : Palette.interactiveText
    }
}
