import AkariKit
import AppKit

final class MessageCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("message")

    private(set) var showsHeader = false
    // Above a group start's box.
    private var spacing: CGFloat = 0
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
    private let extras = MessageTextView()
    // Built for the first failed message: its buttons cost a layout pass each.
    private var failed: FailedLine?
    var onRetry: () -> Void = {}
    var onDelete: () -> Void = {}
    var onLink: (URL) -> Void = { _ in }
    private var message: Message?
    private var mentioned = false {
        didSet {
            if mentioned != oldValue {
                needsDisplay = true
            }
        }
    }
    private let stack = NSStackView()
    private var top: NSLayoutConstraint?
    private var bottom: NSLayoutConstraint?
    private var avatarBottom: NSLayoutConstraint?

    // The official client keeps the space before a group outside the message's box, so two
    // highlighted groups in a row show a gap; its scrollbar's gutter stays clear too.
    private static let groupSpacing: CGFloat = 16
    // A day's first message: the divider's row brings the rest, its box starts 17.5 below the line.
    private static let daySpacing: CGFloat = 6
    // Measured there too: a group start's avatar and name 4 into its box, a follow-up's text 3.
    // What's below them is what's left of the box, so rows keep their heights.
    private static let groupInset: CGFloat = 4
    private static let groupBottom: CGFloat = 0
    private static let continuationTop: CGFloat = 3
    private static let continuationBottom: CGFloat = 1
    private static let avatarBottomInset: CGFloat = 3
    private static let scrollbarGutter: CGFloat = 16
    private static let textLeading: CGFloat = 72
    private static let textTrailing: CGFloat = 16
    private static let spacing: CGFloat = 2
    private static let avatarSize: CGFloat = 40
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
        _ message: Message, body: NSAttributedString, startsGroup: Bool, followsDay: Bool = false,
        followsLine: Bool = false, groupTime: String, shortTime: String, fullDate: String
    ) {
        configureAllButTheBody(
            message, startsGroup: startsGroup, followsDay: followsDay, followsLine: followsLine,
            groupTime: groupTime, shortTime: shortTime, fullDate: fullDate)
        let same = self.message?.id == message.id
        self.message = message
        mentioned = message.mentionsMe
        content.show(body, keepingSelection: same)
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        guard let message else {
            return nil
        }
        let menu = NSMenu()
        for item in MessageMenu.items(for: message) {
            menu.addItem(item)
        }
        return menu
    }

    // The sizing cell measures the body instead of laying it out in its text view.
    private func configureAllButTheBody(
        _ message: Message, startsGroup: Bool, followsDay: Bool, followsLine: Bool,
        groupTime: String, shortTime: String, fullDate: String
    ) {
        showsHeader = startsGroup
        spacing = Self.spacing(
            startsGroup: startsGroup, followsDay: followsDay, followsLine: followsLine)
        header.isHidden = !startsGroup
        avatar.isHidden = !startsGroup
        avatarBottom?.isActive = startsGroup
        top?.constant = startsGroup ? spacing + Self.groupInset : Self.continuationTop
        bottom?.constant = -(startsGroup ? Self.groupBottom : Self.continuationBottom)
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
        extras.show(lines)
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

    /// The space above the box: before a group, less under a day divider, none under the new
    /// messages line, whose row is that space.
    static func spacing(startsGroup: Bool, followsDay: Bool, followsLine: Bool) -> CGFloat {
        guard startsGroup, !followsLine else {
            return 0
        }
        return followsDay ? daySpacing : groupSpacing
    }

    /// The row height for `message` showing `body` at `width`, as Auto Layout would size the
    /// cell.
    static func height(
        _ message: Message, body: NSAttributedString, startsGroup: Bool, followsDay: Bool = false,
        followsLine: Bool = false, width: CGFloat
    ) -> CGFloat {
        sizing.configureAllButTheBody(
            message, startsGroup: startsGroup, followsDay: followsDay, followsLine: followsLine,
            groupTime: "", shortTime: "", fullDate: "")
        let textWidth = width - textLeading - textTrailing
        let body = message.content.isEmpty ? 0 : TextMeasurer.shared.height(body, width: textWidth)
        return sizing.height(width: width, body: body)
    }

    // The constraints of build() in arithmetic; the stack leaves hidden views out.
    private func height(width: CGFloat, body: CGFloat) -> CGFloat {
        let textWidth = width - Self.textLeading - Self.textTrailing
        let shown = ([reply, header, content, extras] + [failed].compactMap { $0 }).filter {
            !$0.isHidden
        }
        let stacked =
            shown.map { view in
                view === header
                    ? self.header.height
                    : view === content
                        ? body
                        : view === extras
                            ? TextMeasurer.shared.height(extras.attributedString(), width: textWidth)
                            : view === failed
                                ? FailedLine.height : view.intrinsicContentSize.height
            }
            .reduce(0, +) + Self.spacing * CGFloat(max(shown.count - 1, 0))
        let top = showsHeader ? spacing + Self.groupInset : Self.continuationTop
        let height = top + stacked + (showsHeader ? Self.groupBottom : Self.continuationBottom)
        guard showsHeader else {
            return height
        }
        let replyHeight = reply.isHidden ? 0 : reply.intrinsicContentSize.height + Self.spacing
        return max(height, top + replyHeight + Self.avatarSize + Self.avatarBottomInset)
    }

    /// The message's own area, which hover and the mention highlight cover.
    var box: NSRect {
        // A legacy scroller sits in a gutter of its own beside the list, while it shows.
        let legacy = enclosingScrollView.map {
            $0.scrollerStyle == .legacy && $0.verticalScroller?.isHidden == false
        }
        let gutter = legacy == true ? 0 : Self.scrollbarGutter
        return NSRect(
            x: 0, y: isFlipped ? spacing : 0, width: max(0, bounds.width - gutter),
            height: max(0, bounds.height - spacing))
    }

    override func draw(_ dirtyRect: NSRect) {
        let box = box
        if mentioned {
            Palette.mentionedBackground.setFill()
            box.fill(using: .sourceOver)
            Palette.mentionedBar.setFill()
            NSRect(x: box.minX, y: box.minY, width: 2, height: box.height).fill()
        }
        if isHovered {
            Palette.hoverBackground.setFill()
            box.fill(using: .sourceOver)
        }
    }

    private func build() {
        content.onLink = { [weak self] url in self?.onLink(url) }
        content.messageMenu = { [weak self] in
            self?.message.map { MessageMenu.items(for: $0) } ?? []
        }
        avatar.translatesAutoresizingMaskIntoConstraints = false
        reply.attributedStringValue = CellText.symbolLine(
            "arrowshape.turn.up.left", "Replying to an earlier message")
        hoverTime.font = TypeScale.hoverTime.nsFont
        hoverTime.textColor = Palette.chatTextMuted
        hoverTime.alignment = .right
        extras.messageMenu = content.messageMenu

        stack.orientation = .vertical
        stack.alignment = .leading
        stack.spacing = Self.spacing
        stack.translatesAutoresizingMaskIntoConstraints = false
        stack.setViews([reply, header, content, extras], in: .top)
        for view in [avatar, stack, hoverTime] as [NSView] {
            addSubview(view)
        }

        let top = stack.topAnchor.constraint(equalTo: topAnchor, constant: Self.continuationTop)
        let bottom = stack.bottomAnchor.constraint(
            equalTo: bottomAnchor, constant: -Self.continuationBottom)
        let avatarBottom = bottomAnchor.constraint(
            greaterThanOrEqualTo: avatar.bottomAnchor, constant: Self.avatarBottomInset)
        // While the table moves row views in an update, a group start can sit in a shorter row
        // for a moment; required, this would conflict with it and log at length.
        avatarBottom.priority = .init(999)
        self.top = top
        self.bottom = bottom
        self.avatarBottom = avatarBottom
        NSLayoutConstraint.activate([
            top, bottom,
            stack.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.textLeading),
            stack.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.textTrailing),
            header.widthAnchor.constraint(equalTo: stack.widthAnchor),
            content.widthAnchor.constraint(equalTo: stack.widthAnchor),
            extras.widthAnchor.constraint(equalTo: stack.widthAnchor),
            avatar.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 16),
            avatar.topAnchor.constraint(equalTo: header.topAnchor),
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
        name.font = TypeScale.username.nsFont
        name.textColor = Palette.textStrong
        name.lineBreakMode = .byTruncatingTail
        name.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        time.font = TypeScale.messageTime.nsFont
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
        // Off before on: both at once can't be satisfied.
        let (off, on) = tagged ? (timeAfterName, timeAfterTag) : (timeAfterTag, timeAfterName)
        off?.isActive = false
        on?.isActive = true
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
        font = TypeScale.appTag.nsFont
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
        label.font = TypeScale.detail.nsFont
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
            attributes: [.font: TypeScale.detail.nsFont, .foregroundColor: Palette.textLink])
        return button
    }
}
