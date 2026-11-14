import AkariKit
import AppKit

final class NoticeCell: NSTableCellView, MessageMenuRow {
    static let identifier = NSUserInterfaceItemIdentifier("notice")

    private let icon = RowIcon()
    private(set) var shownIcon: NoticeIcon?
    private var iconWidth: NSLayoutConstraint?
    private var iconHeight: NSLayoutConstraint?
    private let text = MessageTextView()
    // Built for the first line with a box: buttons cost.
    private var threadBox: ThreadBoxView? {
        didSet {
            threadBox?.images = images
            threadBox?.animatesEmoji = animatesEmoji
        }
    }
    /// The list's say: the box's line and the reactions play their custom emoji.
    var animatesEmoji = false {
        didSet {
            threadBox?.animatesEmoji = animatesEmoji
            reactions?.animatesEmoji = animatesEmoji
        }
    }
    /// Where the thread box's avatar and the reactions' emoji come from.
    var images: ImageCache? {
        didSet {
            threadBox?.images = images
            reactions?.images = images
        }
    }
    // Built for the first line with reactions: a system line takes them like any message.
    private(set) var reactions: ReactionRowView?
    var onReaction: (ReactionPill) -> Void = { _ in }
    var onAddReaction: (NSView) -> Void = { _ in }
    var reactionTooltip: (ReactionPill) -> String? = { _ in nil }
    var reactionsRoll: () -> Bool = { false }
    // What stands under the text, top to bottom, and the constraints that stack it.
    private var chain: [NSLayoutConstraint] = []
    var onLink: (URL) -> Void = { _ in }
    var onOpenThread: () -> Void = {}
    /// The message's menu, asked when it opens.
    var menuItems: () -> [NSMenuItem] = { [] }
    private var message: Message?
    private static let top: CGFloat = 18
    // 10 less right after a day divider, as a message's (MessageCell).
    private static let dayTop: CGFloat = 8
    // Under the new messages line, whose row is the 16 above a group.
    private static let lineTop: CGFloat = 2
    private var textTop: NSLayoutConstraint?
    private static let bottom: CGFloat = 2
    // Discord's box border sits 9.5 below a line's baseline.
    private static let boxGap: CGFloat = 3.5
    // A line's reactions start 2 under it, as a message's under its text.
    private static let reactionsGap: CGFloat = 2
    private static let leading: CGFloat = 72
    private static let trailing: CGFloat = 16
    // Measures as the cell's own text view lays out; string drawing breaks the line differently.
    private static let sizing = MessageTextView()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        text.onLink = { [weak self] url in self?.onLink(url) }
        text.messageMenu = { [weak self] in self?.menuItems() ?? [] }
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.imageScaling = .scaleProportionallyDown
        let iconWidth = icon.widthAnchor.constraint(equalToConstant: 16)
        let iconHeight = icon.heightAnchor.constraint(equalToConstant: 16)
        self.iconWidth = iconWidth
        self.iconHeight = iconHeight
        addSubview(icon)
        addSubview(text)
        let textTop = text.topAnchor.constraint(equalTo: topAnchor, constant: Self.top)
        self.textTop = textTop
        NSLayoutConstraint.activate([
            textTop,
            text.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.leading),
            text.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.trailing),
            iconWidth,
            iconHeight,
            icon.centerXAnchor.constraint(equalTo: leadingAnchor, constant: 36),
            icon.centerYAnchor.constraint(equalTo: text.topAnchor, constant: 11),
        ])
        stackUnderText()
    }

    // The text, then the reactions, then the thread's box, whichever show; the last one ends
    // the row. The old constraints go before the new ones come: both at once can't be satisfied.
    private func stackUnderText() {
        NSLayoutConstraint.deactivate(chain)
        let under = ([reactions, threadBox] as [NSView?]).compactMap { $0 }.filter { !$0.isHidden }
        var above: NSView = text
        chain = under.map { view in
            defer { above = view }
            return view.topAnchor.constraint(equalTo: above.bottomAnchor)
        }
        chain.append(above.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom))
        NSLayoutConstraint.activate(chain)
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// The row height for a notice at `width`, as Auto Layout would size the cell.
    static func height(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, hasBox: Bool = false, reactions: ReactionRow? = nil,
        width: CGFloat
    ) -> CGFloat {
        sizing.show(line(notice, time: time))
        sizing.setFrameSize(NSSize(width: width - leading - trailing, height: sizing.frame.height))
        let pills = reactions.map {
            ReactionRowView.height($0, width: width - leading - trailing, topInset: reactionsGap)
        }
        return textTop(followsDay: followsDay, followsLine: followsLine)
            + sizing.intrinsicContentSize.height + (pills ?? 0)
            + (hasBox ? ThreadBoxView.height(topInset: boxGap) : 0) + bottom
    }

    /// The reactions under the line; `animated` for a change in place, whose counts roll.
    func showReactions(_ row: ReactionRow?, animated: Bool = false) {
        let was = reactions?.isHidden == false
        if let row {
            let view = reactionsView()
            view.isHidden = false
            view.configure(row, animated: animated)
        } else {
            reactions?.configure(nil)
            reactions?.isHidden = true
        }
        if was != (row != nil) {
            stackUnderText()
        }
    }

    private func reactionsView() -> ReactionRowView {
        if let reactions {
            return reactions
        }
        let row = ReactionRowView()
        row.images = images
        row.animatesEmoji = animatesEmoji
        row.topInset = Self.reactionsGap
        row.onPill = { [weak self] pill in self?.onReaction(pill) }
        row.onAdd = { [weak self] button in self?.onAddReaction(button) }
        row.tooltip = { [weak self] pill in self?.reactionTooltip(pill) }
        row.rolls = { [weak self] in self?.reactionsRoll() ?? false }
        row.isHidden = true
        addSubview(row)
        NSLayoutConstraint.activate([
            row.leadingAnchor.constraint(equalTo: text.leadingAnchor),
            row.trailingAnchor.constraint(equalTo: text.trailingAnchor),
        ])
        reactions = row
        return row
    }

    override func layout() {
        reactions?.preferredWidth = max(0, bounds.width - Self.leading - Self.trailing)
        super.layout()
    }

    func configure(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, fullDate: String, reader: UserId? = nil,
        threadBox: (box: ThreadBox, content: NSAttributedString?, time: String?)? = nil,
        reactions: ReactionRow? = nil
    ) {
        if let threadBox {
            boxView().configure(threadBox.box, content: threadBox.content, time: threadBox.time)
        } else {
            self.threadBox?.clear()
        }
        self.threadBox?.isHidden = threadBox == nil
        showReactions(reactions)
        stackUnderText()
        needsDisplay = true
        textTop?.constant = Self.textTop(followsDay: followsDay, followsLine: followsLine)
        let shown = NoticeIcon.of(message, reader: reader)
        if shown != shownIcon {
            shownIcon = shown
            icon.image = shown.image
            icon.contentTintColor = shown.color
            iconWidth?.constant = shown.size
            iconHeight?.constant = shown.size
        }
        text.show(Self.line(notice, time: time, fullDate: fullDate))
        self.message = message
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        messageMenu(for: event)
    }

    func messageMenu(for event: NSEvent) -> NSMenu? {
        guard message != nil, box.contains(convert(event.locationInWindow, from: nil)) else {
            return nil
        }
        return MessageMenu.menu(menuItems())
    }

    // The full date shows over the time only, as Discord's; TextKit keeps its rect in step.
    private static func line(
        _ notice: [Message.NoticePart], time: String, fullDate: String? = nil
    ) -> NSAttributedString {
        let gray = Palette.chatTextMuted
        let line = NSMutableAttributedString()
        for part in notice {
            switch part {
            case .text(let text):
                line.append(CellText.body(text, color: gray))
            case .actor(let name), .strong(let name), .pins(let name):
                line.append(Self.name(name, link: nil))
            case .threads(let name):
                line.append(Self.name(name, link: "akari://threads"))
            case .thread(let name, let id):
                line.append(Self.name(name, link: id.map { "akari://thread/\($0.rawValue)" }))
            case .message(let name, let id):
                line.append(Self.name(name, link: "akari://message/\(id.rawValue)"))
            case .channel(let name, let id):
                line.append(Self.name(name, link: "akari://channel/\(id.rawValue)"))
            case .guild(let name, let id):
                line.append(Self.name(name, link: "akari://guild/\(id.rawValue)"))
            }
        }
        line.append(CellText.body(" ", color: gray))
        // On the text's baseline, which the body raises (D122).
        var timeAttributes: [NSAttributedString.Key: Any] = [
            .font: TypeScale.small.nsFont, .foregroundColor: Palette.chatTextMuted,
            .baselineOffset: CellText.bodyRaise,
        ]
        if let fullDate, !fullDate.isEmpty {
            timeAttributes[.toolTip] = fullDate
        }
        line.append(NSAttributedString(string: time, attributes: timeAttributes))
        return line
    }

    private static func name(_ name: String, link: String?) -> NSAttributedString {
        let text = NSMutableAttributedString(
            attributedString: CellText.body(
                name, color: Palette.textStrong, style: TypeScale.noticeName))
        if let url = link.flatMap(URL.init(string:)) {
            text.addAttributes(
                [.link: url, MessageTextView.underlinesOnHover: true],
                range: NSRange(location: 0, length: text.length))
        }
        return text
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let threadBox, !threadBox.isHidden else {
            return
        }
        ThreadBoxView.drawConnector(
            under: icon.frame, to: threadBox.convert(threadBox.boxFrame, to: self),
            flipped: isFlipped)
    }

    @objc private func threadBoxClicked() {
        onOpenThread()
    }

    private func boxView() -> ThreadBoxView {
        if let threadBox {
            return threadBox
        }
        let box = ThreadBoxView()
        box.target = self
        box.action = #selector(threadBoxClicked)
        addSubview(box)
        box.topInset = Self.boxGap
        NSLayoutConstraint.activate([
            box.leadingAnchor.constraint(equalTo: text.leadingAnchor),
            box.trailingAnchor.constraint(equalTo: text.trailingAnchor),
        ])
        threadBox = box
        return box
    }

    /// The line's own area, which its menu covers; the space before it belongs to no message.
    var box: NSRect {
        // Under the new messages line, whose row is that space, only `lineTop` is left.
        MessageCell.box(of: self, spacing: (textTop?.constant ?? Self.top) - Self.lineTop)
    }

    /// The icon's box (its alignment rect: a symbol's frame adds its insets), from the cell's top
    /// left.
    var iconFrame: NSRect? {
        let frame = icon.alignmentRect(forFrame: icon.frame)
        return isFlipped
            ? frame
            : NSRect(
                x: frame.minX, y: bounds.height - frame.maxY, width: frame.width,
                height: frame.height)
    }

    /// Whether the symbol is drawn no larger than the icon's box.
    var iconFitsItsFrame: Bool {
        guard let image = icon.image else {
            return false
        }
        let scale = min(1, icon.bounds.width / image.size.width, icon.bounds.height / image.size.height)
        return image.size.width * scale <= icon.bounds.width + 0.01
            && image.size.height * scale <= icon.bounds.height + 0.01
    }

    static func textTop(followsDay: Bool, followsLine: Bool) -> CGFloat {
        followsLine ? lineTop : followsDay ? dayTop : top
    }
}
