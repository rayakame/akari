import AkariKit
import AppKit

/// One message: with the author's avatar, name and time when it starts a group, else just the
/// content with the time in the gutter on hover (docs/ui/message-list.md).
final class MessageCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("message")

    private(set) var showsHeader = false
    private let avatar = AvatarView()
    private let reply = CellText.label()
    private let name = CellText.label()
    private let appTag = TagLabel()
    private let time = CellText.label()
    private let hoverTime = CellText.label()
    private let content = CellText.label(wrapping: true)
    private let extras = CellText.label(wrapping: true)
    private let header = NSStackView()
    private let stack = NSStackView()
    private var top: NSLayoutConstraint?
    private var avatarBottom: NSLayoutConstraint?
    private var hovered = false {
        didSet {
            needsDisplay = true
            hoverTime.isHidden = !hovered || showsHeader
        }
    }

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        build()
    }

    required init?(coder: NSCoder) {
        nil
    }

    func configure(
        _ message: Message, startsGroup: Bool, groupTime: String, shortTime: String,
        fullDate: String
    ) {
        showsHeader = startsGroup
        header.isHidden = !startsGroup
        avatar.isHidden = !startsGroup
        avatarBottom?.isActive = startsGroup
        top?.constant = startsGroup ? 18 : 2
        avatar.name = message.author.displayName
        avatar.userId = message.author.id.rawValue
        name.stringValue = message.author.displayName
        appTag.isHidden = !(message.author.bot || message.fromWebhook)
        time.stringValue = groupTime
        time.toolTip = fullDate
        hoverTime.stringValue = shortTime
        hoverTime.toolTip = fullDate
        hoverTime.isHidden = true
        reply.isHidden = !(startsGroup && message.kind == .reply)
        content.attributedStringValue = CellText.body(message.content, color: Palette.textDefault)
        content.isHidden = message.content.isEmpty
        let lines = Self.extras(message)
        extras.attributedStringValue = lines
        extras.isHidden = lines.length == 0
        hovered = false
    }

    override func prepareForReuse() {
        super.prepareForReuse()
        hovered = false
    }

    override func draw(_ dirtyRect: NSRect) {
        if hovered {
            Palette.hoverBackground.setFill()
            bounds.fill(using: .sourceOver)
        }
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        trackingAreas.forEach(removeTrackingArea)
        addTrackingArea(
            NSTrackingArea(
                rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                owner: self))
    }

    override func mouseEntered(with event: NSEvent) {
        hovered = true
    }

    override func mouseExited(with event: NSEvent) {
        hovered = false
    }

    private func build() {
        avatar.translatesAutoresizingMaskIntoConstraints = false
        reply.attributedStringValue = CellText.symbolLine(
            "arrowshape.turn.up.left", "Replying to an earlier message")
        name.font = .systemFont(ofSize: 16, weight: .semibold)
        name.textColor = Palette.textStrong
        name.lineBreakMode = .byTruncatingTail
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        time.font = .systemFont(ofSize: 12, weight: .medium)
        time.textColor = Palette.chatTextMuted
        hoverTime.font = .systemFont(ofSize: 11)
        hoverTime.textColor = Palette.chatTextMuted
        hoverTime.alignment = .right
        content.isSelectable = true
        extras.isSelectable = true

        header.orientation = .horizontal
        header.alignment = .firstBaseline
        header.spacing = 4
        header.setViews([name, appTag, time], in: .leading)
        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = 2
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.setViews([reply, header, content, extras], in: .top)
        for view in [avatar, stack, hoverTime] as [NSView] {
            addSubview(view)
        }

        let top = stack.topAnchor.constraint(equalTo: topAnchor, constant: 2)
        let avatarBottom = bottomAnchor.constraint(
            greaterThanOrEqualTo: avatar.bottomAnchor, constant: 2)
        self.top = top
        self.avatarBottom = avatarBottom
        NSLayoutConstraint.activate([
            top,
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 72),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -16),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -2),
            content.widthAnchor.constraint(equalTo: stack.widthAnchor),
            extras.widthAnchor.constraint(equalTo: stack.widthAnchor),
            avatar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            avatar.topAnchor.constraint(equalTo: header.topAnchor, constant: -2),
            avatar.widthAnchor.constraint(equalToConstant: 40),
            avatar.heightAnchor.constraint(equalToConstant: 40),
            hoverTime.trailingAnchor.constraint(equalTo: leadingAnchor, constant: 56),
            hoverTime.firstBaselineAnchor.constraint(equalTo: content.firstBaselineAnchor),
        ])
    }

    private static func extras(_ message: Message) -> NSAttributedString {
        var lines: [NSAttributedString] = []
        for attachment in message.attachments {
            let size = ByteCountFormatter.string(
                fromByteCount: Int64(attachment.size), countStyle: .file)
            lines.append(CellText.symbolLine("paperclip", "\(attachment.filename) (\(size))"))
        }
        if message.embedCount > 0 {
            let embeds = message.embedCount == 1 ? "1 embed" : "\(message.embedCount) embeds"
            lines.append(CellText.symbolLine("rectangle.on.rectangle", embeds))
        }
        for sticker in message.stickerNames {
            lines.append(CellText.symbolLine("seal", "Sticker: \(sticker)"))
        }
        let joined = NSMutableAttributedString()
        for (index, line) in lines.enumerated() {
            if index > 0 {
                joined.append(NSAttributedString(string: "\n"))
            }
            joined.append(line)
        }
        return joined
    }
}

/// The small tag after a bot's or webhook's name.
private final class TagLabel: NSTextField {
    init() {
        super.init(frame: .zero)
        stringValue = "APP"
        isEditable = false
        isBordered = false
        drawsBackground = false
        font = .systemFont(ofSize: 10, weight: .semibold)
        textColor = .white
        alignment = .center
        wantsLayer = true
        layer?.backgroundColor = Palette.brand.cgColor
        layer?.cornerRadius = 3
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        let size = super.intrinsicContentSize
        return NSSize(width: size.width + 8, height: size.height + 2)
    }
}
