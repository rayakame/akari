import AkariKit
import AppKit

// Under a forum post's first message: Follow and Copy Link at the right of a top border, then
// the divider before the replies or, without replies, the invitation to answer.
final class PostBarCell: NSTableCellView {
    static let identifier = NSUserInterfaceItemIdentifier("postBar")

    private static let side: CGFloat = 16
    private static let top: CGFloat = 8
    private static let inside: CGFloat = 6
    private static let buttonInset: CGFloat = 8
    private static let gap: CGFloat = 4
    private static let barHeight = top + 1 + inside + PostBarButton.height + inside
    private static let dividerHeight: CGFloat = 8
    private static let dividerBelow: CGFloat = 16
    private static let inviteMargin: CGFloat = 16
    private static let inviteHeight: CGFloat = 64
    private static let inviteWidth: CGFloat = 448
    private static let invitePadding: CGFloat = 12
    private static let inviteIcon: CGFloat = 40
    private static let tailStart: CGFloat = 100
    private static let tailHeight: CGFloat = 10
    private static let copiedFor: Duration = .seconds(2)

    enum Under: Equatable {
        case divider, invite, nothing
    }

    var onFollow: ((_ follows: Bool) -> Void)?
    var onCopy: (() -> Void)?
    // Injectable, so tests don't wait two seconds.
    var wait: (Duration) async throws -> Void = { try await Task.sleep(for: $0) }

    let follow = PostBarButton()
    let copy = PostBarButton()
    private let inviteSymbol = NSImageView()
    private let inviteTitle = NSTextField(labelWithString: PostBar.inviteTitle)
    private let inviteText = NSTextField(labelWithString: PostBar.inviteText)
    private(set) var under = Under.nothing
    private var bar: PostBar?
    private var copied: Task<Void, Never>?

