import AkariKit
import AppKit

final class NoticeCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("notice")

    private let icon = NSImageView()
    private let text = MessageTextView()
    // Built for the first line with a box: buttons cost.
    private var threadBox: ThreadBoxView?
    var onLink: (URL) -> Void = { _ in }
    var onOpenThread: () -> Void = {}
    private var textBottom: NSLayoutConstraint?
    private var textAboveBox: NSLayoutConstraint?
    private static let top: CGFloat = 18
    // 10 less right after a day divider, as a message's (MessageCell).
    private static let dayTop: CGFloat = 8
    // Under the new messages line, whose row is the 16 above a group.
    private static let lineTop: CGFloat = 2
    private var textTop: NSLayoutConstraint?
    private static let bottom: CGFloat = 2
    private static let leading: CGFloat = 72
    private static let trailing: CGFloat = 16
    // Measures as the cell's own text view lays out; string drawing breaks the line differently.
    private static let sizing = MessageTextView()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        text.onLink = { [weak self] url in self?.onLink(url) }
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.contentTintColor = Palette.textMuted
        icon.symbolConfiguration = .init(pointSize: 16, weight: .regular)
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
            + sizing.intrinsicContentSize.height + (hasBox ? ThreadBoxView.height : 0) + bottom
    }

    func configure(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, fullDate: String,
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
        icon.image = NSImage(
            systemSymbolName: Self.symbol(message.kind), accessibilityDescription: nil)
        text.show(Self.line(notice, time: time, fullDate: fullDate))
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
            case .actor(let name), .strong(let name), .threads(let name), .pins(let name):
                line.append(Self.name(name, link: nil))
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
        // Under a system line Discord's box has no gap above it.
        textAboveBox = text.bottomAnchor.constraint(equalTo: box.topAnchor)
        NSLayoutConstraint.activate([
            box.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom),
            box.leadingAnchor.constraint(equalTo: text.leadingAnchor),
            box.trailingAnchor.constraint(equalTo: text.trailingAnchor),
        ])
        threadBox = box
        return box
    }

    static func textTop(followsDay: Bool, followsLine: Bool) -> CGFloat {
        followsLine ? lineTop : followsDay ? dayTop : top
    }

    static func symbol(_ kind: MessageType) -> String {
        switch kind {
        case .userJoin: "arrow.right"
        case .channelPinnedMessage: "pin"
        case .premiumGuildSubscription, .premiumGuildSubscriptionTier1,
            .premiumGuildSubscriptionTier2, .premiumGuildSubscriptionTier3:
            "sparkles"
        case .call: "phone"
        case .channelNameChange, .channelIconChange: "pencil"
        case .threadCreated, .threadStarterMessage: "text.bubble"
        default: "info.circle"
        }
    }
}
