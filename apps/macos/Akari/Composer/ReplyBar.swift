import AkariKit
import AppKit
import SwiftUI

/// The band over the composer's text while a reply is pending: "Replying to Name", the ping's
/// switch where it shows, and ✕. A click on the band goes to the message.
final class ReplyBarView: NSView {
    /// Kept clear above the bar's row, inside the band.
    static let clipPadding: CGFloat = 3
    static let labelLeading: CGFloat = 16
    static let trailing: CGFloat = 4
    static let separatorSize = NSSize(width: 1, height: 20)
    static let mentionOn = "Click to disable pinging the original author."
    static let mentionOff = "Click to enable pinging the original author."
    static let cancel = "Cancel reply"
    private static let font = NSFont.systemFont(ofSize: TypeScale.size(14))
    private static let nameFont = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .semibold)
    private static let line: CGFloat = 18

    /// The band's height: the switch's row is 34, the ✕ alone 32.
    static func height(showsToggle: Bool) -> CGFloat {
        clipPadding + (showsToggle ? 34 : 32)
    }

    private(set) var reply: PendingReply?
    let toggle = ReplyBarButton(symbol: "at", padding: NSSize(width: 12, height: 8))
    let close = ReplyBarButton(
        symbol: "xmark.circle.fill", padding: NSSize(width: 12, height: 8), pointSize: 14)
    let separator = NSView()
    var onJump: () -> Void = {}
    var onToggle: () -> Void = {}
    var onCancel: () -> Void = {}
    private var tracking: NSTrackingArea?

    init() {
        super.init(frame: .zero)
        separator.wantsLayer = true
        toggle.target = self
        toggle.action = #selector(toggleClicked)
        toggle.setAccessibilityRole(.checkBox)
        toggle.setAccessibilityLabel("Mention")
        close.target = self
        close.action = #selector(closeClicked)
        close.setAccessibilityLabel(Self.cancel)
        close.toolTip = Self.cancel
        for view in [toggle, separator, close] as [NSView] {
            addSubview(view)
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    func show(_ reply: PendingReply) {
        self.reply = reply
        toggle.isHidden = !reply.showsToggle
        separator.isHidden = !reply.showsToggle
        toggle.title = reply.mention ? "ON" : "OFF"
        toggle.tint = reply.mention ? Palette.textLink : Palette.textMuted
        toggle.toolTip = reply.mention ? Self.mentionOn : Self.mentionOff
        toggle.setAccessibilityValue(reply.mention ? "on" : "off")
        close.tint = close.isHovered ? Palette.interactiveTextActive : Palette.interactiveText
        setAccessibilityLabel("Replying to \(reply.authorName)")
        recolor()
        needsLayout = true
        needsDisplay = true
    }

    /// "Replying to Name", the name semibold.
    var label: NSAttributedString {
        let label = NSMutableAttributedString(
            string: "Replying to ",
            attributes: [.font: Self.font, .foregroundColor: Palette.textDefault])
        label.append(
            NSAttributedString(
                string: reply?.authorName ?? "",
                attributes: [.font: Self.nameFont, .foregroundColor: Palette.textDefault]))
        return label
    }

    // The row under the clip padding.
    private var row: NSRect {
        NSRect(
            x: 0, y: Self.clipPadding, width: bounds.width,
            height: max(0, bounds.height - Self.clipPadding))
    }

    /// Where the label draws: from 16 in to the first button.
    var labelFrame: NSRect {
        let end = (toggle.isHidden ? close : toggle).frame.minX
        let row = row
        return NSRect(
            x: Self.labelLeading, y: row.midY - Self.line / 2,
            width: max(0, min(ceil(label.size().width), end - Self.labelLeading)), height: Self.line)
    }

    override func layout() {
        super.layout()
        let row = row
        let closeSize = close.fittingSize
        close.frame = NSRect(
            x: bounds.width - Self.trailing - closeSize.width, y: row.midY - closeSize.height / 2,
            width: closeSize.width, height: closeSize.height)
        separator.frame = NSRect(
            x: close.frame.minX - Self.separatorSize.width,
            y: row.midY - Self.separatorSize.height / 2, width: Self.separatorSize.width,
            height: Self.separatorSize.height)
        let toggleSize = toggle.fittingSize
        toggle.frame = NSRect(
            x: separator.frame.minX - toggleSize.width, y: row.midY - toggleSize.height / 2,
            width: toggleSize.width, height: toggleSize.height)
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        recolor()
    }

    private func recolor() {
        effectiveAppearance.performAsCurrentDrawingAppearance {
            separator.layer?.backgroundColor = Palette.borderSubtle.cgColor
        }
    }

    override func draw(_ dirtyRect: NSRect) {
        Palette.surfaceHigher.setFill()
        bounds.fill()
        let frame = labelFrame
        let baseline = PreviewLineView.baseline(in: frame.height, font: Self.font)
        label.draw(
            with: NSRect(x: frame.minX, y: frame.minY + baseline, width: frame.width, height: 0),
            options: [.truncatesLastVisibleLine], context: nil)
    }

    override func mouseDown(with event: NSEvent) {}

    override func mouseUp(with event: NSEvent) {
        if bounds.contains(convert(event.locationInWindow, from: nil)) {
            onJump()
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if let tracking {
            removeTrackingArea(tracking)
        }
        let area = NSTrackingArea(
            rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: self)
        addTrackingArea(area)
        tracking = area
    }

    override func mouseEntered(with event: NSEvent) {
        pointerMoved(to: convert(event.locationInWindow, from: nil))
    }

    override func mouseMoved(with event: NSEvent) {
        pointerMoved(to: convert(event.locationInWindow, from: nil))
    }

    override func mouseExited(with event: NSEvent) {
        pointerMoved(to: nil)
    }

    /// The pointer's place in the bar, `nil` outside it: ✕ brightens under it.
    func pointerMoved(to point: NSPoint?) {
        close.isHovered = point.map(close.frame.contains) ?? false
        close.tint = close.isHovered ? Palette.interactiveTextActive : Palette.interactiveText
    }

    @objc private func toggleClicked() {
        onToggle()
    }

    @objc private func closeClicked() {
        onCancel()
    }
}

/// A button of the reply bar: a 16 pt symbol, and for the switch its word; no background.
final class ReplyBarButton: NSButton {
    static let iconSize: CGFloat = 16
    static let iconGap: CGFloat = 4
    private static let font = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .semibold)
    private static let line: CGFloat = 18

    private let icon = NSImageView()
    private let label = CellText.label()
    private let padding: NSSize
    let symbolName: String
    var isHovered = false

    var tint: NSColor = Palette.interactiveText {
        didSet {
            icon.contentTintColor = tint
            label.textColor = tint
        }
    }

    override var title: String {
        didSet {
            label.stringValue = title
            label.isHidden = title.isEmpty
            needsLayout = true
        }
    }

    init(symbol: String, padding: NSSize, pointSize: CGFloat = 13) {
        self.padding = padding
        symbolName = symbol
        super.init(frame: .zero)
        isBordered = false
        setButtonType(.momentaryChange)
        icon.image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)
        icon.symbolConfiguration = .init(pointSize: pointSize, weight: .medium)
        icon.imageScaling = .scaleProportionallyDown
        label.font = Self.font
        label.translatesAutoresizingMaskIntoConstraints = true
        label.isHidden = true
        addSubview(icon)
        addSubview(label)
        title = ""
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    private var labelWidth: CGFloat {
        label.isHidden ? 0 : ceil(label.intrinsicContentSize.width)
    }

    override var fittingSize: NSSize {
        let content = Self.iconSize + (label.isHidden ? 0 : Self.iconGap + labelWidth)
        let height = label.isHidden ? Self.iconSize : Self.line
        return NSSize(width: content + 2 * padding.width, height: height + 2 * padding.height)
    }

    var iconFrame: NSRect { icon.frame }
    var labelFrame: NSRect? { label.isHidden ? nil : label.frame }

    override func layout() {
        super.layout()
        icon.frame = NSRect(
            x: padding.width, y: (bounds.height - Self.iconSize) / 2, width: Self.iconSize,
            height: Self.iconSize)
        let height = label.intrinsicContentSize.height
        label.frame = NSRect(
            x: icon.frame.maxX + Self.iconGap, y: (bounds.height - height) / 2, width: labelWidth,
            height: height)
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        !isHidden && frame.contains(point) ? self : nil
    }

    override func draw(_ dirtyRect: NSRect) {}
}

/// `ReplyBarView` over the composer's text.
struct ReplyBar: NSViewRepresentable {
    let reply: PendingReply
    let jump: () -> Void
    let toggle: () -> Void
    let cancel: () -> Void

    func makeNSView(context: Context) -> ReplyBarView {
        ReplyBarView()
    }

    func updateNSView(_ view: ReplyBarView, context: Context) {
        view.onJump = jump
        view.onToggle = toggle
        view.onCancel = cancel
        view.show(reply)
    }
}
