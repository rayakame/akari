import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct MessageTextDrawingTests {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 500, height: 300), styleMask: [.titled],
        backing: .buffered, defer: false)

    // The chat's background under the text, so sampled pixels are opaque and compare closely.
    final class Backdrop: NSView {
        override var isFlipped: Bool { true }

        override func draw(_ dirtyRect: NSRect) {
            Palette.chat.setFill()
            bounds.fill()
        }
    }

    func shown(_ content: String, appearance: NSAppearance.Name) -> MessageTextView {
        let message = row(1, at: .now, content: content).message
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let backdrop = Backdrop(frame: NSRect(x: 0, y: 0, width: 400, height: 260))
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: .now, times: times).text)
        view.translatesAutoresizingMaskIntoConstraints = true
        view.frame = backdrop.bounds
        backdrop.addSubview(view)
        window.appearance = NSAppearance(named: appearance)
        window.contentView?.addSubview(backdrop)
        return view
    }

    // The rect TextKit 1 gives the first line holding `shown`, in the view's coordinates.
    func line(of shown: String, in view: MessageTextView) throws -> NSRect {
        let range = (view.string as NSString).range(of: shown)
        try #require(range.location != NSNotFound)
        let layout = try #require(view.layoutManager)
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return layout.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
            .offsetBy(dx: 0, dy: view.textContainerOrigin.y)
    }

    // A point inside the pill around `shown`: below its baseline, above the pill's border.
    func insidePill(of shown: String, in view: MessageTextView) throws -> NSPoint {
        let range = (view.string as NSString).range(of: shown)
        let layout = try #require(view.layoutManager)
        let container = try #require(view.textContainer)
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        let bounds = layout.boundingRect(forGlyphRange: glyphs, in: container)
        let line = layout.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
        let baseline = line.minY + layout.location(forGlyphAt: glyphs.location).y
        return NSPoint(x: bounds.midX, y: view.textContainerOrigin.y + baseline + 1.5)
    }

    // The chat's background with `token` over it, drawn through the same bitmap as the text.
    final class Swatch: NSView {
        let token: NSColor

        init(_ token: NSColor) {
            self.token = token
            super.init(frame: NSRect(x: 0, y: 0, width: 10, height: 10))
        }

        required init?(coder: NSCoder) {
            nil
        }

        override func draw(_ dirtyRect: NSRect) {
            Palette.chat.setFill()
            bounds.fill()
            token.setFill()
            bounds.fill(using: .sourceOver)
        }
    }

    func render(_ view: NSView, at point: NSPoint) throws -> NSColor {
        let rep = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / view.bounds.width
        return try #require(
            rep.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?.usingColorSpace(.sRGB))
    }

    func pixel(_ view: NSView, at point: NSPoint) throws -> NSColor {
        try render(try #require(view.superview), at: point)
    }

    func matches(_ drawn: NSColor, _ token: NSColor) throws -> Bool {
        let swatch = Swatch(token)
        window.contentView?.addSubview(swatch)
        defer { swatch.removeFromSuperview() }
        let expected = try render(swatch, at: NSPoint(x: 5, y: 5))
        return abs(drawn.redComponent - expected.redComponent) < 0.02
            && abs(drawn.greenComponent - expected.greenComponent) < 0.02
            && abs(drawn.blueComponent - expected.blueComponent) < 0.02
    }

    // The topmost shape behind the text at a point, in the text view's coordinates.
    func decoration(at point: NSPoint, in view: MessageTextView) -> (frame: NSRect, layer: CALayer)? {
        view.decorations.layoutSubtreeIfNeeded()
        view.decorations.displayIfNeeded()
        let shape = view.decorations.subviews.last { $0.frame.contains(point) }
        return shape.flatMap { shape in shape.layer.map { (shape.frame, $0) } }
    }

    func resolved(_ color: NSColor, _ appearance: NSAppearance.Name) -> NSColor {
        var result = color
        NSAppearance(named: appearance)?.performAsCurrentDrawingAppearance {
            result = color.usingColorSpace(.sRGB) ?? color
        }
        return result
    }

    func same(_ color: CGColor?, _ token: NSColor, _ appearance: NSAppearance.Name) -> Bool {
        guard let color, let drawn = NSColor(cgColor: color)?.usingColorSpace(.sRGB) else {
            return false
        }
        let expected = resolved(token, appearance)
        return abs(drawn.redComponent - expected.redComponent) < 0.01
            && abs(drawn.greenComponent - expected.greenComponent) < 0.01
            && abs(drawn.blueComponent - expected.blueComponent) < 0.01
            && abs(drawn.alphaComponent - expected.alphaComponent) < 0.01
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func pillsBoxesAndBarsAreDrawnInBothAppearances(_ appearance: NSAppearance.Name) throws {
        let view = shown("x `code` y\n> quoted\n```\nblock\n```", appearance: appearance)

        let pill = try pixel(view, at: try insidePill(of: "code", in: view))
        #expect(try matches(pill, Palette.codeBackground), "\(pill)")
        let quote = try line(of: "quoted", in: view)
        let bar = try #require(decoration(at: NSPoint(x: 2, y: quote.midY), in: view))
        #expect(same(bar.layer.backgroundColor, Palette.quoteBar, appearance))
        #expect(bar.frame.width == 4 && bar.layer.cornerRadius == 2)
        let block = try line(of: "block", in: view)
        let box = try #require(decoration(at: NSPoint(x: 4, y: block.midY), in: view))
        #expect(same(box.layer.backgroundColor, Palette.codeBackground, appearance))
        #expect(same(box.layer.borderColor, Palette.codeBorder, appearance))
        #expect(box.layer.borderWidth == 1 && box.layer.cornerRadius == 4)
        #expect(box.frame.maxX == view.bounds.width)
    }

    // Boxes and bars are layers: a view that draws gets a bitmap of everything it draws, and
    // past about 40,000 pt² Core Animation renders it on its costly path.
    @Test
    func theTextDrawsNoBitmapOfItsOwn() {
        #expect(!RowDrawingTests.drawsItsOwnBitmap(MessageTextView.self, over: NSTextView.self))
    }

    // TextKit 1 leaves out paragraph spacing above a first line, so the view pads it.
    @Test
    func aCodeBlockAtTheTopKeepsItsPadding() throws {
        let view = shown("```\nfirst\n```", appearance: .darkAqua)
        let layout = try #require(view.layoutManager)
        let first = layout.lineFragmentUsedRect(forGlyphAt: 0, effectiveRange: nil)

        #expect(first.minY + view.textContainerOrigin.y == MessageText.codePadding)
        let box = try #require(decoration(at: NSPoint(x: 100, y: 4), in: view))
        #expect(box.frame.minY == 0)
        #expect(box.frame.maxY == view.intrinsicContentSize.height)
    }

    @Test
    func anAppearanceChangeRecolorsBoxesAndBars() throws {
        let view = shown("> quoted\n```\nblock\n```", appearance: .darkAqua)
        let block = try line(of: "block", in: view)
        let quote = try line(of: "quoted", in: view)

        window.appearance = NSAppearance(named: .aqua)
        view.decorations.layoutSubtreeIfNeeded()

        let box = try #require(decoration(at: NSPoint(x: 20, y: block.midY), in: view))
        #expect(same(box.layer.backgroundColor, Palette.codeBackground, .aqua))
        let bar = try #require(decoration(at: NSPoint(x: 2, y: quote.midY), in: view))
        #expect(same(bar.layer.backgroundColor, Palette.quoteBar, .aqua))
    }

    @Test
    func anAppearanceChangeRecolorsFormattedText() throws {
        let view = shown("x `code` y", appearance: .darkAqua)
        let point = try insidePill(of: "code", in: view)
        #expect(try matches(try pixel(view, at: point), Palette.codeBackground))

        window.appearance = NSAppearance(named: .aqua)

        #expect(try matches(try pixel(view, at: point), Palette.codeBackground))
    }
}
