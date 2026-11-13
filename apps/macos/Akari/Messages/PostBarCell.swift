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
    /// A click on a pill or on the forum's default reaction.
    var onReaction: (ReactionEmoji) -> Void = { _ in }
    /// "React to Post", with the button.
    var onAddReaction: (NSView) -> Void = { _ in }
    /// "+N": all of the post's reactions.
    var onMoreReactions: () -> Void = {}
    var reactionTooltip: (ReactionPill) -> String? = { _ in nil }
    var reactionsRoll: () -> Bool = { false }
    var images: ImageCache? {
        get { reactions.images }
        set { reactions.images = newValue }
    }
    // Injectable, so tests don't wait two seconds.
    var wait: (Duration) async throws -> Void = { try await Task.sleep(for: $0) }

    let follow = PostBarButton()
    let copy = PostBarButton()
    /// The first message's reactions at the bar's left, as many as fit.
    let reactions = ReactionRowView()
    /// "React to Post" while the post has no reactions, the symbol alone after.
    let react = PostBarButton()
    // The official client's button is 24 high in a bar of 32.
    private static let reactHeight: CGFloat = 24
    private static let reactGap: CGFloat = 4
    private var pillsShown = 0
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
        react.target = self
        react.action = #selector(reactClicked)
        reactions.translatesAutoresizingMaskIntoConstraints = true
        reactions.onPill = { [weak self] pill in self?.onReaction(pill.emoji) }
        reactions.onOffer = { [weak self] emoji in self?.onReaction(emoji) }
        reactions.onMore = { [weak self] in self?.onMoreReactions() }
        reactions.tooltip = { [weak self] pill in self?.reactionTooltip(pill) }
        reactions.rolls = { [weak self] in self?.reactionsRoll() ?? false }
        inviteSymbol.image = NSImage(
            systemSymbolName: "bubble.left.and.text.bubble.right.fill", accessibilityDescription: nil)?
            .withSymbolConfiguration(.init(pointSize: 26, weight: .regular))
        inviteSymbol.contentTintColor = Palette.textMuted
        inviteTitle.font = TypeScale.postInviteTitle.nsFont
        inviteTitle.textColor = Palette.textStrong
        inviteText.font = TypeScale.postInviteText.nsFont
        inviteText.textColor = Palette.textDefault
        for view in [reactions, react, follow, copy, inviteSymbol, inviteTitle, inviteText] as [NSView] {
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
        let samePost = bar.post == self.bar?.post
        self.bar = bar
        showReactions(animated: samePost)
        react.isHidden = !bar.reacts
        react.show(
            symbol: "face.smiling", title: bar.reactions.isEmpty ? ReactionWords.reactToPost : nil)
        react.toolTip = bar.reactions.isEmpty ? nil : ReactionWords.add
        react.setAccessibilityLabel(
            bar.reactions.isEmpty ? ReactionWords.reactToPost : ReactionWords.add)
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

    // As many pills as the official client's bar shows at this width, the rest behind "+N".
    private func showReactions(animated: Bool) {
        guard let bar else {
            return
        }
        pillsShown = min(bar.reactions.count, PostBar.pillsShown(width: Double(bounds.width)))
        let shown = Array(bar.reactions.prefix(pillsShown))
        reactions.configure(
            shown.isEmpty ? nil : ReactionRow(pills: shown, adds: false, toggles: bar.toggles),
            offer: bar.reactions.isEmpty ? bar.defaultReaction : nil,
            more: bar.reactions.count - shown.count, animated: animated)
    }

    @objc private func reactClicked() {
        onAddReaction(react)
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
        if let bar, pillsShown != min(bar.reactions.count, PostBar.pillsShown(width: Double(bounds.width))) {
            showReactions(animated: false)
        }
        let left = Self.side + Self.buttonInset
        let pills = reactions.contentWidth
        reactions.frame = NSRect(
            x: left,
            y: y + (PostBarButton.height - ReactionRowView.pillHeight) / 2 - ReactionRowView.above,
            width: pills + 1, height: ReactionRowView.above + ReactionRowView.pillHeight)
        let width = react.attributedTitle.string.isEmpty ? PostBarButton.height : react.fittingWidth
        react.frame = NSRect(
            x: left + pills + (pills > 0 ? Self.reactGap : 0),
            y: y + (PostBarButton.height - Self.reactHeight) / 2, width: width,
            height: Self.reactHeight)
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
