import AkariKit
import AppKit
import SwiftUI

/// A message on one line (`ThreadBoxView.content`), cut with an ellipsis: the text drawn once,
/// a custom emoji as the list's image layer on its box, there from the first layout. Only an
/// image that failed gives its box up to `:name:`. Takes no clicks.
final class PreviewLineView: NSView {
    private let storage = NSTextStorage()
    private let layoutManager = NSLayoutManager()
    private let container = NSTextContainer(size: .zero)
    private let overlay = EmojiOverlay()
    // The emoji whose image failed: their names stand in the line.
    private var failed: Set<ImageKey> = []

    var text = NSAttributedString() {
        didSet {
            // A line built again has new attachments, which compare by identity.
            guard !Self.same(text, oldValue) else {
                return
            }
            failed = []
            show()
        }
    }
    var images: ImageCache? {
        get { overlay.images }
        set {
            guard newValue !== overlay.images else {
                return
            }
            overlay.images = newValue
            if !failed.isEmpty {
                failed = []
                show()
            }
        }
    }
    /// The owner's say, by the list's rule for emoji.
    var animatesEmoji: Bool {
        get { overlay.animates }
        set { overlay.animates = newValue }
    }
    /// The line's plain text: its baseline is where a line of it lies in the view's height.
    var font = NSFont.systemFont(ofSize: TypeScale.size(14)) {
        didSet { needsDisplay = true }
    }
    /// The baseline's place below the view's top, where the owner draws the text beside it.
    var baseline: CGFloat? {
        didSet { needsDisplay = true }
    }
    /// A failed image's name took its box's place, or gave it back: the line's width changed.
    var onReflow: (() -> Void)?
    var emojiViews: [ImageLayerView] { overlay.emojiViews }
    /// The line as it shows now: a name where an image failed.
    var shown: NSAttributedString { NSAttributedString(attributedString: storage) }
    /// The shown line's whole width.
    private(set) var textWidth: CGFloat = 0

