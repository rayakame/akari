import AkariKit
import AppKit
import SwiftUI

/// A message on one line (`ThreadBoxView.content`), cut with an ellipsis: the text drawn once,
/// its custom emoji as the list's image layers on their boxes. Takes no clicks.
final class PreviewLineView: NSView {
    private let storage = NSTextStorage()
    private let layoutManager = NSLayoutManager()
    private let container = NSTextContainer(size: .zero)
    private let overlay = EmojiOverlay()

    var text = NSAttributedString() {
        didSet {
            guard text != oldValue else {
                return
            }
            storage.setAttributedString(text)
            invalidateIntrinsicContentSize()
            needsLayout = true
            needsDisplay = true
        }
    }
    var images: ImageCache? {
        get { overlay.images }
        set { overlay.images = newValue }
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
    /// Off inside a control that reads the line itself.
    var readsItself = true
    var emojiViews: [ImageLayerView] { overlay.emojiViews }

    init() {
        super.init(frame: .zero)
        container.lineFragmentPadding = 0
        container.maximumNumberOfLines = 1
        container.lineBreakMode = .byTruncatingTail
        layoutManager.addTextContainer(container)
        storage.addLayoutManager(layoutManager)
        overlay.namesWhileLoading = true
        addSubview(overlay)
    }

    required init?(coder: NSCoder) {
        nil
    }

    override var isFlipped: Bool { true }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: ceil(text.size().width), height: NSView.noIntrinsicMetric)
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
        let baseline = line.minY + layoutManager.location(forGlyphAt: glyphs.location).y
        return Self.baseline(in: bounds.height, font: font) - baseline
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

    // A box for each emoji the line still shows: not past the ellipsis, not under a hidden
    // spoiler, which asks for nothing.
    private func emojiShapes() -> [MessageBlockShape] {
        guard bounds.width > 0, storage.length > 0 else {
            return []
        }
        let shown = layoutManager.glyphRange(for: container)
        var shapes: [MessageBlockShape] = []
        storage.enumerateAttribute(.akariEmoji, in: NSRange(location: 0, length: storage.length)) {
            value, range, _ in
            guard let box = value as? MessageText.EmojiBox else {
                return
            }
            let spoiler = storage.attribute(.akariSpoiler, at: range.location, effectiveRange: nil)
            if let spoiler = spoiler as? MessageText.Spoiler, spoiler.hidden {
                return
            }
            for index in range.location..<NSMaxRange(range) {
                let glyph = layoutManager.glyphIndexForCharacter(at: index)
                guard NSLocationInRange(glyph, shown) else {
                    continue
                }
                let cut = layoutManager.truncatedGlyphRange(inLineFragmentForGlyphAt: glyph)
                if cut.location != NSNotFound, glyph >= cut.location {
                    continue
                }
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

    override func isAccessibilityElement() -> Bool {
        readsItself
    }

    override func accessibilityRole() -> NSAccessibility.Role? {
        .staticText
    }

    override func accessibilityValue() -> Any? {
        MessageText.accessible(text)
    }
}

/// `PreviewLineView` in a SwiftUI row: as wide as its text at most, on the row's text baseline,
/// its emoji played by the list's rule for emoji with the row's hover.
struct PreviewLine: View {
    let text: NSAttributedString
    var font = NSFont.systemFont(ofSize: TypeScale.size(14))
    let lineHeight: CGFloat
    var hovered = false

    var body: some View {
        let baseline = PreviewLineView.baseline(in: lineHeight, font: font)
        Line(text: text, font: font, lineHeight: lineHeight, hovered: hovered)
            .alignmentGuide(.firstTextBaseline) { _ in baseline }
            .alignmentGuide(.lastTextBaseline) { _ in baseline }
    }

    private struct Line: NSViewRepresentable {
        let text: NSAttributedString
        let font: NSFont
        let lineHeight: CGFloat
        let hovered: Bool
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
            let ideal = ceil(text.size().width)
            return CGSize(width: min(proposal.width ?? ideal, ideal), height: lineHeight)
        }
    }
}
