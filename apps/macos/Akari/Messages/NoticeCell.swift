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
    private var threadBox: ThreadBoxView?
    var onLink: (URL) -> Void = { _ in }
    var onOpenThread: () -> Void = {}
    // As a message's: asked when the menu opens, nil leaves "Create Thread" out.
    var threadAction: () -> (() -> Void)? = { nil }
    private var message: Message?
    private var textBottom: NSLayoutConstraint?
    private var textAboveBox: NSLayoutConstraint?
    private static let top: CGFloat = 18
    // 10 less right after a day divider, as a message's (MessageCell).
    private static let dayTop: CGFloat = 8
    // Under the new messages line, whose row is the 16 above a group.
    private static let lineTop: CGFloat = 2
    private var textTop: NSLayoutConstraint?
    private static let bottom: CGFloat = 2
    // Discord's box border sits 9.5 below a line's baseline.
    private static let boxGap: CGFloat = 3.5
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
        let textBottom = text.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom)
        self.textBottom = textBottom
        NSLayoutConstraint.activate([
            textTop,
            textBottom,
            text.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.leading),
            text.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.trailing),
            iconWidth,
            iconHeight,
            icon.centerXAnchor.constraint(equalTo: leadingAnchor, constant: 36),
            icon.centerYAnchor.constraint(equalTo: text.topAnchor, constant: 11),
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// The row height for a notice at `width`, as Auto Layout would size the cell.
    static func height(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, hasBox: Bool = false, width: CGFloat
    ) -> CGFloat {
        sizing.show(line(notice, time: time))
        sizing.setFrameSize(NSSize(width: width - leading - trailing, height: sizing.frame.height))
        return textTop(followsDay: followsDay, followsLine: followsLine)
            + sizing.intrinsicContentSize.height
            + (hasBox ? ThreadBoxView.height(topInset: boxGap) : 0) + bottom
    }

    func configure(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, fullDate: String, reader: UserId? = nil,
        threadBox: (box: ThreadBox, content: NSAttributedString?, time: String?)? = nil
    ) {
        if let threadBox {
            boxView().configure(threadBox.box, content: threadBox.content, time: threadBox.time)
        }
        self.threadBox?.isHidden = threadBox == nil
        // Off before on: both at once can't be satisfied.
        let (off, on) = threadBox == nil ? (textAboveBox, textBottom) : (textBottom, textAboveBox)
        off?.isActive = false
        on?.isActive = true
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
        guard box.contains(convert(event.locationInWindow, from: nil)) else {
            return nil
        }
        return MessageMenu.menu(menuItems())
    }

    private func menuItems() -> [NSMenuItem] {
        message.map { MessageMenu.items(for: $0, createThread: threadAction()) } ?? []
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
        textAboveBox = box.topAnchor.constraint(equalTo: text.bottomAnchor)
        NSLayoutConstraint.activate([
            box.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom),
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
