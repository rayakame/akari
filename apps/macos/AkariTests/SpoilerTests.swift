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

    // `render_spoilers` "Always" (or "If moderator" with Manage Messages): the core says the
    // channel's spoilers show, in the text as in the media.
    @Test
    func aChannelThatShowsSpoilersBuildsThemUncovered() throws {
        var message = row(1, at: .now, content: "a ||one|| b ||`two`||").message
        message.spoilersShown = true
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)

        let text = MessageText.build(message, revealed: [], now: .now, times: times).text
        var hidden: [Bool] = []
        text.enumerateAttribute(.akariSpoiler, in: NSRange(location: 0, length: text.length)) { value, _, _ in
            if let spoiler = value as? MessageText.Spoiler {
                hidden.append(spoiler.hidden)
            }
        }

        #expect(!hidden.isEmpty)
        #expect(hidden.allSatisfy { !$0 })
        #expect(text.string.contains("one") && text.string.contains("two"))
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
        list.show([row(1, at: list.noon, content: "||one||")])

        try press("one", in: try list.text(of: 1))
        #expect(list.actions.calls == ["reveal 1 0"])
        list.reveal([key(1, 0)])
        let text = try list.text(of: 1)
        try press("one", in: text)

        #expect(list.actions.calls == ["reveal 1 0"])
        #expect(try spoiler("one", in: text)?.hidden == false)
    }

    func click(
        _ type: NSEvent.EventType, at point: NSPoint, in view: MessageTextView
    ) throws -> NSEvent {
        let window = try #require(view.window)
        return try #require(
            NSEvent.mouseEvent(
                with: type, location: view.convert(point, to: nil), modifierFlags: [],
                timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1))
    }

    // Down and up on the word, through the text view's own handling.
    func press(_ shown: String, in view: MessageTextView) throws {
        try settleEvents(in: try #require(view.window))
        let word = try glyphs(of: shown, in: view)
        let point = NSPoint(x: word.midX, y: word.midY)
        NSApp.postEvent(try click(.leftMouseUp, at: point, in: view), atStart: false)
        view.mouseDown(with: try click(.leftMouseDown, at: point, in: view))
    }

    @Test
    func aRightClickOnAHiddenSpoilerKeepsItHidden() throws {
        let drawing = MessageTextDrawingTests()
        let view = drawing.shown("a ||secret|| b", appearance: .darkAqua)
        view.messageMenu = { MessageMenu.items(for: row(1, at: .now, content: "x").message) }
        let point = try drawing.insidePill(of: "secret", in: view)
        let before = view.selectedRange()

        let menu = try #require(view.menu(for: try click(.rightMouseDown, at: point, in: view)))

        #expect(!menu.items.contains { $0.title.contains("secret") }, "\(menu.items.map(\.title))")
        #expect(menu.items.contains { $0.title == "Copy Text" })
        #expect(view.selectedRange() == before)
    }

    @Test
    func aClickAnywhereOnAHiddenCodeBlockRevealsIt() throws {
        let drawing = MessageTextDrawingTests()
        let view = drawing.shown("||```\nsecret\n```||", appearance: .darkAqua)
        var clicked: [URL] = []
        view.onLink = { clicked.append($0) }

        try settleEvents(in: try #require(view.window))
        // Ends the text view's tracking loop, should the click reach it.
        NSApp.postEvent(try click(.leftMouseUp, at: NSPoint(x: 300, y: 3), in: view), atStart: false)
        view.mouseDown(with: try click(.leftMouseDown, at: NSPoint(x: 300, y: 3), in: view))

        #expect(clicked == [URL(string: "akari://spoiler/0")])
    }

    @Test
    func voiceOverReadsNoHiddenTextByRange() throws {
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let message = row(1, at: .now, content: "a ||secret|| b").message
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: .now, times: times).text)
        let whole = NSRange(location: 0, length: view.string.utf16.count)

        let string = try #require(view.accessibilityString(for: whole))
        #expect(string.contains("Spoiler") && !string.contains("secret"), "\(string)")
        let attributed = try #require(view.accessibilityAttributedString(for: whole))
        #expect(!attributed.string.contains("secret"))
    }

    func shownWithALink() -> MessageTextView {
        MessageTextDrawingTests().shown(
            "a ||secret|| b [guide](https://example.com) c", appearance: .darkAqua)
    }

    func whole(_ view: MessageTextView) -> NSRange {
        NSRange(location: 0, length: view.string.utf16.count)
    }

    // AppKit's entry points for an assistive app's requests, which in-process calls skip.
    typealias Value = @convention(c) (AnyObject, NSString) -> Unmanaged<AnyObject>?
    typealias ValueWith = @convention(c) (AnyObject, NSString, AnyObject) -> Unmanaged<AnyObject>?

    func entryPoint<T>(_ name: String, as type: T.Type) throws -> T {
        let symbol = try #require(dlsym(UnsafeMutableRawPointer(bitPattern: -2), name), "\(name)")
        return unsafeBitCast(symbol, to: type)
    }

    func text(ofRTF data: Data?) -> String {
        data.flatMap { NSAttributedString(rtf: $0, documentAttributes: nil)?.string } ?? ""
    }

    // The glyphs' bounds in the view.
    func glyphs(of shown: String, in view: MessageTextView) throws -> NSRect {
        let range = (view.string as NSString).range(of: shown)
        try #require(range.location != NSNotFound)
        let layout = try #require(view.layoutManager)
        let glyphs = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        return layout.boundingRect(forGlyphRange: glyphs, in: try #require(view.textContainer))
            .offsetBy(dx: view.textContainerOrigin.x, dy: view.textContainerOrigin.y)
    }

    func onScreen(_ shown: String, in view: MessageTextView) throws -> NSPoint {
        let word = try glyphs(of: shown, in: view)
        return try #require(view.window).convertPoint(
            toScreen: view.convert(NSPoint(x: word.midX, y: word.midY), to: nil))
    }

    @Test
    func voiceOverReadsNoHiddenTextInTheSelection() throws {
        let view = shownWithALink()
        view.selectAll(nil)

        let selected = try #require(view.accessibilitySelectedText())

        #expect(selected.contains("Spoiler") && !selected.contains("secret"), "\(selected)")
    }

    @Test
    func voiceOverReadsNoHiddenTextInRichText() throws {
        let view = shownWithALink()

        let rtf = text(ofRTF: view.accessibilityRTF(for: whole(view)))

        #expect(rtf.contains("Spoiler") && !rtf.contains("secret"), "\(rtf)")
    }

    @Test
    func voiceOverReadsNoHiddenTextInPartOfASpoiler() throws {
        let view = shownWithALink()
        let part = (view.string as NSString).range(of: "cre")

        #expect(view.accessibilityString(for: part) == "Spoiler")
        #expect(view.accessibilityAttributedString(for: part)?.string == "Spoiler")
    }

    @Test
    func aHiddenSpoilersLinkElementIsNamedSpoilerAndRevealsIt() throws {
        let view = shownWithALink()
        var clicked: [URL] = []
        view.onLink = { clicked.append($0) }

        let children = try #require(view.accessibilityChildren())
        let elements = children.compactMap { $0 as? NSAccessibilityElement }
        #expect(elements.count == children.count)
        let names = elements.flatMap { element in
            [element.accessibilityTitle(), element.accessibilityLabel(),
             element.accessibilityValue() as? String].compactMap { $0 }
        }
        #expect(!names.contains { $0.contains("secret") }, "\(names)")
        #expect(names.contains("guide"))

        let spoiler = try #require(elements.first { $0.accessibilityLabel() == "Spoiler" })
        #expect(spoiler.accessibilityRole() == .link)
        #expect(spoiler.accessibilityFrame().contains(try onScreen("secret", in: view)))
        #expect(spoiler.accessibilityPerformPress())
        #expect(clicked == [URL(string: "akari://spoiler/0")])
    }

    @Test
    func anAssistiveAppReadsNoHiddenText() throws {
        let view = shownWithALink()
        view.selectAll(nil)
        let value = try entryPoint("NSAccessibilityEntryPointValueForAttribute", as: Value.self)
        let valueWith = try entryPoint(
            "NSAccessibilityEntryPointValueForAttributeWithParameter", as: ValueWith.self)
        let range = NSValue(range: whole(view))

        let texts = [
            value(view, "AXValue")?.takeUnretainedValue() as? String,
            value(view, "AXSelectedText")?.takeUnretainedValue() as? String,
            valueWith(view, "AXStringForRange", range)?.takeUnretainedValue() as? String,
            (valueWith(view, "AXAttributedStringForRange", range)?.takeUnretainedValue()
                as? NSAttributedString)?.string,
            text(ofRTF: valueWith(view, "AXRTFForRange", range)?.takeUnretainedValue() as? Data),
        ]
        var names: [String] = []
        for attribute in ["AXChildren", "AXChildrenInNavigationOrder"] {
            let children = value(view, attribute as NSString)?.takeUnretainedValue() as? [AnyObject]
            for child in try #require(children) {
                for name in ["AXTitle", "AXDescription", "AXValue", "AXHelp"] {
                    names += [value(child, name as NSString)?.takeUnretainedValue() as? String]
                        .compactMap { $0 }
                }
            }
        }

        #expect(texts.allSatisfy { $0?.contains("Spoiler") == true }, "\(texts)")
        #expect(!texts.contains { $0?.contains("secret") == true }, "\(texts)")
        #expect(names.filter { $0 == "Spoiler" }.count == 2, "\(names)")
        #expect(!names.contains { $0.contains("secret") }, "\(names)")
    }

    @Test
    func hoveringAHiddenSpoilerFindsItsLinkElement() throws {
        let view = shownWithALink()

        let hit = try #require(
            view.accessibilityHitTest(try onScreen("secret", in: view)) as? NSAccessibilityElement)

        #expect(hit.accessibilityLabel() == "Spoiler")
        #expect(hit.accessibilityTitle() == nil)
    }

    @Test
    func lookUpFindsNoHiddenWordAnywhereOnIt() throws {
        let view = MessageTextDrawingTests().shown("a ||secret|| b", appearance: .darkAqua)
        let word = try glyphs(of: "secret", in: view)

        for x in stride(from: word.minX + 0.5, to: word.maxX, by: 1) {
            #expect(view.looksUpHiddenText(at: NSPoint(x: x, y: word.midY)), "at \(x)")
        }
        let shown = try glyphs(of: "b", in: view)
        #expect(!view.looksUpHiddenText(at: NSPoint(x: shown.midX, y: shown.midY)))
    }

    @Test
    func lookUpOnASelectionWithAHiddenSpoilerFindsIt() throws {
        let view = MessageTextDrawingTests().shown("a ||secret|| b", appearance: .darkAqua)
        view.selectAll(nil)
        let shown = try glyphs(of: "b", in: view)

        #expect(view.looksUpHiddenText(at: NSPoint(x: shown.midX, y: shown.midY)))
    }

    @Test
    func voiceOverSaysSpoiler() throws {
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let message = row(1, at: .now, content: "a ||secret|| b").message
        let view = MessageTextView()
        view.show(MessageText.build(message, revealed: [], now: .now, times: times).text)

        let hidden = try #require(view.accessibilityValue())
        #expect(hidden.contains("Spoiler") && !hidden.contains("secret"), "\(hidden)")

        view.show(MessageText.build(message, revealed: [0], now: .now, times: times).text)
        #expect(view.accessibilityValue()?.contains("secret") == true)
    }
}
