import AkariKit
import AppKit
import SwiftUI

@MainActor
protocol MessageQuestion {
    var id: UUID { get }
    var row: MessageListModel.Row { get }
    var words: MessageQuestionWords { get }
}

struct MessageQuestionWords: Equatable {
    let title: String
    let message: String
    let tip: MessageTip?
    let confirm: String
    let cancel: String
    let confirmKind: PanelButton.Kind
}

extension MessageActionsModel.DeleteQuestion: MessageQuestion {
    var words: MessageQuestionWords {
        MessageQuestionWords(
            title: Self.title, message: Self.message,
            tip: showsTip ? MessageTip(label: nil, text: Self.tip, bold: Self.tipBold) : nil,
            confirm: Self.confirm, cancel: Self.cancel, confirmKind: .critical)
    }
}

extension MessageActionsModel.PinQuestion: MessageQuestion {
    var words: MessageQuestionWords {
        MessageQuestionWords(
            title: title, message: message, tip: tip, confirm: confirm, cancel: cancel,
            confirmKind: destructive ? .critical : .primary)
    }
}

enum MessageQuestionLayout {
    static let width: CGFloat = 440
    static let margin: CGFloat = 40
    static let inset: CGFloat = 24
    static let titleGap: CGFloat = 8
    // The official box's margins.
    static let boxGap: CGFloat = 20
    static let buttonsGap: CGFloat = 24
    static let buttonGap: CGFloat = 12
    static let radius: CGFloat = 12
    // The least of the message that shows before the box scrolls.
    static let minimumBox: CGFloat = 80

    static func frame(for question: some MessageQuestion, in area: CGSize) -> CGRect {
        let width = min(Self.width, max(0, area.width - 2 * margin))
        let height = MessageQuestionView.sizing.height(
            for: question, width: width, maxHeight: max(0, area.height - 2 * margin))
        return CGRect(
            x: ((area.width - width) / 2).rounded(), y: ((area.height - height) / 2).rounded(),
            width: width, height: height)
    }
}

final class MessageQuestionView: NSView {
    static let sizing = MessageQuestionView()

    let title = MessageQuestionView.label("", style: TypeScale.emptyTitle, color: Palette.textStrong)
    let message = MessageQuestionView.label("", style: TypeScale.body, color: Palette.textDefault)
    let box = MessagePreviewBox()
    let tip = MessageQuestionView.label("", style: TypeScale.detail, color: Palette.textMuted)
    let cancel = PanelButton("", kind: .secondary)
    let confirm = PanelButton("", kind: .critical)
    var answer: (Bool) -> Void = { _ in }
    private var row: MessageListModel.Row?
    private var words: MessageQuestionWords?
    private var maxHeight: CGFloat = .greatestFiniteMagnitude