    init() {
        super.init(frame: .zero)
        container.lineFragmentPadding = 0
        container.maximumNumberOfLines = 1
        container.lineBreakMode = .byTruncatingTail
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
        overlay.onFailure = { [weak self] key in self?.fail(key) }
        addSubview(overlay)
        NotificationCenter.default.addObserver(
            self, selector: #selector(retry(_:)), name: ImageCache.retryFailedNotification, object: nil)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: textWidth, height: NSView.noIntrinsicMetric)
    }

    /// `line`'s width as the view shows it, when it is the view's own; else its plain width.
    func width(of line: NSAttributedString) -> CGFloat {
        Self.same(line, text) ? textWidth : ceil(line.size().width)
    }

    private static func same(_ line: NSAttributedString, _ other: NSAttributedString) -> Bool {
        guard line.string == other.string else {
            return false
        }
        var same = true
        line.enumerateAttributes(in: NSRange(location: 0, length: line.length)) { attributes, range, stop in
            var (mine, others) = (attributes, other.attributes(at: range.location, effectiveRange: nil))
            var covered = NSRange()
            _ = other.attributes(at: range.location, longestEffectiveRange: &covered, in: range)
            mine[.attachment] = nil
            others[.attachment] = nil
            if covered != range || !(mine as NSDictionary).isEqual(to: others) {
                same = false
                stop.pointee = true
            }
        }
        return same
    }

    override func setFrameSize(_ newSize: NSSize) {
        let resized = newSize != frame.size
        super.setFrameSize(newSize)
        if resized {
            needsLayout = true
            needsDisplay = true
        }
    }

    override func layout() {
        super.layout()
        overlay.frame = bounds
        container.size = NSSize(width: bounds.width, height: .greatestFiniteMagnitude)
        overlay.show(emojiShapes(), at: .zero)
    }

    /// Where the text's baseline lies below the top of a line `height` tall: `font`'s own line
    /// centered in it, as the text beside it in a row.
    static func baseline(in height: CGFloat, font: NSFont) -> CGFloat {
        (height - (font.ascender - font.descender)) / 2 + font.ascender
    }

    private var textTop: CGFloat {
        let glyphs = layoutManager.glyphRange(for: container)
        guard glyphs.length > 0 else {
            return 0
        }
        let line = layoutManager.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
        let drawn = line.minY + layoutManager.location(forGlyphAt: glyphs.location).y
        return (baseline ?? Self.baseline(in: bounds.height, font: font)) - drawn
    }

    override func draw(_ dirtyRect: NSRect) {
        guard bounds.width > 0, storage.length > 0 else {
            return
        }
        let origin = NSPoint(x: 0, y: textTop)
        let glyphs = layoutManager.glyphRange(for: container)
        layoutManager.drawBackground(forGlyphRange: glyphs, at: origin)
        layoutManager.drawGlyphs(forGlyphRange: glyphs, at: origin)
    }

    // The layer's image failed for good: the one time the line reflows.
    private func fail(_ key: ImageKey) {
        guard failed.insert(key).inserted else {
            return
        }
        show()
    }

    // The session is back: the boxes return and ask again.
    @objc private func retry(_ notification: Notification) {
        guard notification.object as AnyObject === images, !failed.isEmpty else {
            return
        }
        failed = []
        show()
    }

    // The text, each emoji whose image failed as its name.
    private func show() {
        let line = NSMutableAttributedString(attributedString: text)
        var index = line.length - 1
        while index >= 0, !failed.isEmpty {
            defer { index -= 1 }
            var attributes = line.attributes(at: index, effectiveRange: nil)
            guard let box = attributes[.akariEmoji] as? MessageText.EmojiBox, failed.contains(box.key)
            else {
                continue
            }
            attributes[.akariEmoji] = nil
            attributes[.attachment] = nil
            line.replaceCharacters(
                in: NSRange(location: index, length: 1),
                with: NSAttributedString(string: ":\(box.name):", attributes: attributes))
        }
        storage.setAttributedString(line)
        let width = ceil(line.size().width)
        let reflowed = width != textWidth
        textWidth = width
        invalidateIntrinsicContentSize()
        needsLayout = true
        needsDisplay = true
        if reflowed {
            onReflow?()
        }
    }

    // Laid out in the line, before the ellipsis, outside a hidden spoiler.
    private func isShown(_ index: Int) -> Bool {
        let spoiler = storage.attribute(.akariSpoiler, at: index, effectiveRange: nil)
        if let spoiler = spoiler as? MessageText.Spoiler, spoiler.hidden {
            return false
        }
        let glyph = layoutManager.glyphIndexForCharacter(at: index)
        guard NSLocationInRange(glyph, layoutManager.glyphRange(for: container)) else {
            return false
        }
        let cut = layoutManager.truncatedGlyphRange(inLineFragmentForGlyphAt: glyph)
        return cut.location == NSNotFound || glyph < cut.location
    }

    // A box for each emoji the line shows; the others ask for nothing.
    private func emojiShapes() -> [MessageBlockShape] {
        guard bounds.width > 0, storage.length > 0 else {
            return []
        }
        var shapes: [MessageBlockShape] = []
        storage.enumerateAttribute(.akariEmoji, in: NSRange(location: 0, length: storage.length)) {
            value, range, _ in
            guard let box = value as? MessageText.EmojiBox else {
                return
            }
            for index in range.location..<NSMaxRange(range) where isShown(index) {
                let glyph = layoutManager.glyphIndexForCharacter(at: index)
                let line = layoutManager.lineFragmentRect(forGlyphAt: glyph, effectiveRange: nil)
                let x = line.minX + layoutManager.location(forGlyphAt: glyph).x
                guard x + box.side <= bounds.width + 0.5 else {
                    continue
                }
                shapes.append(
                    .emoji(
                        NSRect(x: x, y: (bounds.height - box.side) / 2, width: box.side, height: box.side),
                        box))
            }
        }
        return shapes
    }
}

extension EnvironmentValues {
    /// The pointer is over the row or card a line is in: its emoji play.
    @Entry var rowHovered = false
}

/// `PreviewLineView` in a SwiftUI row: as wide as its text at most, on the row's text baseline,
/// its emoji played by the list's rule for emoji with the row's hover (`rowHovered`).
struct PreviewLine: View {
    let text: NSAttributedString
    var font = NSFont.systemFont(ofSize: TypeScale.size(14))
    let lineHeight: CGFloat

    var body: some View {
        let baseline = PreviewLineView.baseline(in: lineHeight, font: font)
        Line(text: text, font: font, lineHeight: lineHeight)
            .alignmentGuide(.firstTextBaseline) { _ in baseline }
            .alignmentGuide(.lastTextBaseline) { _ in baseline }
            // The row or the card around it takes the clicks and reads the line.
            .allowsHitTesting(false)
            .accessibilityLabel(MessageText.accessible(text))
    }

    private struct Line: NSViewRepresentable {
        let text: NSAttributedString
        let font: NSFont
        let lineHeight: CGFloat
        @Environment(\.rowHovered) private var hovered
        @Environment(\.imageCache) private var images
        @Environment(\.motion) private var motion
        @Environment(\.windowFocused) private var focused

        func makeNSView(context: Context) -> PreviewLineView {
            PreviewLineView()
        }

        func updateNSView(_ view: PreviewLineView, context: Context) {
            view.font = font
            view.text = text
            view.images = images
            view.animatesEmoji = motion.plays(
                .emoji, MotionPolicy.Context(windowKey: focused, appActive: focused, hovered: hovered))
        }

        func sizeThatFits(_ proposal: ProposedViewSize, nsView: PreviewLineView, context: Context)
            -> CGSize?
        {
            let ideal = nsView.textWidth
            return CGSize(width: min(proposal.width ?? ideal, ideal), height: lineHeight)
        }
    }
}
