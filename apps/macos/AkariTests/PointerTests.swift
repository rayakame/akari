import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
func pointerEvent(_ type: NSEvent.EventType, at point: NSPoint, in view: NSView) throws -> NSEvent
{
    let location = view.convert(point, to: nil)
    let number = view.window?.windowNumber ?? 0
    if type == .mouseMoved {
        return try #require(
            NSEvent.mouseEvent(
                with: type, location: location, modifierFlags: [], timestamp: 0,
                windowNumber: number, context: nil, eventNumber: 0, clickCount: 0, pressure: 0))
    }
    return try #require(
        NSEvent.enterExitEvent(
            with: type, location: location, modifierFlags: [], timestamp: 0, windowNumber: number,
            context: nil, eventNumber: 0, trackingNumber: 0, userData: nil))
}

// What a message's text shows the pointer, as over a web page's text: the I-beam over the text
// itself, the pointing hand over links, the arrow around it (Chris: the I-beam showed over a
// message's empty area). The list asks; the text view sets nothing itself.
@MainActor
struct MessageCursorTests {
    let texts = MessageTextViewTests()

    func linked() -> MessageTextView {
        let text = NSMutableAttributedString(
            attributedString: CellText.body("short link here", color: .labelColor))
        text.addAttribute(
            .link, value: URL(string: "https://example.com")!,
            range: NSRange(location: 6, length: 4))
        return texts.shown(text, width: 400)
    }

    func middle(_ part: String, in view: MessageTextView) throws -> NSPoint {
        let rect = try glyphs(of: part, in: view, within: view)
        return NSPoint(x: rect.midX, y: rect.midY)
    }

    @Test
    func theIBeamShowsOverTheTextOnly() throws {
        let view = linked()
        let word = try middle("short", in: view)

        #expect(view.cursor(at: word) == .iBeam)
        #expect(view.cursor(at: try middle("here", in: view)) == .iBeam)
        #expect(view.cursor(at: try middle("link", in: view)) == .pointingHand)
        #expect(view.cursor(at: NSPoint(x: 350, y: word.y)) == .arrow)
    }

    @Test
    func aBlankLineAndTheSpaceAfterALineShowTheArrow() throws {
        let view = texts.shown(
            CellText.body("first line that is long\n\nlast", color: .labelColor), width: 400)
        let first = try glyphs(of: "first line that is long", in: view, within: view)
        let last = try glyphs(of: "last", in: view, within: view)

        #expect(view.cursor(at: NSPoint(x: first.midX, y: first.midY)) == .iBeam)
        #expect(view.cursor(at: NSPoint(x: first.midX, y: (first.maxY + last.minY) / 2)) == .arrow)
        #expect(view.cursor(at: NSPoint(x: last.maxX + 40, y: last.midY)) == .arrow)
    }

    // NSTextView's own cursor area would set the I-beam anywhere in the view, and race its
    // neighbours' between rows; only its tooltips' area stays.
    @Test
    func theTextViewSetsNoCursorItself() throws {
        let view = linked()
        let word = try middle("short", in: view)
        let cursor = NSTrackingArea(
            rect: .zero, options: [.cursorUpdate, .mouseMoved, .activeInKeyWindow, .inVisibleRect],
            owner: view)
        let tips = NSTrackingArea(
            rect: .zero, options: [.mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
            owner: view)

        view.addTrackingArea(cursor)
        view.addTrackingArea(tips)

        #expect(!view.trackingAreas.contains(cursor))
        #expect(view.trackingAreas.contains(tips))
        NSCursor.arrow.set()
        view.cursorUpdate(with: try pointerEvent(.cursorUpdate, at: word, in: view))
        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: word, in: view))
        #expect(NSCursor.current == .arrow)
    }
}

// The message list's one cursor authority: its own tracking area, and what's under the pointer
// decides, on every move, scroll and update; nothing in its rows sets the cursor.
@MainActor
struct ListCursorTests {
    let list = MessageTableTests()

