import AppKit

// Above a channel's first message, as the official client welcomes it; its edit button waits for
// channel settings.
final class ChannelStartCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("start")

    // Measured from the official client, apart from the DM's avatar and username.
    private static let top: CGFloat = 16
    private static let side: CGFloat = 16
    private static let iconSize: CGFloat = 68
    private static let avatarSize: CGFloat = 80
    private static let titleBaseline: CGFloat = 39
    private static let usernameBaseline: CGFloat = 28
    private static let subtitleBaseline: CGFloat = 33.5
    private static let bottom: CGFloat = 6
    private static let sizing = ChannelStartCell()

    let icon = StartIcon()
    private let title = MessageTextView()
    private let username = MessageTextView()
    private let subtitle = MessageTextView()
    private let iconWidth: NSLayoutConstraint
    private let iconHeight: NSLayoutConstraint
    private let subtitleAfterTitle: NSLayoutConstraint
    private let subtitleAfterUsername: NSLayoutConstraint

    init() {
        iconWidth = icon.widthAnchor.constraint(equalToConstant: Self.iconSize)
        iconHeight = icon.heightAnchor.constraint(equalToConstant: Self.iconSize)
        subtitleAfterTitle = subtitle.firstBaselineAnchor.constraint(
            equalTo: title.lastBaselineAnchor, constant: Self.subtitleBaseline)
        subtitleAfterUsername = subtitle.firstBaselineAnchor.constraint(
            equalTo: username.lastBaselineAnchor, constant: Self.subtitleBaseline)
        super.init(frame: .zero)
        identifier = Self.identifier
        icon.translatesAutoresizingMaskIntoConstraints = false
        for view in [icon, title, username, subtitle] as [NSView] {
            addSubview(view)
        }
        // The table sizes the row; a required bottom would conflict while it moves row views.
        let bottom = bottomAnchor.constraint(
            equalTo: subtitle.lastBaselineAnchor, constant: Self.bottom)
        bottom.priority = .init(999)
        NSLayoutConstraint.activate([
            icon.topAnchor.constraint(equalTo: topAnchor, constant: Self.top),
            icon.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.side),
            iconWidth, iconHeight,
            title.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.side),
            title.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.side),
            title.firstBaselineAnchor.constraint(
                equalTo: icon.bottomAnchor, constant: Self.titleBaseline),
            username.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.side),
            username.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.side),
            username.firstBaselineAnchor.constraint(
                equalTo: title.lastBaselineAnchor, constant: Self.usernameBaseline),
            subtitle.leadingAnchor.constraint(equalTo: leadingAnchor, constant: Self.side),
            subtitle.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -Self.side),
            subtitleAfterTitle, bottom,
        ])
    }

    required init?(coder: NSCoder) {
        nil
    }

    /// The row height for `start` at `width`, as Auto Layout would size the cell; a channel
    /// whose name isn't known yet gets a row too thin to see.
    static func height(_ start: ChannelStart?, width: CGFloat) -> CGFloat {
        guard let start else {
            return 1
        }
        sizing.show(start)
        let textWidth = max(1, width - 2 * side)
        // From the text's first baseline to its last.
        func lines(_ text: MessageTextView) -> CGFloat {
            text.setFrameSize(NSSize(width: textWidth, height: text.frame.height))
            return text.intrinsicContentSize.height - text.firstBaselineOffsetFromTop
                - text.lastBaselineOffsetFromBottom
        }
        var height = top + sizing.iconHeight.constant + titleBaseline + lines(sizing.title)
        if start.username != nil {
            height += usernameBaseline + lines(sizing.username)
        }
        // Auto Layout gives the cell whole points.
        return (height + subtitleBaseline + lines(sizing.subtitle) + bottom).rounded(.up)
    }

    func show(_ start: ChannelStart?) {
        isHidden = start == nil
        guard let start else {
            return
        }
        icon.icon = start.icon
        let size = if case .avatar = start.icon { Self.avatarSize } else { Self.iconSize }
        iconWidth.constant = size
        iconHeight.constant = size
        title.show(CellText.plain(start.title, style: TypeScale.startTitle, color: Palette.textStrong))
        username.show(
            CellText.plain(
                start.username ?? "", style: TypeScale.startUsername, color: Palette.textStrong))
        username.isHidden = start.username == nil
        // Off before on: both at once can't be satisfied.
        let (off, on) =
            start.username == nil
            ? (subtitleAfterUsername, subtitleAfterTitle)
            : (subtitleAfterTitle, subtitleAfterUsername)
        off.isActive = false
        on.isActive = true
        subtitle.show(
            CellText.plain(
                start.subtitle, style: TypeScale.startSubtitle, color: Palette.textDefault))
    }
}

// The channel's symbol on a neutral circle, or the other person's avatar in a DM.
final class StartIcon: NSView {
    var icon = ChannelStart.Icon.symbol("number") {
        didSet {
            if icon != oldValue {
                needsDisplay = true
            }
        }
    }
    private static let symbolSize: CGFloat = 36

    override func draw(_ dirtyRect: NSRect) {
        switch icon {
        case .avatar(let name, let id):
            AvatarView.draw(name: name, userId: id, in: bounds)
        case .symbol(let name):
            Palette.startIcon.setFill()
            NSBezierPath(ovalIn: bounds).fill()
            let configuration = NSImage.SymbolConfiguration(
                pointSize: Self.symbolSize, weight: .medium
            )
            .applying(NSImage.SymbolConfiguration(paletteColors: [Palette.textStrong]))
            guard
                let image = NSImage(systemSymbolName: name, accessibilityDescription: nil)?
                    .withSymbolConfiguration(configuration)
            else {
                return
            }
            let size = image.size
            image.draw(
                in: NSRect(
                    x: bounds.midX - size.width / 2, y: bounds.midY - size.height / 2,
                    width: size.width, height: size.height))
        }
    }
}