    init() {
        super.init(frame: .zero)
        identifier = Self.identifier
        follow.target = self
        follow.action = #selector(followClicked)
        follow.toolTip = PostBar.followTip
        copy.target = self
        copy.action = #selector(copyClicked)
        inviteSymbol.image = NSImage(
            systemSymbolName: "bubble.left.and.text.bubble.right.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .regular))
        inviteSymbol.contentTintColor = Palette.textMuted
        inviteTitle.font = TypeScale.postInviteTitle.nsFont
        inviteTitle.textColor = Palette.textStrong
        inviteText.font = TypeScale.postInviteText.nsFont
        inviteText.textColor = Palette.textDefault
        for view in [follow, copy, inviteSymbol, inviteTitle, inviteText] as [NSView] {
            addSubview(view)
        }
        showCopy(copied: false)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    static func under(_ bar: PostBar, hasReplies: Bool) -> Under {
        hasReplies ? .divider : bar.invites ? .invite : .nothing
    }

    static func height(_ bar: PostBar, hasReplies: Bool) -> CGFloat {
        switch under(bar, hasReplies: hasReplies) {
        case .divider: barHeight + dividerHeight + dividerBelow
        case .invite: barHeight + 2 * inviteMargin + inviteHeight
        case .nothing: barHeight
        }
    }

    func show(_ bar: PostBar, hasReplies: Bool) {
        if bar.post != self.bar?.post {
            copied?.cancel()
            showCopy(copied: false)
        }
        self.bar = bar
        under = Self.under(bar, hasReplies: hasReplies)
        follow.show(
            symbol: bar.following ? "checkmark" : "bell.fill",
            title: bar.following ? PostBar.following : PostBar.follow)
        follow.isEnabled = bar.canFollow
        for view in [inviteSymbol, inviteTitle, inviteText] as [NSView] {
            view.isHidden = under != .invite
        }
        needsLayout = true
        needsDisplay = true
    }

    private func showCopy(copied: Bool) {
        copy.show(symbol: copied ? "checkmark" : "link", title: copied ? PostBar.copied : nil)
        copy.toolTip = copied ? nil : PostBar.copyLink
        copy.setAccessibilityLabel(copied ? PostBar.copied : PostBar.copyLink)
        needsLayout = true
    }

    @objc private func followClicked() {
        guard let bar else {
            return
        }
        onFollow?(!bar.following)
    }

    @objc private func copyClicked() {
        onCopy?()
        showCopy(copied: true)
        copied?.cancel()
        copied = Task { [weak self, wait] in
            try? await wait(Self.copiedFor)
            guard !Task.isCancelled else {
                return
            }
            self?.showCopy(copied: false)
        }
    }

    override func layout() {
        super.layout()
        let y = Self.top + 1 + Self.inside
        var x = bounds.width - Self.side - Self.buttonInset
        for button in [copy, follow] {
            let width = button.fittingWidth
            x -= width
            button.frame = NSRect(x: x, y: y, width: width, height: PostBarButton.height)
            x -= Self.gap
        }
        let box = inviteBox
        inviteSymbol.frame = NSRect(
            x: box.minX + Self.invitePadding, y: box.minY + Self.invitePadding,
            width: Self.inviteIcon, height: Self.inviteIcon)
        let textX = inviteSymbol.frame.maxX + 16
        let textWidth = max(0, box.maxX - Self.invitePadding - textX)
        let title = inviteTitle.intrinsicContentSize.height
        let text = inviteText.intrinsicContentSize.height
        let textY = box.midY - (title + text) / 2
        inviteTitle.frame = NSRect(x: textX, y: textY, width: textWidth, height: title)
        inviteText.frame = NSRect(x: textX, y: textY + title, width: textWidth, height: text)
    }

    var inviteBox: NSRect {
        let x = Self.side + Self.inviteMargin
        return NSRect(
            x: x, y: Self.barHeight + Self.inviteMargin,
            width: min(Self.inviteWidth, max(0, bounds.width - 2 * x)), height: Self.inviteHeight)
    }

    override func draw(_ dirtyRect: NSRect) {
        Palette.borderSubtle.setFill()
        let width = max(0, bounds.width - 2 * Self.side)
        NSRect(x: Self.side, y: Self.top, width: width, height: 1).fill(using: .sourceOver)
        switch under {
        case .divider:
            NSRect(x: Self.side, y: Self.barHeight, width: width, height: Self.dividerHeight)
                .fill(using: .sourceOver)
        case .invite:
            Palette.backgroundModNormal.setFill()
            let box = inviteBox
            // One path: the color is translucent, a tail drawn apart would show its seam.
            let bubble = NSBezierPath(roundedRect: box, xRadius: 12, yRadius: 12)
            let tail = box.minX + Self.tailStart
            bubble.move(to: NSPoint(x: tail, y: box.maxY))
            bubble.line(to: NSPoint(x: tail + Self.tailHeight, y: box.maxY + Self.tailHeight))
            bubble.line(to: NSPoint(x: tail + 2 * Self.tailHeight, y: box.maxY))
            bubble.close()
            bubble.fill()
        case .nothing:
            break
        }
    }
}

/// A small secondary button of the post's bar: a symbol, and a title or none.
final class PostBarButton: NSButton {
    static let height: CGFloat = 32
    private static let padding: CGFloat = 12

    private(set) var symbol = ""
    /// The title as worded; the drawn one starts with the gap after the symbol.
    private(set) var label = ""

    init() {
        super.init(frame: .zero)
        isBordered = false
        refusesFirstResponder = true
        wantsLayer = true
        layer?.cornerRadius = 8
        imagePosition = .imageLeading
        imageScaling = .scaleNone
        imageHugsTitle = true
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isEnabled: Bool {
        didSet { alphaValue = isEnabled ? 1 : 0.5 }
    }

    func show(symbol: String, title: String?) {
        self.symbol = symbol
        label = title ?? ""
        image = NSImage(systemSymbolName: symbol, accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 13, weight: .semibold))
        contentTintColor = Palette.textDefault
        imagePosition = title == nil ? .imageOnly : .imageLeading
        attributedTitle = NSAttributedString(
            string: title.map { " " + $0 } ?? "",
            attributes: [
                .font: TypeScale.postBarButton.nsFont, .foregroundColor: Palette.textDefault,
            ])
        if let title {
            setAccessibilityLabel(title)
        }
    }

    var fittingWidth: CGFloat {
        guard !attributedTitle.string.isEmpty else {
            return Self.height
        }
        let text = attributedTitle.size().width.rounded(.up)
        return 2 * Self.padding + (image?.size.width ?? 0).rounded(.up) + text
    }

    override func updateLayer() {
        layer?.backgroundColor = Palette.controlSecondary.cgColor
    }

    override var wantsUpdateLayer: Bool { true }
}
