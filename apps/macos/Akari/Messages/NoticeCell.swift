import AkariKit
import AppKit

final class NoticeCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("notice")

    private let icon = NSImageView()
    private let text = MessageTextView()
    var onLink: (URL) -> Void = { _ in }
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
        NSLayoutConstraint.activate([
            textTop,
            text.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottom),
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
        followsLine: Bool = false, width: CGFloat
    ) -> CGFloat {
        sizing.show(line(notice, time: time))
        sizing.setFrameSize(NSSize(width: width - leading - trailing, height: sizing.frame.height))
        return textTop(followsDay: followsDay, followsLine: followsLine)
            + sizing.intrinsicContentSize.height + bottom
    }

    func configure(
        _ message: Message, notice: [Message.NoticePart], time: String, followsDay: Bool = false,
        followsLine: Bool = false, fullDate: String
    ) {
        textTop?.constant = Self.textTop(followsDay: followsDay, followsLine: followsLine)
        icon.image = NSImage(
            systemSymbolName: Self.symbol(message.kind), accessibilityDescription: nil)
        text.show(Self.line(notice, time: time, fullDate: fullDate))
    }

    // The full date shows over the time only, as Discord's; TextKit keeps its rect in step.
    private static func line(
        _ notice: [Message.NoticePart], time: String, fullDate: String? = nil
    ) -> NSAttributedString {
        // Kinds whose actor Discord draws as a name have their gray too (D121).
        let named = notice.contains { if case .actor = $0 { true } else { false } }
        let gray = named ? Palette.chatTextMuted : Palette.textMuted
        let line = NSMutableAttributedString()
        for part in notice {
            switch part {
            case .text(let text):
                line.append(CellText.body(text, color: gray))
            case .actor(let name), .threads(let name):
                line.append(
                    CellText.body(name, color: Palette.textStrong, style: TypeScale.noticeName))
            case .thread(let name, let id):
                let link = NSMutableAttributedString(
                    attributedString: CellText.body(
                        name, color: Palette.textStrong, style: TypeScale.noticeName))
                if let id, let url = URL(string: "akari://channel/\(id.rawValue)") {
                    link.addAttributes(
                        [.link: url, MessageTextView.underlinesOnHover: true],
                        range: NSRange(location: 0, length: link.length))
                }
                line.append(link)
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
