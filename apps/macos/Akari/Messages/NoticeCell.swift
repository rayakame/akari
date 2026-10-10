import AkariKit
import AppKit

final class NoticeCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("notice")

    private let icon = NSImageView()
    private let text = CellText.label(wrapping: true)

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        icon.translatesAutoresizingMaskIntoConstraints = false
        icon.contentTintColor = Palette.textMuted
        icon.symbolConfiguration = .init(pointSize: 16, weight: .regular)
        addSubview(icon)
        addSubview(text)
        NSLayoutConstraint.activate([
            text.topAnchor.constraint(equalTo: topAnchor, constant: 18),
            text.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2),
            text.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 72),
            text.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            icon.centerXAnchor.constraint(equalTo: leadingAnchor, constant: 36),
            icon.centerYAnchor.constraint(equalTo: text.topAnchor, constant: 11),
        ])
    }

    required init?(coder: NSCoder) {
        nil
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
