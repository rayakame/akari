import AkariKit
import AppKit

/// The box under a message a thread started from, or under the line about a thread started
/// without one: Discord's measures, one button that opens the thread.
final class ThreadBoxView: NSButton {
    static let height: CGFloat = 56
    static let maxWidth: CGFloat = 480
    private static let border: CGFloat = 1
    private static let padding: CGFloat = 8
    private static let line: CGFloat = 18
    private static let lineGap: CGFloat = 2
    private static let radius: CGFloat = 4
    private static let avatar: CGFloat = 16
    private static let nameGap: CGFloat = 8
    private static let avatarGap: CGFloat = 8
    private static let authorGap: CGFloat = 4
    private static let timeGap: CGFloat = 8
    private static let font = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .regular)
    private static let strongFont = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .semibold)
    private static let authorFont = NSFont.systemFont(ofSize: TypeScale.size(14), weight: .medium)

    private(set) var box: ThreadBox?
    private var content: NSAttributedString?
    private var time: String?
    /// Kept clear above the box: the gap to the message's text. The view's height rounds up to a
    /// whole point, as row heights do.
    var topInset: CGFloat = 0 {
        didSet { invalidateIntrinsicContentSize() }
    }
    /// The list sets it while the pointer is over the box.
    var hovered = false {
        didSet {
            if hovered != oldValue {
                needsDisplay = true
            }
        }
    }

    init() {
        super.init(frame: .zero)
        isBordered = false
        title = ""
        setButtonType(.momentaryChange)
        translatesAutoresizingMaskIntoConstraints = false
        setContentHuggingPriority(.defaultLow, for: .horizontal)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override var intrinsicContentSize: NSSize {
        NSSize(width: NSView.noIntrinsicMetric, height: Self.height(topInset: topInset))
    }

    static func height(topInset: CGFloat) -> CGFloat {
        (topInset + height).rounded(.up)
    }

    /// `content`: the newest message's text as the box shows it, `time` the relative time.
    func configure(_ box: ThreadBox, content: NSAttributedString?, time: String?) {
        self.box = box
        self.content = content
        self.time = time
        // A reused cell's box may be elsewhere now; the list says after its update.
        hovered = false
        needsDisplay = true
    }

    /// The box itself, within the view.
    var boxFrame: NSRect {
        NSRect(x: 0, y: topInset, width: boxWidth, height: Self.height)
    }

    /// The texts of the first line, as drawn.
    var top: [String] {
        parts.filter { $0.line == 0 }.map(\.text.string)
    }

    /// The texts of the second line, as drawn.
    var bottom: [String] {
        parts.filter { $0.line == 1 && $0.text.length > 0 }.map(\.text.string)
    }

    var underlinesLink: Bool {
        hovered
    }

    /// Where `text` is drawn, from the view's top left.
    func rect(of text: String) -> NSRect? {
        parts.first { $0.text.string == text }?.rect
    }

    /// The drawn part that reads `text`, with its attributes.
    func text(of text: String) -> NSAttributedString? {
        parts.first { $0.text.string == text }?.text
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let superview else {
            return nil
        }
        return boxFrame.contains(convert(point, from: superview)) ? self : nil
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        rowMessageMenu(for: event)
    }


    override func draw(_ dirtyRect: NSRect) {
        guard box != nil else {
            return
        }
        let frame = boxFrame
        let outline = NSBezierPath(roundedRect: frame, xRadius: Self.radius, yRadius: Self.radius)
        Palette.threadBoxBorder.setFill()
        outline.fill()
        let inset = Self.border
        Palette.threadBoxFill.setFill()
        NSBezierPath(
            roundedRect: frame.insetBy(dx: inset, dy: inset), xRadius: Self.radius - inset,
            yRadius: Self.radius - inset
        ).fill()
        for part in parts {
            if let avatar = part.avatar {
                AvatarView.draw(name: avatar.name, userId: avatar.id, in: part.rect)
            } else {
                Self.draw(part.text, in: part.rect)
            }
        }
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .button
    }

    override func accessibilityLabel() -> String? {
        box.map { box in
            let link = box.link.replacingOccurrences(of: " ›", with: "")
            return "Thread \(box.name), \(link)"
        }
    }

    // A hidden spoiler reads as "Spoiler", as in the message itself.
    override func accessibilityValue() -> Any? {
        guard let box else {
            return nil
        }
        var words: [String]
        switch box.bottom {
        case .latest(_, let authorName):
            let text = content.map(MessageText.accessible) ?? ""
            words = [text.isEmpty ? authorName : "\(authorName): \(text)"]
        case .noRecentMessages:
            words = [ThreadBox.noRecentMessages]
        case .noMessages:
            words = [ThreadBox.noMessages]
        }
        if let time {
            words.append(time)
        }
        return words.joined(separator: ", ")
    }

    override var focusRingMaskBounds: NSRect {
        boxFrame
    }

    override func drawFocusRingMask() {
        NSBezierPath(roundedRect: boxFrame, xRadius: Self.radius, yRadius: Self.radius).fill()
    }

    // The newest message's content on one line: formatted, small, links not clickable, hidden
    // spoilers covered.
    static func content(_ built: NSAttributedString) -> NSAttributedString {
        let text = NSMutableAttributedString(attributedString: built)
        let whole = NSRange(location: 0, length: text.length)
        text.removeAttribute(.link, range: whole)
        text.removeAttribute(.toolTip, range: whole)
        text.removeAttribute(.paragraphStyle, range: whole)
        text.removeAttribute(.baselineOffset, range: whole)
        text.removeAttribute(.attachment, range: whole)
        let scale = TypeScale.size(14) / TypeScale.body.size
        text.enumerateAttribute(.font, in: whole) { value, range, _ in
            if let font = value as? NSFont,
                let smaller = NSFont(descriptor: font.fontDescriptor, size: font.pointSize * scale)
            {
                text.addAttribute(.font, value: smaller, range: range)
            }
        }
        text.enumerateAttribute(.akariSpoiler, in: whole) { value, range, _ in
            if let spoiler = value as? MessageText.Spoiler, spoiler.hidden {
                text.addAttribute(.backgroundColor, value: Palette.spoilerHidden, range: range)
            }
        }
        for (search, replacement) in [("\n", " "), ("\u{2028}", " "), ("\u{FFFC}", "")] {
            var range = (text.string as NSString).range(of: search)
            while range.location != NSNotFound {
                text.replaceCharacters(in: range, with: replacement)
                let start = range.location + (replacement as NSString).length
                range = (text.string as NSString).range(
                    of: search, range: NSRange(location: start, length: text.length - start))
            }
        }
        return text
    }

    // The connector: 2 wide under the avatar's or icon's center, from 5 below it, bending with an
    // 8 outer radius into the box's middle and ending 3 before it (Discord's cozy spine).
    private static let spineWidth: CGFloat = 2
    private static let spineStart: CGFloat = 5
    private static let spineRadius: CGFloat = 7
    private static let spineEnd: CGFloat = 3

    /// Draws the line from under `anchor` into `box`, both in the drawing view's coordinates.
    static func drawConnector(under anchor: NSRect, to box: NSRect, flipped: Bool) {
        let x = anchor.midX
        let down: CGFloat = flipped ? 1 : -1
        let start = (flipped ? anchor.maxY : anchor.minY) + down * spineStart
        let middle = box.midY
        let path = NSBezierPath()
        path.move(to: NSPoint(x: x, y: start))
        path.line(to: NSPoint(x: x, y: middle - down * spineRadius))
        path.appendArc(
            withCenter: NSPoint(x: x + spineRadius, y: middle - down * spineRadius),
            radius: spineRadius, startAngle: 180, endAngle: flipped ? 90 : 270, clockwise: flipped)
        path.line(to: NSPoint(x: box.minX - spineEnd, y: middle))
        path.lineWidth = spineWidth
        Palette.threadSpine.setStroke()
        path.stroke()
    }

    static func placeholder(_ text: String) -> NSAttributedString {
        let italic =
            NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
        return NSAttributedString(
            string: text, attributes: [.font: italic, .foregroundColor: Palette.textDefault])
    }

    private struct Part {
        let text: NSAttributedString
        let rect: NSRect
        let line: Int
        var avatar: (name: String, id: UInt64)?
    }

    private var boxWidth: CGFloat {
        let top = firstLine.map(Self.width).reduce(0, +) + Self.nameGap
        var bottom = secondLine.map(Self.width).reduce(0, +)
        if case .latest? = box?.bottom {
            bottom += Self.avatar + Self.avatarGap + Self.authorGap
        }
        if time != nil {
            bottom += Self.timeGap
        }
        let natural = max(top, bottom) + 2 * (Self.border + Self.padding)
        return min(Self.maxWidth, bounds.width, natural)
    }

    private var firstLine: [NSAttributedString] {
        guard let box else {
            return []
        }
        var link: [NSAttributedString.Key: Any] = [
            .font: Self.strongFont, .foregroundColor: Palette.textLink,
        ]
        if hovered {
            link[.underlineStyle] = NSUnderlineStyle.single.rawValue
        }
        return [
            NSAttributedString(
                string: box.name,
                attributes: [.font: Self.strongFont, .foregroundColor: Palette.textStrong]),
            NSAttributedString(string: box.link, attributes: link),
        ]
    }

    // Avatar and author, then the content, then the time; or a sentence and the time.
    private var secondLine: [NSAttributedString] {
        guard let box else {
            return []
        }
        var parts: [NSAttributedString] = []
        switch box.bottom {
        case .latest(_, let authorName):
            parts.append(NSAttributedString(string: ""))
            parts.append(
                NSAttributedString(
                    string: authorName,
                    attributes: [
                        .font: Self.authorFont,
                        .foregroundColor: Palette.textStrong.withAlphaComponent(0.64),
                    ]))
            parts.append(content ?? NSAttributedString())
        case .noRecentMessages:
            parts.append(Self.plain(ThreadBox.noRecentMessages, Palette.textDefault))
        case .noMessages:
            parts.append(Self.plain(ThreadBox.noMessages, Palette.textDefault))
        }
        if let time {
            parts.append(Self.plain(time, Palette.textMuted))
        }
        return parts
    }

    // Laid out left to right; the name and the content give way first.
    private var parts: [Part] {
        guard let box else {
            return []
        }
        let frame = boxFrame
        let left = frame.minX + Self.border + Self.padding
        let right = frame.maxX - Self.border - Self.padding
        let firstTop = frame.minY + Self.border + Self.padding
        let secondTop = firstTop + Self.line + Self.lineGap
        var parts: [Part] = []

        let top = firstLine
        let linkWidth = Self.width(top[1])
        let nameWidth = max(0, min(Self.width(top[0]), right - left - Self.nameGap - linkWidth))
        parts.append(Part(text: top[0], rect: line(left, firstTop, nameWidth), line: 0))
        parts.append(
            Part(
                text: top[1], rect: line(left + nameWidth + Self.nameGap, firstTop, linkWidth),
                line: 0))

        let second = secondLine
        var x = left
        var end = right
        var timeRect: NSRect?
        if let time = second.last, self.time != nil {
            let width = Self.width(time)
            end = right - width - Self.timeGap
            timeRect = line(right - width, secondTop, width)
        }
        if case .latest(let latest, _) = box.bottom {
            let avatarRect = NSRect(
                x: x, y: secondTop + (Self.line - Self.avatar) / 2, width: Self.avatar,
                height: Self.avatar)
            parts.append(
                Part(
                    text: NSAttributedString(), rect: avatarRect, line: 1,
                    avatar: (second[1].string, latest.author.id.rawValue)))
            x += Self.avatar + Self.avatarGap
            let authorWidth = max(0, min(Self.width(second[1]), end - x))
            parts.append(Part(text: second[1], rect: line(x, secondTop, authorWidth), line: 1))
            x += authorWidth + Self.authorGap
            let contentWidth = max(0, min(Self.width(second[2]), end - x))
            parts.append(Part(text: second[2], rect: line(x, secondTop, contentWidth), line: 1))
            x += contentWidth
        } else {
            let width = max(0, min(Self.width(second[0]), end - x))
            parts.append(Part(text: second[0], rect: line(x, secondTop, width), line: 1))
            x += width
        }
        if let timeRect, let time = second.last {
            // Right after the text when the box is wider than its content.
            let shifted = timeRect.offsetBy(dx: min(0, x + Self.timeGap - timeRect.minX), dy: 0)
            parts.append(Part(text: time, rect: shifted, line: 1))
        }
        return parts
    }

    private func line(_ x: CGFloat, _ top: CGFloat, _ width: CGFloat) -> NSRect {
        NSRect(x: x, y: top, width: width, height: Self.line)
    }

    private static func plain(_ text: String, _ color: NSColor) -> NSAttributedString {
        NSAttributedString(string: text, attributes: [.font: font, .foregroundColor: color])
    }

    private static func width(_ text: NSAttributedString) -> CGFloat {
        ceil(text.size().width)
    }

    // On one line, cut with an ellipsis, centered in the 18 tall line.
    private static func draw(_ text: NSAttributedString, in rect: NSRect) {
        guard text.length > 0, rect.width > 0 else {
            return
        }
        let truncated = NSMutableAttributedString(attributedString: text)
        let style = NSMutableParagraphStyle()
        style.lineBreakMode = .byTruncatingTail
        truncated.addAttribute(
            .paragraphStyle, value: style, range: NSRange(location: 0, length: text.length))
        let height = ceil(font.ascender - font.descender)
        truncated.draw(
            with: NSRect(
                x: rect.minX, y: rect.minY + (rect.height - height) / 2, width: rect.width,
                height: height),
            options: [.usesLineFragmentOrigin, .truncatesLastVisibleLine])
    }
}
