import AkariKit
import AppKit
import Testing

@testable import Akari

// Custom emoji in a message's text (D139): the layout manager gives each its box, the
// decorations put an image layer on it from the cache, a failed one shows its name in the box,
// the list says when they play (the setting, 70 % visible, the nearest 100, or the message
// hovered), and copying gives `:name:`.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class EmojiTests {
    let images = TestImages()
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    let now = Date(timeIntervalSince1970: 1_800_000_000 - 300)
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled],
        backing: .buffered, defer: false)

    func text(_ content: String) -> NSAttributedString {
        let message = row(1, at: now, content: content).message
        return MessageText.build(message, revealed: [], now: now, times: times).text
    }

    func shown(_ content: String, width: CGFloat = 400, images cache: ImageCache? = nil) -> MessageTextView {
        let view = MessageTextView()
        view.images = cache
        let container = NSView(frame: NSRect(x: 0, y: 0, width: width, height: 400))
        window.contentView = container
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.widthAnchor.constraint(equalToConstant: width),
        ])
        view.show(text(content))
        container.layoutSubtreeIfNeeded()
        return view
    }

    func emojiShapes(_ view: MessageTextView) -> [(rect: NSRect, box: MessageText.EmojiBox)] {
        ((view.layoutManager as? MessageLayoutManager)?.blocks() ?? []).compactMap {
            if case .emoji(let rect, let box) = $0 { (rect, box) } else { nil }
        }
    }

    func emojiLayers(_ view: MessageTextView) -> [ImageLayerView] {
        MessageMenuAreaTests.views(ImageLayerView.self, in: view.emojiOverlay)
    }

    func lineFragment(_ view: MessageTextView, at character: Int) throws -> NSRect {
        let layout = try #require(view.layoutManager)
        let glyph = layout.glyphIndexForCharacter(at: character)
        return layout.lineFragmentUsedRect(forGlyphAt: glyph, effectiveRange: nil)
    }

    // Inline: 22 × 22 with its bottom on the line's bottom; jumbo: 48 × 48 centered on its line.
    @Test
    func theLayoutManagerGivesEachEmojiItsBox() throws {
        let inline = shown("hi <:wave:5> there <a:dance:6>")
        let shapes = emojiShapes(inline)
        #expect(shapes.map(\.box.name) == ["wave", "dance"])
        for shape in shapes {
            #expect(shape.rect.size == NSSize(width: 22, height: 22))
        }
        let line = try lineFragment(inline, at: 3)
        #expect(shapes[0].rect.maxY == line.maxY, "\(shapes[0].rect) in \(line)")
        #expect(shapes[0].rect.minX > 16 && shapes[0].rect.minX < 30, "after \"hi \"")
        #expect(shapes[1].rect.minX > shapes[0].rect.maxX)

        let jumbo = shown("<:wave:5> 😀")
        let big = emojiShapes(jumbo)
        #expect(big.count == 1)
        #expect(big.first?.rect.size == NSSize(width: 48, height: 48))
        let jumboLine = try lineFragment(jumbo, at: 0)
        #expect(big.first?.rect.midY == jumboLine.midY, "\(big) in \(jumboLine)")
    }

    // TextKit gives an attachment without an image a 1 pt advance: the box needs its own cell,
    // so the text after it moves over and string drawing (row heights) agrees with the view.
    @Test
    func theEmojiBoxAdvancesTheTextByItsWidth() throws {
        let inline = shown("hi <:wave:5> there")
        let layout = try #require(inline.layoutManager)
        let box = layout.location(forGlyphAt: layout.glyphIndexForCharacter(at: 3)).x
        let after = layout.location(forGlyphAt: layout.glyphIndexForCharacter(at: 4)).x
        #expect(after - box >= 22, "\(box) → \(after)")

        let jumbo = shown(String(repeating: "<:wave:5>", count: 5))
        let rects = emojiShapes(jumbo).map(\.rect)
        #expect(rects.count == 5)
        for (left, right) in zip(rects, rects.dropFirst()) {
            #expect(right.minX - left.minX >= 48, "\(rects)")
        }
    }

    @Test(arguments: [400, 292] as [CGFloat])
    func theMeasurerAgreesWithTheViewForEmoji(_ width: CGFloat) {
        for content in [
            "hi <:wave:5> there", String(repeating: "<:wave:5>", count: 5),
            String(repeating: "words <a:dance:6> and more <:wave:5> ", count: 12), "-# small <:wave:5> text",
            "# big <:wave:5>\nbody <:wave:5>", "> quoted <:wave:5>\n- item <:wave:5>",
        ] {
            let built = text(content)
            let view = shown(content, width: width)
            let measured = TextMeasurer.shared.height(built, width: width)
            #expect(
                view.frame.height == measured,
                "\(content.prefix(30)) at \(width): view \(view.frame.height) measured \(measured)")
        }
    }

    // The selection's fill is opaque; the emoji sit above the text, so a selected emoji stays
    // visible.
    @Test
    func emojiLayersSitAboveTheText() throws {
        let view = shown("hi <:wave:5> there", images: ImageCache(source: FakeImageSource.stalling()))
        let layer = try #require(emojiLayers(view).first)
        let container = try #require(view.superview)
        let host = try #require(layer.superview)

        let textIndex = try #require(container.subviews.firstIndex(of: view))
        let hostIndex = try #require(container.subviews.firstIndex(of: host))
        #expect(hostIndex > textIndex)
        #expect(host.frame == view.frame)
        #expect(host.hitTest(host.convert(layer.frame.origin, to: container)) == nil)
    }

    @Test
    func theDecorationsHostAnImageLayerPerEmoji() async throws {
        let source = FakeImageSource.serving(try images.png("e", pixels: 64), pixels: 64)
        let cache = ImageCache(source: source)
        let view = shown("hi <:wave:5> there <a:dance:6>", images: cache)

        let layers = emojiLayers(view)
        #expect(layers.count == 2)
        #expect(layers.map { $0.content?.key } == [
            .emoji(emojiId: EmojiId(rawValue: 5), animated: false),
            .emoji(emojiId: EmojiId(rawValue: 6), animated: true),
        ])
        #expect(layers.first?.images === cache)
        #expect(layers.first?.placeholder == ImageLayerView.Placeholder.none)
        #expect(layers.allSatisfy { $0.fit == .contain }, "the whole emoji, never cut")
        let shapes = emojiShapes(view)
        #expect(layers.first?.frame.size == shapes.first?.rect.size)
        #expect(layers.first?.frame.minX == (shapes.first?.rect.minX ?? -1) + view.textContainerOrigin.x)
        try await eventually { layers.allSatisfy { $0.imageLayer.contents != nil } }
        let scale = window.backingScaleFactor
        #expect(source.requests.withLock { $0.first }?.width == UInt32((22 * scale).rounded(.up)))
        #expect(layers.first?.animates == false)

        view.show(text("no emoji"))
        #expect(emojiLayers(view).isEmpty)
    }

    // A failed emoji shows `:name:` in its box; the text doesn't reflow (C-4).
    @Test
    func aFailedEmojiShowsItsNameInTheBoxWithoutReflow() async throws {
        let cache = ImageCache(source: FakeImageSource.failing())
        let view = shown("hi <:wave:5> there", images: cache)
        let height = view.intrinsicContentSize.height
        let layer = try #require(emojiLayers(view).first)

        try await eventually { layer.failed }

        #expect(layer.fallbackText == ":wave:")
        #expect(!layer.fallbackLayer.isHidden)
        #expect(layer.fallbackLayer.string as? String == ":wave:")
        #expect(view.intrinsicContentSize.height == height)
        #expect(view.string == "hi \u{FFFC} there")
    }

    @Test
    func animatesEmojiPlaysTheLayers() throws {
        let view = shown("<a:dance:6> <:wave:5>", images: ImageCache(source: FakeImageSource.stalling()))
        let layers = emojiLayers(view)

        view.animatesEmoji = true
        #expect(layers.map(\.animates) == [true, true])
        #expect(layers.map { $0.content?.canAnimate } == [true, false])
        view.animatesEmoji = false
        #expect(layers.map(\.animates) == [false, false])
        view.show(text("<a:dance:6>"))
        view.animatesEmoji = true
        #expect(emojiLayers(view).first?.animates == true)
    }

    @Test
    func copyingAcrossAnEmojiGivesItsName() {
        let board = PrivatePasteboard()
        let view = shown("hi <:wave:5> there")

        view.setSelectedRange(NSRange(location: 0, length: view.string.utf16.count))
        _ = view.writeSelection(to: board.pasteboard, types: view.writablePasteboardTypes)

        #expect(board.pasteboard.string(forType: .string) == "hi :wave: there")
    }

    // The list's say per cell: the setting with 70 % visible and the nearest 100 messages, or
    // the message hovered (C-1, C-3); nothing while the window isn't key.
    @Test
    func theListAnimatesEmojiByTheSettingVisibilityHoverAndTheNearestHundred() throws {
        let list = MessageTableTests()
        let cache = ImageCache(source: FakeImageSource.stalling())
        let rows = (1...120).map { raw in
            row(UInt64(raw), at: list.noon + Double(raw) * 120, content: "<a:dance:6> m \(raw)")
        }
        func state(focused: Bool = true, motion: MotionPolicy = .default) -> MessageTableState {
            MessageTableState(
                rows: rows, reachedOldest: true, images: ImageCacheRef(cache), motion: motion,
                focused: focused)
        }
        func cell(_ raw: UInt64) throws -> MessageCell {
            try #require(try list.cell(of: raw) as? MessageCell)
        }
        list.show(state())

        #expect(try cell(120).animatesEmoji, "near the bottom, visible, the setting on")
        list.show(state(focused: false))
        #expect(try !cell(120).animatesEmoji, "not while the window isn't key")
        var off = MotionPolicy.default
        off.animateEmoji = false
        list.show(state(motion: off))
        #expect(try !cell(120).animatesEmoji)
        let text = try #require(list.messageTexts(in: try cell(119)).first)
        list.pointer.location = text.convert(NSPoint(x: 4, y: text.bounds.midY), to: nil)
        list.controller.pointerMoved()
        #expect(try cell(119).animatesEmoji, "the hovered message plays its emoji")
        #expect(try !cell(120).animatesEmoji)
        list.pointer.location = nil
        list.controller.pointerMoved()
        list.show(state())

        // A row half outside the visible rect.
        let index = list.index(of: 118)
        let rect = list.table.rect(ofRow: index)
        list.clip.scroll(to: NSPoint(x: 0, y: rect.midY))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()
        #expect(try !cell(118).animatesEmoji, "half visible")
        #expect(try cell(119).animatesEmoji)

        // The oldest rows: not among the 100 nearest the bottom.
        list.clip.scroll(to: NSPoint(x: 0, y: 0))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()
        #expect(try !cell(1).animatesEmoji)
        #expect(try !cell(20).animatesEmoji)
        let old = try #require(list.messageTexts(in: try cell(2)).first)
        list.pointer.location = old.convert(NSPoint(x: 4, y: old.bounds.midY), to: nil)
        list.controller.pointerMoved()
        #expect(try cell(2).animatesEmoji, "hovered plays anywhere")
    }
}
