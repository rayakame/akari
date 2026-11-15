import AkariKit
import AppKit
import SwiftUI

/// The official client's delete question by its words and its message box; the rest is Akari's
/// modal (the reactions window's surface, ring and radius).
enum DeleteLayout {
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

    /// Centered in `area`, as tall as its content and never taller than the area allows.
    static func frame(for question: MessageActionsModel.DeleteQuestion, in area: CGSize) -> CGRect {
        let width = min(Self.width, max(0, area.width - 2 * margin))
        let height = DeleteMessageView.sizing.height(
            for: question, width: width, maxHeight: max(0, area.height - 2 * margin))
        return CGRect(
            x: ((area.width - width) / 2).rounded(), y: ((area.height - height) / 2).rounded(),
            width: width, height: height)
    }
}

/// "Delete Message": the question, the message as its row shows it, the Shift tip when it came
/// from the menu, Cancel and Delete. Return deletes; Escape, Cancel and a press outside don't.
final class DeleteMessageView: NSView {
    typealias Question = MessageActionsModel.DeleteQuestion

    static let sizing = DeleteMessageView()

    let title = DeleteMessageView.label(Question.title, style: TypeScale.emptyTitle, color: Palette.textStrong)
    let message = DeleteMessageView.label(Question.message, style: TypeScale.body, color: Palette.textDefault)
    let box = MessagePreviewBox()
    let tip = DeleteMessageView.label("", style: TypeScale.detail, color: Palette.textMuted)
    let cancel = PanelButton(Question.cancel, kind: .secondary)
    let delete = PanelButton(Question.confirm, kind: .critical)
    /// Delete's `true`, Cancel's `false`.
    var answer: (Bool) -> Void = { _ in }
    private var question: Question?
    private var maxHeight: CGFloat = .greatestFiniteMagnitude

    override var isFlipped: Bool { true }
    override var wantsUpdateLayer: Bool { true }

    init() {
        super.init(frame: .zero)
        wantsLayer = true
        layer?.cornerRadius = DeleteLayout.radius
        layer?.borderWidth = 1
        title.setAccessibilityRole(.staticText)
        tip.attributedStringValue = Self.tipText
        cancel.keyEquivalent = "\u{1b}"
        cancel.target = self
        cancel.action = #selector(cancelled)
        delete.keyEquivalent = "\r"
        delete.hasDestructiveAction = true
        delete.target = self
        delete.action = #selector(confirmed)
        for view in [title, message, box, tip, cancel, delete] as [NSView] {
            view.translatesAutoresizingMaskIntoConstraints = true
            addSubview(view)
        }
        setAccessibilityElement(true)
        setAccessibilityRole(.group)
        setAccessibilityLabel(Question.title)
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(_ question: Question, images: ImageCache?, maxHeight: CGFloat) {
        self.question = question
        self.maxHeight = maxHeight
        tip.isHidden = !question.showsTip
        box.show(question.row, images: images)
        needsLayout = true
    }

    /// The panel's height for `question` at `width`, its box scrolling past `maxHeight`.
    func height(for question: Question, width: CGFloat, maxHeight: CGFloat) -> CGFloat {
        tip.isHidden = !question.showsTip
        return frames(width: width, row: question.row, maxHeight: maxHeight).height
    }

    private struct Frames {
        var title = NSRect.zero
        var message = NSRect.zero
        var box = NSRect.zero
        var tip = NSRect.zero
        var cancel = NSRect.zero
        var delete = NSRect.zero
        var height: CGFloat = 0
    }

    private func frames(width: CGFloat, row: MessageListModel.Row, maxHeight: CGFloat) -> Frames {
        let inset = DeleteLayout.inset
        let inner = max(0, width - 2 * inset)
        var frames = Frames()
        var y = inset
        frames.title = NSRect(x: inset, y: y, width: inner, height: Self.height(of: title, width: inner))
        y = frames.title.maxY + DeleteLayout.titleGap
        frames.message = NSRect(x: inset, y: y, width: inner, height: Self.height(of: message, width: inner))
        y = frames.message.maxY + DeleteLayout.boxGap
        let tipHeight = tip.isHidden ? 0 : DeleteLayout.boxGap + Self.height(of: tip, width: inner)
        let buttons = PanelButton.height
        let rest = tipHeight + DeleteLayout.buttonsGap + buttons + inset
        let room = max(DeleteLayout.minimumBox, maxHeight - y - rest)
        let box = min(MessagePreviewBox.height(of: row, width: inner), room)
        frames.box = NSRect(x: inset, y: y, width: inner, height: box)
        y = frames.box.maxY
        if !tip.isHidden {
            frames.tip = NSRect(
                x: inset, y: y + DeleteLayout.boxGap, width: inner, height: Self.height(of: tip, width: inner))
            y = frames.tip.maxY
        }
        y += DeleteLayout.buttonsGap
        let deleteWidth = delete.width
        let cancelWidth = cancel.width
        frames.delete = NSRect(x: width - inset - deleteWidth, y: y, width: deleteWidth, height: buttons)
        frames.cancel = NSRect(
            x: frames.delete.minX - DeleteLayout.buttonGap - cancelWidth, y: y, width: cancelWidth,
            height: buttons)
        frames.height = (y + buttons + inset).rounded(.up)
        return frames
    }

    override func layout() {
        super.layout()
        guard let question else {
            return
        }
        let frames = frames(width: bounds.width, row: question.row, maxHeight: maxHeight)
        // A label's text starts 2 into its frame.
        title.frame = frames.title.insetBy(dx: -2, dy: 0)
        message.frame = frames.message.insetBy(dx: -2, dy: 0)
        box.frame = frames.box
        tip.frame = frames.tip.insetBy(dx: -2, dy: 0)
        cancel.frame = frames.cancel
        delete.frame = frames.delete
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

    /// The tip, "delete message" in bold as the official client's.
    static var tipText: NSAttributedString {
        let text = NSMutableAttributedString(
            string: Question.tip,
            attributes: [.font: TypeScale.detail.nsFont, .foregroundColor: Palette.textMuted])
        let bold = (Question.tip as NSString).range(of: Question.tipBold)
        text.addAttribute(.font, value: TypeScale.detail.weight(.semibold).nsFont, range: bold)
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

/// The question in a SwiftUI panel: the view above, fed the session's images.
struct DeleteMessagePanel: NSViewRepresentable {
    let question: MessageActionsModel.DeleteQuestion
    let size: CGSize
    let maxHeight: CGFloat
    let answer: (Bool) -> Void
    @Environment(\.imageCache) private var images

    func makeNSView(context: Context) -> DeleteMessageView {
        DeleteMessageView()
    }

    func updateNSView(_ view: DeleteMessageView, context: Context) {
        view.answer = answer
        view.show(question, images: images, maxHeight: maxHeight)
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: DeleteMessageView, context: Context) -> CGSize? {
        size
    }
}

/// The official modal's buttons, as `BrandButton` draws them: secondary gray, critical red;
/// 40 high, at least 100 wide, 16 beside the title, a faint border, darker under the pointer.
final class PanelButton: NSButton {
    enum Kind {
        case secondary, critical
    }

    static let height: CGFloat = 40
    static let minWidth: CGFloat = 100
    static let padding: CGFloat = 16

    let kind: Kind
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
                .foregroundColor: kind == .critical ? NSColor.white : Palette.textStrong,
            ])
    }

    override func updateLayer() {
        let fill: NSColor =
            switch kind {
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