    // Ray's link, Mira's short line, a message with a thread's box, a failed one with Retry.
    func shown() {
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "see https://example.com now"),
            row(2, by: author(3), at: list.noon + 600, content: "short"),
            row(
                3, by: author(2, "Ray"), at: list.noon + 1200, content: "test", hasThread: true,
                threadBox: ThreadBox(
                    threadId: ChannelId(rawValue: 20), name: "t1", link: "16 Messages ›",
                    bottom: .noRecentMessages, time: list.noon)),
            row(4, at: list.noon + 1800, delivery: .failed),
        ])
    }

    func text(of raw: UInt64) throws -> MessageTextView {
        try #require(list.messageTexts(in: try list.cell(of: raw)).first)
    }

    // `part` of message `raw`'s text, in window coordinates.
    func point(of part: String, in raw: UInt64) throws -> NSPoint {
        let view = try text(of: raw)
        let rect = try glyphs(of: part, in: view, within: view)
        return view.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    }

    func box(in raw: UInt64 = 3) throws -> ThreadBoxView {
        try #require(ThreadBoxViewTests.boxView(in: try list.cell(of: raw)))
    }

    func center(of view: NSView, in rect: NSRect? = nil) -> NSPoint {
        let rect = rect ?? view.bounds
        return view.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    }

    func retry() throws -> NSButton {
        let cell = try list.cell(of: 4)
        func buttons(_ view: NSView) -> [NSButton] {
            ((view as? NSButton).map { [$0] } ?? []) + view.subviews.flatMap(buttons)
        }
        return try #require(buttons(cell).first { $0.title == "Retry" })
    }

    @Test
    func itPicksTheCursorForWhatsUnderThePointer() throws {
        shown()
        let short = try point(of: "short", in: 2)

        #expect(list.controller.cursor(at: short) == .iBeam)
        #expect(list.controller.cursor(at: NSPoint(x: short.x + 300, y: short.y)) == .arrow)
        #expect(list.controller.cursor(at: try point(of: "example", in: 1)) == .pointingHand)
        let box = try box()
        #expect(list.controller.cursor(at: center(of: box, in: box.boxFrame)) == .pointingHand)
        #expect(list.controller.cursor(at: center(of: try retry())) == .pointingHand)
        // The space before a group belongs to no message.
        let second = list.table.rect(ofRow: list.index(of: 2))
        let gap = list.table.convert(NSPoint(x: 300, y: second.minY + 4), to: nil)
        #expect(list.controller.cursor(at: gap) == .arrow)
        #expect(list.controller.cursor(at: NSPoint(x: -20, y: short.y)) == nil)
    }

    @Test
    func itSetsTheCursorOnEveryMoveBetweenRows() throws {
        shown()
        let short = try point(of: "short", in: 2)
        let points = [
            short, NSPoint(x: short.x + 300, y: short.y), try point(of: "example", in: 1), short,
            center(of: try retry()),
        ]

        for point in points {
            list.pointer.location = point
            list.controller.pointerMoved()
            #expect(NSCursor.current == list.controller.cursor(at: point), "\(point)")
        }
    }

    // A still pointer, the list moving under it: the cursor follows what's now there.
    @Test
    func scrollingUnderAStillPointerSetsTheCursorForWhatsNowThere() throws {
        list.show(list.messages(80, longText: true))
        let clip = list.controller.scrollView.contentView
        let still = NSPoint(x: 200, y: 200)
        list.pointer.location = still
        list.controller.pointerMoved()
        var seen: Set<String> = []

        for distance in stride(from: CGFloat(7), through: 700, by: 7) {
            clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY - 7))
            list.controller.scrollView.reflectScrolledClipView(clip)
            list.window.layoutIfNeeded()
            let wanted = list.controller.cursor(at: still)
            #expect(NSCursor.current == wanted, "\(distance)")
            seen.insert(wanted == .iBeam ? "text" : "other")
        }
        #expect(seen == ["text", "other"], "the pointer saw text and the space around it")
    }

    @Test
    func rowsArrivingUnderAStillPointerSetTheCursor() throws {
        list.show([row(1, at: list.noon, content: "first")])
        let below = list.table.convert(NSPoint(x: 120, y: list.table.bounds.maxY - 30), to: nil)
        list.pointer.location = below
        list.controller.pointerMoved()

        list.show(list.messages(30, longText: true))

        #expect(NSCursor.current == list.controller.cursor(at: below))
    }

    // The list underlines the hover link under the pointer and lights the thread box under it.
    @Test
    func itUnderlinesTheLinkAndHoversTheBoxUnderThePointer() throws {
        list.show([
            row(
                1, by: author(2, "Ray"), at: list.noon, kind: .threadCreated, content: "t2",
                reference: MessageReference(
                    messageId: nil, channelId: ChannelId(rawValue: 20),
                    guildId: GuildId(rawValue: 1))),
            row(
                3, by: author(2, "Ray"), at: list.noon + 1200, content: "test", hasThread: true,
                threadBox: ThreadBox(
                    threadId: ChannelId(rawValue: 21), name: "t1", link: "16 Messages ›",
                    bottom: .noRecentMessages, time: list.noon)),
        ])
        let line = try text(of: 1)
        let threads = try point(of: "threads", in: 1)
        let index = (line.string as NSString).range(of: "threads").location
        let underlined = {
            line.layoutManager?.temporaryAttribute(
                .underlineStyle, atCharacterIndex: index, effectiveRange: nil) != nil
        }
        let box = try box()

        list.pointer.location = threads
        list.controller.pointerMoved()
        #expect(underlined())
        #expect(!box.hovered)

        list.pointer.location = center(of: box, in: box.boxFrame)
        list.controller.pointerMoved()
        #expect(!underlined())
        #expect(box.hovered)

        list.pointer.location = nil
        list.controller.pointerMoved()
        #expect(!box.hovered)
    }

    // The box sets no cursor and tracks no pointer of its own.
    @Test
    func theThreadBoxLeavesThePointerToTheList() throws {
        shown()
        let box = try box()

        #expect(box.trackingAreas.isEmpty)
        NSCursor.arrow.set()
        box.mouseEntered(with: try pointerEvent(.mouseEntered, at: .zero, in: box))
        #expect(NSCursor.current == .arrow)
        #expect(!box.hovered)
    }

    // The bars over the list (unread, jump, locked, the status strip) show the arrow, even
    // over a message's text.
    @Test
    func whatLiesOverTheListShowsTheArrow() throws {
        shown()
        let short = try point(of: "short", in: 2)
        let scroll = list.controller.scrollView
        let bar = NSView(frame: NSRect(x: 0, y: 0, width: scroll.bounds.width, height: 0))
        let over = scroll.convert(short, from: nil)
        bar.frame = NSRect(x: 0, y: over.y - 15, width: scroll.bounds.width, height: 30)
        scroll.addSubview(bar)

        #expect(list.controller.cursor(at: short) == .arrow)
        list.pointer.location = short
        NSCursor.iBeam.set()
        list.controller.pointerMoved()
        #expect(NSCursor.current == .arrow)
    }
}

