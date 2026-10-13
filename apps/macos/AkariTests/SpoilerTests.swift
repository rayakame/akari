import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct SpoilerTests {
    let list = MessageListFixture()

    func key(_ raw: UInt64, _ index: UInt16) -> SpoilerKey {
        SpoilerKey(messageId: MessageId(rawValue: raw), index: index)
    }

    func spoiler(_ shown: String, in text: MessageTextView) throws -> MessageText.Spoiler? {
        let index = (text.string as NSString).range(of: shown).location
        try #require(index != NSNotFound)
        return text.textStorage?.attribute(.akariSpoiler, at: index, effectiveRange: nil)
            as? MessageText.Spoiler
    }

    @Test
    func aHiddenSpoilerIsCovered() throws {
        let drawing = MessageTextDrawingTests()
        let view = drawing.shown("a ||secret words|| b", appearance: .darkAqua)
        let range = (view.string as NSString).range(of: "secret words")
        let layout = try #require(view.layoutManager)
        let container = try #require(view.textContainer)
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        let bounds = layout.boundingRect(forGlyphRange: glyphs, in: container)
        let line = layout.lineFragmentRect(forGlyphAt: glyphs.location, effectiveRange: nil)
        let font = try #require(
            view.textStorage?.attribute(.font, at: range.location, effectiveRange: nil) as? NSFont)
        let baseline = line.minY + layout.location(forGlyphAt: glyphs.location).y

        // Through the letters' middle, where any drawn glyph would show.
        for x in stride(from: bounds.minX + 1, to: bounds.maxX - 1, by: 1.5) {
            let point = NSPoint(x: x, y: baseline - font.xHeight / 2)
            #expect(
                try drawing.matches(try drawing.pixel(view, at: point), Palette.spoilerHidden),
                "at \(x)")
        }
    }

    @Test
    func aHiddenCodeBlockIsCoveredWhole() throws {
        let drawing = MessageTextDrawingTests()
        let view = drawing.shown("||```\nsecret\n```||", appearance: .darkAqua)
        let height = view.intrinsicContentSize.height

        let cover = try #require(drawing.decoration(at: NSPoint(x: 200, y: 2), in: view))
        #expect(drawing.same(cover.layer.backgroundColor, Palette.spoilerHidden, .darkAqua))
        #expect(cover.frame == NSRect(x: 0, y: 0, width: view.bounds.width, height: height))
        // Over the text too, so a selection's highlight stays hidden.
        let middle = NSPoint(x: 30, y: height / 2)
        #expect(try drawing.matches(try drawing.pixel(view, at: middle), Palette.spoilerHidden))
    }

    @Test
    func aClickRevealsOnlyThatSpoiler() throws {
        list.show([row(1, at: list.noon, content: "||one|| and ||two||")])
        let height = list.table.rect(ofRow: list.index(of: 1)).height

        try list.click("one", in: 1)
        #expect(list.actions.calls == ["reveal 1 0"])
        list.reveal([key(1, 0)])

        let text = try list.text(of: 1)
        #expect(try spoiler("one", in: text)?.hidden == false)
        #expect(try spoiler("two", in: text)?.hidden == true)
        #expect(list.table.rect(ofRow: list.index(of: 1)).height == height)
    }

    @Test
    func aRevealedSpoilerStaysRevealedAfterReuse() throws {
        let rows =
            [row(1, at: list.noon, content: "||hidden|| text")]
            + (2...80).map { raw in
                row(raw, by: author(raw % 2 + 1), at: list.noon + Double(raw) * 60)
            }
        list.show(rows, revealed: [key(1, 0)])
        list.table.scrollRowToVisible(0)
        list.window.layoutIfNeeded()
        let height = list.table.rect(ofRow: list.index(of: 1)).height
        #expect(try spoiler("hidden", in: try list.text(of: 1))?.hidden == false)

        list.table.scrollRowToVisible(list.table.numberOfRows - 1)
        list.window.layoutIfNeeded()
        // A reread brings the same rows again, and a newer message arrives.
        list.show(rows + [row(81, at: list.noon + 81 * 60)], revealed: [key(1, 0)])
        list.table.scrollRowToVisible(0)
        list.window.layoutIfNeeded()

        #expect(try spoiler("hidden", in: try list.text(of: 1))?.hidden == false)
        #expect(list.table.rect(ofRow: list.index(of: 1)).height == height)
    }

    @Test
    func aSecondClickKeepsItRevealed() throws {
        list.show([row(1, at: list.noon, content: "||one||")], revealed: [key(1, 0)])
        let text = try list.text(of: 1)

        #expect(text.textStorage?.attribute(.link, at: 0, effectiveRange: nil) == nil)
        #expect(try spoiler("one", in: text)?.hidden == false)
        #expect(list.actions.calls.isEmpty)
    }

    @Test
    func voiceOverSaysSpoiler() throws {
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let message = row(1, at: .now, content: "a ||secret|| b").message
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: .now, times: times).text)

        let hidden = try #require(view.accessibilityValue() as? String)
        #expect(hidden.contains("Spoiler") && !hidden.contains("secret"), "\(hidden)")

        view.show(MessageText.build(message, revealed: [0], now: .now, times: times).text)
        #expect((view.accessibilityValue() as? String)?.contains("secret") == true)
    }
}
