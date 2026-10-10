import AkariKit
import AppKit

final class MessageCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("message")

    private(set) var showsHeader = false
    // Set by the table's controller, which knows the one row under the pointer.
    var isHovered = false {
        didSet {
            guard isHovered != oldValue else {
                return
            }
            needsDisplay = true
            hoverTime.isHidden = !isHovered || showsHeader
        }
    }
    private let avatar = AvatarView()
    private let reply = CellText.label()
    private let header = HeaderRow()
    private let hoverTime = CellText.label()
    private let content = CellText.label(wrapping: true)
    private let extras = CellText.label(wrapping: true)
    private let stack = NSStackView()
    private var top: NSLayoutConstraint?
    private var avatarBottom: NSLayoutConstraint?

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
        header.show(
            name: message.author.displayName, tagged: message.author.bot || message.fromWebhook,
            time: groupTime, fullDate: fullDate)
        hoverTime.stringValue = shortTime
        hoverTime.toolTip = fullDate
        hoverTime.isHidden = !isHovered || startsGroup
        reply.isHidden = !(startsGroup && message.kind == .reply)
        content.attributedStringValue = CellText.body(message.content, color: Palette.textDefault)
        content.isHidden = message.content.isEmpty
        let lines = Self.extras(message)
        extras.attributedStringValue = lines
        extras.isHidden = lines.length == 0
    }

    override func draw(_ dirtyRect: NSRect) {
        if isHovered {
            Palette.hoverBackground.setFill()
            bounds.fill(using: .sourceOver)
        }
    }

    private func build() {
        avatar.translatesAutoresizingMaskIntoConstraints = false
        reply.attributedStringValue = CellText.symbolLine(
            "arrowshape.turn.up.left", "Replying to an earlier message")
        hoverTime.font = .systemFont(ofSize: 11)
        hoverTime.textColor = Palette.chatTextMuted
        hoverTime.alignment = .right
        for field in [content, extras] {
            field.isSelectable = true
            // Without it, the field editor drops the attributed string's font on a click.
            field.allowsEditingTextAttributes = true
            field.font = .systemFont(ofSize: 16)
        }

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
            header.widthAnchor.constraint(equalTo: stack.widthAnchor),
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
        if message.componentsV2 {
            lines.append(
                CellText.symbolLine(
                    "square.grid.2x2", "This message uses a layout Akari can't show yet"))
        }
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

// The tag is centered on the name's line, and the time shares the name's baseline, so a stack
// view with one alignment can't hold all three.
private final class HeaderRow: NSView {
    private let name = CellText.label()
    private let appTag = TagLabel()
    private let time = CellText.label()
    private var timeAfterName: NSLayoutConstraint?
    private var timeAfterTag: NSLayoutConstraint?

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        name.font = .systemFont(ofSize: 16, weight: .semibold)
        name.textColor = Palette.textStrong
        name.lineBreakMode = .byTruncatingTail
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        time.font = .systemFont(ofSize: 12, weight: .medium)
        time.textColor = Palette.chatTextMuted
        appTag.translatesAutoresizingMaskIntoConstraints = false
        for view in [name, appTag, time] as [NSView] {
            addSubview(view)
        }
        let timeAfterName = time.leadingAnchor.constraint(equalTo: name.trailingAnchor, constant: 4)
        let timeAfterTag = time.leadingAnchor.constraint(
            equalTo: appTag.trailingAnchor, constant: 4)
        self.timeAfterName = timeAfterName
        self.timeAfterTag = timeAfterTag
        NSLayoutConstraint.activate([
            name.leadingAnchor.constraint(equalTo: leadingAnchor),
            name.topAnchor.constraint(equalTo: topAnchor),
            name.bottomAnchor.constraint(equalTo: bottomAnchor),
            appTag.leadingAnchor.constraint(equalTo: name.trailingAnchor, constant: 4),
            appTag.centerYAnchor.constraint(equalTo: name.centerYAnchor),
            time.firstBaselineAnchor.constraint(equalTo: name.firstBaselineAnchor),
            time.trailingAnchor.constraint(lessThanOrEqualTo: trailingAnchor),
            timeAfterName,
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    func show(name: String, tagged: Bool, time: String, fullDate: String) {
        self.name.stringValue = name
        appTag.isHidden = !tagged
        timeAfterName?.isActive = !tagged
        timeAfterTag?.isActive = tagged
        self.time.stringValue = time
        self.time.toolTip = fullDate
    }
}

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

    override class var cellClass: AnyClass? {
        get { TagCell.self }
        set {}
    }

    // The pill is the whole frame, so layout gaps measure from its edge.
    override var alignmentRectInsets: NSEdgeInsets {
        NSEdgeInsetsZero
    }
}

// Centers the capitals by their height; the line's ascender and descender would push them up.
private final class TagCell: NSTextFieldCell {
    override func drawInterior(withFrame cellFrame: NSRect, in controlView: NSView) {
        guard let font, let context = NSGraphicsContext.current?.cgContext else {
            return
        }
        let text = NSAttributedString(
            string: stringValue, attributes: [.font: font, .foregroundColor: textColor ?? .white])
        let line = CTLineCreateWithAttributedString(text)
        let width = CTLineGetTypographicBounds(line, nil, nil, nil)
        let flipped = controlView.isFlipped
        context.saveGState()
        context.textMatrix = flipped ? CGAffineTransform(scaleX: 1, y: -1) : .identity
        context.textPosition = CGPoint(
            x: cellFrame.midX - width / 2,
            y: cellFrame.midY + (flipped ? 1 : -1) * font.capHeight / 2)
        CTLineDraw(line, context)
        context.restoreGState()
    }
}