    override var isFlipped: Bool { true }
    override var wantsUpdateLayer: Bool { true }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = MessageQuestionLayout.radius
        layer?.borderWidth = 1
        title.setAccessibilityRole(.staticText)
        cancel.keyEquivalent = "\u{1b}"
        cancel.target = self
        cancel.action = #selector(cancelled)
        confirm.keyEquivalent = "\r"
        confirm.target = self
        confirm.action = #selector(confirmed)
        for view in [title, message, box, tip, cancel, confirm] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = true
            addSubview(view)
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ question: some MessageQuestion, images: ImageCache?, maxHeight: CGFloat) {
        self.maxHeight = maxHeight
        row = question.row
        put(question.words)
        box.show(question.row, images: images)
        needsLayout = true
    }

    func height(for question: some MessageQuestion, width: CGFloat, maxHeight: CGFloat) -> CGFloat {
        put(question.words)
        return frames(width: width, row: question.row, maxHeight: maxHeight).height
    }

    private func put(_ words: MessageQuestionWords) {
        guard words != self.words else {
            return
        }
        self.words = words
        title.stringValue = words.title
        message.stringValue = words.message
        tip.isHidden = words.tip == nil
        tip.attributedStringValue = words.tip.map(Self.tipText) ?? NSAttributedString()
        cancel.show(words.cancel, kind: .secondary)
        confirm.show(words.confirm, kind: words.confirmKind)
        confirm.hasDestructiveAction = words.confirmKind == .critical
        setAccessibilityLabel(words.title)
    }

    private struct Frames {
        var title = NSRect.zero
        var message = NSRect.zero
        var box = NSRect.zero
        var tip = NSRect.zero
        var cancel = NSRect.zero
        var confirm = NSRect.zero
        var height: CGFloat = 0
    }

    private func frames(width: CGFloat, row: MessageListModel.Row, maxHeight: CGFloat) -> Frames {
        let inset = MessageQuestionLayout.inset
        let inner = max(0, width - 2 * inset)
        var frames = Frames()
        var y = inset
        frames.title = NSRect(x: inset, y: y, width: inner, height: Self.height(of: title, width: inner))
        y = frames.title.maxY + MessageQuestionLayout.titleGap
        frames.message = NSRect(x: inset, y: y, width: inner, height: Self.height(of: message, width: inner))
        y = frames.message.maxY + MessageQuestionLayout.boxGap
        let tipHeight = tip.isHidden ? 0 : MessageQuestionLayout.boxGap + Self.height(of: tip, width: inner)
        let buttons = PanelButton.height
        let rest = tipHeight + MessageQuestionLayout.buttonsGap + buttons + inset
        let room = max(MessageQuestionLayout.minimumBox, maxHeight - y - rest)
        let box = min(MessagePreviewBox.height(of: row, width: inner), room)
        frames.box = NSRect(x: inset, y: y, width: inner, height: box)
        y = frames.box.maxY
        if !tip.isHidden {
            frames.tip = NSRect(
                x: inset, y: y + MessageQuestionLayout.boxGap, width: inner, height: Self.height(of: tip, width: inner))
            y = frames.tip.maxY
        }
        y += MessageQuestionLayout.buttonsGap
        let confirmWidth = confirm.width
        let cancelWidth = cancel.width
        frames.confirm = NSRect(x: width - inset - confirmWidth, y: y, width: confirmWidth, height: buttons)
        frames.cancel = NSRect(
            x: frames.confirm.minX - MessageQuestionLayout.buttonGap - cancelWidth, y: y, width: cancelWidth,
            height: buttons)
        frames.height = (y + buttons + inset).rounded(.up)
        return frames
    }

    override func layout() {
        super.layout()
        guard let row else {
            return
        }
        let frames = frames(width: bounds.width, row: row, maxHeight: maxHeight)
        // A label's text starts 2 into its frame.
        title.frame = frames.title.insetBy(dx: -2, dy: 0)
        message.frame = frames.message.insetBy(dx: -2, dy: 0)
        box.frame = frames.box
        tip.frame = frames.tip.insetBy(dx: -2, dy: 0)
        cancel.frame = frames.cancel
        confirm.frame = frames.confirm
    }

    override func updateLayer() {
        layer?.backgroundColor = Palette.surfaceHigh.cgColor
        layer?.borderColor = Palette.borderSubtle.cgColor
    }

    @objc private func cancelled() {
        answer(false)
    }

    @objc private func confirmed() {
        answer(true)
    }

    static func tipText(_ tip: MessageTip) -> NSAttributedString {
        let text = NSMutableAttributedString()
        if let label = tip.label {
            text.append(
                NSAttributedString(
                    string: label + " ",
                    attributes: [
                        .font: TypeScale.detail.weight(.semibold).nsFont,
                        .foregroundColor: Palette.statusPositive,
                    ]))
        }
        let start = text.length
        text.append(
            NSAttributedString(
                string: tip.text,
                attributes: [.font: TypeScale.detail.nsFont, .foregroundColor: Palette.textMuted]))
        let bold = (tip.text as NSString).range(of: tip.bold)
        if bold.location != NSNotFound {
            text.addAttribute(
                .font, value: TypeScale.detail.weight(.semibold).nsFont,
                range: NSRange(location: start + bold.location, length: bold.length))
        }
        return text
    }

    private static func label(_ text: String, style: TextStyle, color: NSColor) -> NSTextField {
        let label = NSTextField(wrappingLabelWithString: text)
        label.font = style.nsFont
        label.textColor = color
        label.isSelectable = false
        label.drawsBackground = false
        label.isBordered = false
        return label
    }

    private static func height(of label: NSTextField, width: CGFloat) -> CGFloat {
        ceil(label.cell?.cellSize(forBounds: NSRect(x: 0, y: 0, width: width, height: .greatestFiniteMagnitude)).height ?? 0)
    }
}

