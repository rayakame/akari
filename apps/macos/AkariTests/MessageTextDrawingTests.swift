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

        let pill = try #require(decoration(at: try insidePill(of: "code", in: view), in: view))
        #expect(same(pill.layer.backgroundColor, Palette.codeBackground, appearance))
        #expect(same(pill.layer.borderColor, Palette.codeBorder, appearance))
        #expect(pill.layer.cornerRadius == 4)
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

    @Test
    func mentionsRolesAndRevealedSpoilersArePillsToo() throws {
        let message = named(row(1, at: .now, content: "<@1> <@&4> ||shown||")).message
        var colored = message
        for block in colored.formatted.blocks.indices {
            for span in colored.formatted.blocks[block].spans.indices {
                if case .role(let id, _, _) = colored.formatted.blocks[block].spans[span].kind {
                    colored.formatted.blocks[block].spans[span].kind = .role(
                        id: id, name: .known(name: "Red"), color: 0xff0000)
                }
            }
        }
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let view = MessageTextView()
        view.show(MessageText.build(colored, revealed: [0], now: .now, times: times).text)
        view.translatesAutoresizingMaskIntoConstraints = true
        view.frame = NSRect(x: 0, y: 0, width: 400, height: 60)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView?.addSubview(view)

        let mention = try #require(decoration(at: try insidePill(of: "@mira", in: view), in: view))
        #expect(same(mention.layer.backgroundColor, Palette.mentionBackground, .darkAqua))
        let role = try #require(decoration(at: try insidePill(of: "@Red", in: view), in: view))
        let red = try #require(role.layer.backgroundColor.flatMap { NSColor(cgColor: $0) }?
            .usingColorSpace(.sRGB))
        #expect(red.redComponent == 1 && abs(red.alphaComponent - 0.1) < 0.01)
        let spoiler = try #require(decoration(at: try insidePill(of: "shown", in: view), in: view))
        #expect(same(spoiler.layer.backgroundColor, Palette.spoilerRevealed, .darkAqua))
    }

    @Test
    func adjacentPillsStayApart() throws {
        let message = named(row(1, at: .now, content: "<@1><@2> <#5><@3>")).message
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: .now, times: times).text)
        view.translatesAutoresizingMaskIntoConstraints = true
        view.frame = NSRect(x: 0, y: 0, width: 400, height: 40)
        window.contentView?.addSubview(view)

        #expect(view.decorations.subviews.count == 4)
    }

    // The number column grows with the list's widest number.
    @Test(arguments: ["9. a\n10. b", "99. a\n100. b", "- x\n  10. a"])
    func orderedNumbersKeepAGapBeforeTheirText(_ content: String) throws {
        let view = shown(content, appearance: .darkAqua)
        let layout = try #require(view.layoutManager)
        let container = try #require(view.textContainer)
        let storage = view.string as NSString
        var searched = NSRange(location: 0, length: storage.length)
        while true {
            let number = storage.range(
                of: "[0-9]+\\.", options: .regularExpression, range: searched)
            guard number.location != NSNotFound else {
                break
            }
            let item = NSRange(location: NSMaxRange(number) + 1, length: 1)
            let glyphs = { (range: NSRange) in
                layout.boundingRect(
                    forGlyphRange: layout.glyphRange(
                        forCharacterRange: range, actualCharacterRange: nil),
                    in: container)
            }
            #expect(glyphs(number).maxX + 3 < glyphs(item).minX, "\(storage.substring(with: number))")
            searched = NSRange(location: NSMaxRange(number), length: storage.length - NSMaxRange(number))
        }
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
        let view = shown("x `code` y <t:1800000000:f>", appearance: .darkAqua)
        let code = try insidePill(of: "code", in: view)
        let range = (view.string as NSString).range(of: " y ")
        let layout = try #require(view.layoutManager)
        let after = layout.boundingRect(
            forGlyphRange: layout.glyphRange(
                forCharacterRange: NSRange(location: NSMaxRange(range) + 2, length: 1),
                actualCharacterRange: nil),
            in: try #require(view.textContainer))
        let stamp = NSPoint(x: after.midX, y: code.y)

        window.appearance = NSAppearance(named: .aqua)

        let pill = try #require(decoration(at: code, in: view))
        #expect(same(pill.layer.backgroundColor, Palette.codeBackground, .aqua))
        let timestamp = try #require(decoration(at: stamp, in: view))
        #expect(same(timestamp.layer.backgroundColor, Palette.timestampBackground, .aqua))
    }
}
