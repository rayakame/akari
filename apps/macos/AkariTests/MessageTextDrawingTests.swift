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

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func pillsBoxesAndBarsAreDrawnInBothAppearances(_ appearance: NSAppearance.Name) throws {
        let view = shown("x `code` y\n> quoted\n```\nblock\n```", appearance: appearance)

        let pill = try pixel(view, at: try insidePill(of: "code", in: view))
        #expect(try matches(pill, Palette.codeBackground), "\(pill)")
        let quote = try line(of: "quoted", in: view)
        let bar = try pixel(view, at: NSPoint(x: 2, y: quote.midY))
        #expect(try matches(bar, Palette.quoteBar), "\(bar)")
        let block = try line(of: "block", in: view)
        let box = try pixel(view, at: NSPoint(x: 4, y: block.midY))
        #expect(try matches(box, Palette.codeBackground), "\(box)")
    }

    // TextKit 1 leaves out paragraph spacing above a first line, so the view pads it.
    @Test
    func aCodeBlockAtTheTopKeepsItsPadding() throws {
        let view = shown("```\nfirst\n```", appearance: .darkAqua)
        let layout = try #require(view.layoutManager)
        let first = layout.lineFragmentUsedRect(forGlyphAt: 0, effectiveRange: nil)

        #expect(first.minY + view.textContainerOrigin.y == MessageText.codePadding)
        #expect(try matches(try pixel(view, at: NSPoint(x: 100, y: 4)), Palette.codeBackground))
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