struct MessageQuestionPanel: NSViewRepresentable {
    let question: any MessageQuestion
    let size: CGSize
    let maxHeight: CGFloat
    let answer: (Bool) -> Void
    @Environment(\.imageCache) private var images

    func makeNSView(context: Context) -> MessageQuestionView {
        MessageQuestionView()
    }

    func updateNSView(_ view: MessageQuestionView, context: Context) {
        view.answer = answer
        view.show(question, images: images, maxHeight: maxHeight)
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: MessageQuestionView, context: Context) -> CGSize? {
        size
    }
}

// An NSButton's bezel color shows only while its window is key, so these draw BrandButton's.
final class PanelButton: NSButton {
    enum Kind {
        case primary, secondary, critical
    }

    static let height: CGFloat = 40
    static let minWidth: CGFloat = 100
    static let padding: CGFloat = 16

    private(set) var kind: Kind
    private(set) var isHovered = false {
        didSet { needsDisplay = true }
    }

    override var wantsUpdateLayer: Bool { true }

    init(_ title: String, kind: Kind) {
        self.kind = kind
        super.init(frame: .zero)
        self.title = title
        isBordered = false
        wantsLayer = true
        layer?.cornerRadius = 8
        layer?.borderWidth = 1
        updateTitle()
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ title: String, kind: Kind) {
        self.title = title
        self.kind = kind
        updateTitle()
        needsDisplay = true
    }

    var width: CGFloat {
        let text = (title as NSString).size(withAttributes: [.font: TypeScale.button.nsFont]).width
        return max(Self.minWidth, text.rounded(.up) + 2 * Self.padding)
    }

    private func updateTitle() {
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        attributedTitle = NSAttributedString(
            string: title,
            attributes: [
                .font: TypeScale.button.nsFont, .paragraphStyle: paragraph,
                .foregroundColor: kind == .secondary ? Palette.textStrong : NSColor.white,
            ])
    }

    override func updateLayer() {
        let fill: NSColor =
            switch kind {
            case .primary: isHovered ? Palette.brandHover : Palette.brand
            case .secondary: isHovered ? Palette.backgroundModStrong : Palette.backgroundModNormal
            case .critical: isHovered ? Palette.dangerHover : Palette.danger
            }
        layer?.backgroundColor = fill.cgColor
        layer?.borderColor = NSColor.white.withAlphaComponent(0.08).cgColor
    }

    override func viewDidChangeEffectiveAppearance() {
        super.viewDidChangeEffectiveAppearance()
        updateTitle()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas where area.owner === self {
            removeTrackingArea(area)
        }
        addTrackingArea(
            NSTrackingArea(
                rect: .zero, options: [.mouseEnteredAndExited, .activeAlways, .inVisibleRect, .cursorUpdate],
                owner: self))
    }

    override func mouseEntered(with event: NSEvent) {
        isHovered = true
    }

    override func mouseExited(with event: NSEvent) {
        isHovered = false
    }

    override func cursorUpdate(with event: NSEvent) {
        NSCursor.pointingHand.set()
    }
}
