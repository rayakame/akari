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
    private let content = MessageTextView()
    private let extras = CellText.label(wrapping: true)
    // Built for the first failed message: its buttons cost a layout pass each.
    private var failed: FailedLine?
    var onRetry: () -> Void = {}
    var onDelete: () -> Void = {}
    private let stack = NSStackView()
    private var top: NSLayoutConstraint?
    private var avatarBottom: NSLayoutConstraint?

    private static let groupTop: CGFloat = 18
    private static let continuationTop: CGFloat = 2
    private static let bottomInset: CGFloat = 2
    private static let textLeading: CGFloat = 72
    private static let textTrailing: CGFloat = 16
    private static let spacing: CGFloat = 2
    private static let avatarSize: CGFloat = 40
    private static let avatarLift: CGFloat = 2
    private static let sizing = MessageCell()

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
        configureAllButTheBody(
            message, startsGroup: startsGroup, groupTime: groupTime, shortTime: shortTime,
            fullDate: fullDate)
        content.show(Self.body(message))
    }

    private static func body(_ message: Message) -> NSAttributedString {
        let body = CellText.body(message.content, color: color(message.delivery))
        guard message.editedTimestamp != nil, !message.content.isEmpty else {
            return body
        }
        let marked = NSMutableAttributedString(attributedString: body)
        marked.append(CellText.editedMark())
        return marked
    }

    // The sizing cell measures the body instead of laying it out in its text view.
    private func configureAllButTheBody(
        _ message: Message, startsGroup: Bool, groupTime: String, shortTime: String,
        fullDate: String
    ) {
        showsHeader = startsGroup
        header.isHidden = !startsGroup
        avatar.isHidden = !startsGroup
        avatarBottom?.isActive = startsGroup
        top?.constant = startsGroup ? Self.groupTop : Self.continuationTop
        avatar.name = message.author.displayName
        avatar.userId = message.author.id.rawValue
        header.show(
            name: message.author.displayName, tagged: message.author.bot || message.fromWebhook,
            time: groupTime, fullDate: fullDate)
        hoverTime.stringValue = shortTime
        hoverTime.toolTip = fullDate
        hoverTime.isHidden = !isHovered || startsGroup
        reply.isHidden = !(startsGroup && message.kind == .reply)
        let lines = NSMutableAttributedString(attributedString: Self.extras(message))
        if message.editedTimestamp != nil, message.content.isEmpty {
            lines.append(CellText.editedMark())
        }
        content.isHidden = message.content.isEmpty
        extras.attributedStringValue = lines
        extras.isHidden = lines.length == 0
        let isFailed = message.delivery == .failed
        if isFailed && failed == nil {
            let line = FailedLine()
            line.retry.target = self
            line.retry.action = #selector(retryClicked)
            line.delete.target = self
            line.delete.action = #selector(deleteClicked)
            stack.addView(line, in: .top)
            failed = line
        }
        failed?.isHidden = !isFailed
    }

    private static func color(_ delivery: Delivery) -> NSColor {
        switch delivery {
        case .pending: Palette.textMuted
        case .failed: Palette.danger
        default: Palette.textDefault
        }
    }

    /// The row height for `message` at `width`, as Auto Layout would size the cell.
    static func height(_ message: Message, startsGroup: Bool, width: CGFloat) -> CGFloat {
        sizing.configureAllButTheBody(
            message, startsGroup: startsGroup, groupTime: "", shortTime: "", fullDate: "")
        let textWidth = width - textLeading - textTrailing
        let body = message.content.isEmpty
            ? 0 : TextMeasurer.shared.height(Self.body(message), width: textWidth)
        return sizing.height(width: width, body: body)
    }

    // The constraints of build() in arithmetic; the stack leaves hidden views out.
    private func height(width: CGFloat, body: CGFloat) -> CGFloat {
        let textWidth = width - Self.textLeading - Self.textTrailing
        extras.preferredMaxLayoutWidth = textWidth
        let shown = ([reply, header, content, extras] + [failed].compactMap { $0 }).filter {
            !$0.isHidden
        }
        let stacked =
            shown.map { view in
                view === header
                    ? self.header.height
                    : view === content
                        ? body : view === failed ? FailedLine.height : view.intrinsicContentSize.height
            }
            .reduce(0, +) + Self.spacing * CGFloat(max(shown.count - 1, 0))
        let top = showsHeader ? Self.groupTop : Self.continuationTop
        let height = top + stacked + Self.bottomInset
        guard showsHeader else {
            return height
        }
        let replyHeight = reply.isHidden ? 0 : reply.intrinsicContentSize.height + Self.spacing
        return max(height, top + replyHeight - Self.avatarLift + Self.avatarSize + Self.bottomInset)
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
        extras.isSelectable = true
        // Without it, the field editor drops the attributed string's font on a click.
        extras.allowsEditingTextAttributes = true
        extras.font = .systemFont(ofSize: 16)

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = Self.spacing
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.setViews([reply, header, content, extras], in: .top)
        for view in [avatar, stack, hoverTime] as [NSView] {
            addSubview(view)
        }

        let top = stack.topAnchor.constraint(equalTo: topAnchor, constant: Self.continuationTop)
        let avatarBottom = bottomAnchor.constraint(
            greaterThanOrEqualTo: avatar.bottomAnchor, constant: Self.bottomInset)
        // While the table moves row views in an update, a group start can sit in a shorter row
        // for a moment; required, this would conflict with it and log at length.
        avatarBottom.priority = .init(999)
        self.top = top
        self.avatarBottom = avatarBottom
        NSLayoutConstraint.activate([
            top,
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.textLeading),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.textTrailing),
            stack.bottomAnchor.constraint(equalTo: bottomAnchor, constant: -Self.bottomInset),
            header.widthAnchor.constraint(equalTo: stack.widthAnchor),
            content.widthAnchor.constraint(equalTo: stack.widthAnchor),
            extras.widthAnchor.constraint(equalTo: stack.widthAnchor),
            avatar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            avatar.topAnchor.constraint(equalTo: header.topAnchor, constant: -Self.avatarLift),
            avatar.widthAnchor.constraint(equalToConstant: Self.avatarSize),
            avatar.heightAnchor.constraint(equalToConstant: Self.avatarSize),
            hoverTime.trailingAnchor.constraint(equalTo: leadingAnchor, constant: 56),
            hoverTime.firstBaselineAnchor.constraint(equalTo: content.firstBaselineAnchor),
        ])
    }

    @objc private func retryClicked() {
        onRetry()
    }

    @objc private func deleteClicked() {
        onDelete()
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

    // The name sets the row's height; the tag and the time hang off it.
    var height: CGFloat {
        name.intrinsicContentSize.height
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

private final class FailedLine: NSView {
    static let height: CGFloat = 20
    let retry = FailedLine.button("Retry")
    let delete = FailedLine.button("Delete")

    init() {
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = false
        let label = CellText.label()
        label.stringValue = "Not sent."
        label.font = .systemFont(ofSize: 14)
        label.textColor = Palette.danger
        let row = NSStackView(views: [label, retry, delete])
        row.orientation = .horizontal
        row.spacing = 8
        row.translatesAutoresizingMaskIntoConstraints = false
        addSubview(row)
        NSLayoutConstraint.activate([
            heightAnchor.constraint(equalToConstant: Self.height),
            row.leadingAnchor.constraint(equalTo: leadingAnchor),
            row.centerYAnchor.constraint(equalTo: centerYAnchor),
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    private static func button(_ title: String) -> NSButton {
        let button = NSButton(title: title, target: nil, action: nil)
        button.isBordered = false
        button.attributedTitle = NSAttributedString(
            string: title,
            attributes: [.font: NSFont.systemFont(ofSize: 14), .foregroundColor: Palette.textLink])
        return button
    }
}
