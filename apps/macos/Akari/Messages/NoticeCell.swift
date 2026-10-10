import AkariKit
import AppKit

final class NoticeCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("notice")

    private let icon = NSImageView()
    private let text = CellText.label(wrapping: true)
    private static let top: CGFloat = 18
    private static let bottom: CGFloat = 2
    private static let leading: CGFloat = 72
    private static let trailing: CGFloat = 16
    private static let sizing = NoticeCell()

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.contentTintColor = Palette.textMuted
        icon.symbolConfiguration = .init(pointSize: 16, weight: .regular)
        addSubview(icon)
        addSubview(text)
        NSLayoutConstraint.activate([
            text.topAnchor.constraint(equalTo: topAnchor, constant: Self.top),
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
    static func height(_ message: Message, notice: String, time: String, width: CGFloat)
        -> CGFloat
    {
        sizing.configure(message, notice: notice, time: time, fullDate: "")
        sizing.text.preferredMaxLayoutWidth = width - leading - trailing
        return top + sizing.text.intrinsicContentSize.height + bottom
    }

    func configure(_ message: Message, notice: String, time: String, fullDate: String) {
        icon.image = NSImage(
            systemSymbolName: Self.symbol(message.kind), accessibilityDescription: nil)
        let line = NSMutableAttributedString(
            attributedString: CellText.body(notice, color: Palette.textMuted))
        line.append(
            NSAttributedString(
                string: "  \(time)",
                attributes: [
                    .font: NSFont.systemFont(ofSize: 12), .foregroundColor: Palette.chatTextMuted,
                ]))
        text.attributedStringValue = line
        text.toolTip = fullDate
    }

    private static func symbol(_ kind: MessageType) -> String {
        switch kind {
        case .userJoin: "arrow.right"
        case .channelPinnedMessage: "pin"
        case .premiumGuildSubscription, .premiumGuildSubscriptionTier1,
            .premiumGuildSubscriptionTier2, .premiumGuildSubscriptionTier3:
            "sparkles"
        case .call: "phone"
        case .channelNameChange, .channelIconChange: "pencil"
        case .threadCreated: "text.bubble"
        default: "info.circle"
        }
    }
}
