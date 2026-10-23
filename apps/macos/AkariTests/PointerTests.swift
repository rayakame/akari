import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// A shield over the window, as a popout or modal raises one.
@MainActor
func raiseShield(in window: NSWindow) async throws -> PointerShield {
    let content = try #require(window.contentView)
    let shield = PointerShield(frame: content.bounds)
    content.addSubview(shield)
    // It tells the window's views on the next turn.
    try await Task.sleep(for: .milliseconds(20))
    return shield
}

@MainActor
func lowerShield(_ shield: PointerShield) async throws {
    shield.removeFromSuperview()
    try await Task.sleep(for: .milliseconds(20))
}

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

// Discord's message text is a web page's: the I-beam over the text itself, the pointing hand over
// links, the arrow over the rest of the row (Chris: the I-beam showed over a message's empty
// area).
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

    // NSTextView's own rect is its whole visible area; Akari's cover the text and links only.
    @Test
    func theCursorRectsCoverTheTextAndItsLinks() throws {
        let view = linked()
        let areas = view.shownCursorAreas
        let word = try middle("short", in: view)
        let at = { (point: NSPoint) in areas.last { $0.rect.contains(point) }?.cursor }

        #expect(at(word) == .iBeam)
        #expect(at(try middle("link", in: view)) == .pointingHand)
        #expect(at(NSPoint(x: 350, y: word.y)) == nil)
    }

    // The pointer coming into the view shows what's under it, not NSTextView's I-beam everywhere.
    @Test
    func comingInShowsTheCursorForThePoint() throws {
        let view = linked()
        let word = try middle("short", in: view)

        NSCursor.iBeam.set()
        let after = NSPoint(x: 350, y: word.y)
        view.cursorUpdate(with: try pointerEvent(.cursorUpdate, at: after, in: view))
        #expect(NSCursor.current == .arrow)

        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: word, in: view))
        #expect(NSCursor.current == .iBeam)
        let link = try middle("link", in: view)
        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: link, in: view))
        #expect(NSCursor.current == .pointingHand)
    }

    // Chris: with a popout or the modal open, the messages behind still showed the I-beam.
    @Test
    func behindAShieldTheTextChangesNoCursorAndTakesNoFocus() async throws {
        let view = linked()
        let word = try middle("short", in: view)
        let window = try #require(view.window)
        _ = try await raiseShield(in: window)

        NSCursor.arrow.set()
        view.cursorUpdate(with: try pointerEvent(.cursorUpdate, at: word, in: view))
        let link = try middle("link", in: view)
        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: link, in: view))

        #expect(NSCursor.current == .arrow)
        #expect(view.shownCursorAreas.isEmpty)
        #expect(!view.acceptsFirstResponder)
        // The window takes the focus the view refuses, and says yes.
        window.makeFirstResponder(view)
        #expect(window.firstResponder !== view)
    }

    @Test
    func aTextWithTheFocusGivesItUpWhenAShieldRises() async throws {
        let view = linked()
        let window = try #require(view.window)
        #expect(window.makeFirstResponder(view))

        let shield = try await raiseShield(in: window)

        #expect(window.firstResponder !== view)
        try await lowerShield(shield)
        window.makeFirstResponder(view)
        #expect(window.firstResponder === view)
    }

    @Test
    func behindAShieldALinkGetsNoUnderline() async throws {
        let text = NSMutableAttributedString(
            attributedString: CellText.body("See all threads.", color: .labelColor))
        text.addAttributes(
            [.link: URL(string: "akari://threads")!, MessageTextView.underlinesOnHover: true],
            range: NSRange(location: 8, length: 7))
        let view = texts.shown(text, width: 400)
        let threads = try middle("threads", in: view)
        let index = (view.string as NSString).range(of: "threads").location
        let underlined = {
            view.layoutManager?.temporaryAttribute(
                .underlineStyle, atCharacterIndex: index, effectiveRange: nil) != nil
        }
        let window = try #require(view.window)

        let shield = try await raiseShield(in: window)
        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: threads, in: view))
        #expect(!underlined())

        try await lowerShield(shield)
        view.mouseMoved(with: try pointerEvent(.mouseMoved, at: threads, in: view))
        #expect(underlined())
    }
}

// Behind a popout or modal, nothing in the message list hovers (Chris: the rows still lit up).
@MainActor
struct HoverBehindShieldTests {
    let list = MessageTableTests()

    @Test
    func noRowHoversWhileAShieldIsUpAndTheHoverComesBackAfter() async throws {
        list.show(list.messages(20))
        list.pointer.location = NSPoint(x: 300, y: 200)
        list.controller.pointerMoved()
        let under = list.hoveredRows
        try #require(!under.isEmpty)

        let shield = try await raiseShield(in: list.window)
        #expect(list.hoveredRows.isEmpty, "gone without the pointer moving")
        list.controller.pointerMoved()
        #expect(list.hoveredRows.isEmpty)

        try await lowerShield(shield)
        #expect(list.hoveredRows == under)
    }

    @Test
    func aThreadBoxDoesntHoverBehindAShield() async throws {
        let boxes = ThreadBoxViewTests()
        let (_, box) = try boxes.shown()
        let window = try #require(box.window)
        #expect(box.cursorRect == box.boxFrame)

        let shield = try await raiseShield(in: window)
        box.mouseEntered(with: try pointerEvent(.mouseEntered, at: .zero, in: box))
        #expect(!box.hovered)
        #expect(box.cursorRect == nil)

        try await lowerShield(shield)
        box.mouseEntered(with: try pointerEvent(.mouseEntered, at: .zero, in: box))
        #expect(box.hovered)
    }
}

@MainActor
struct ComposerBehindShieldTests {
    @Test
    func theComposerShowsNoIBeamTakesNoFocusAndGivesItUp() async throws {
        let composer = ComposerTextViewTests()
        let window = composer.window
        let text = composer.textView
        #expect(window.firstResponder === text)

        let shield = try await raiseShield(in: window)
        #expect(window.firstResponder !== text)
        window.makeFirstResponder(text)
        #expect(window.firstResponder !== text)
        text.takeFocus()
        #expect(window.firstResponder !== text)
        NSCursor.arrow.set()
        let inside = NSPoint(x: 20, y: 20)
        text.cursorUpdate(with: try pointerEvent(.cursorUpdate, at: inside, in: text))
        #expect(NSCursor.current == .arrow)

        try await lowerShield(shield)
        text.takeFocus()
        #expect(window.firstResponder === text)
    }
}

// A cursor area that may be behind a shield or in front of it (a card's in the popout).
@MainActor
struct CursorAreaTests {
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled],
        backing: .buffered, defer: false)

    func area(_ cursor: NSCursor) -> CursorArea.Area {
        let content = NSView(frame: NSRect(x: 0, y: 0, width: 400, height: 300))
        window.contentView = content
        let area = CursorArea.Area(frame: NSRect(x: 20, y: 20, width: 100, height: 40))
        area.cursor = cursor
        content.addSubview(area)
        return area
    }

    @Test
    func anAreaBehindAShieldShowsNothingAndOneInFrontOfItShowsItsCursor() async throws {
        let behind = area(.pointingHand)
        #expect(behind.shownRect == behind.bounds)

        let shield = try await raiseShield(in: window)
        #expect(behind.shownRect == nil)

        // A card above the shield takes the clicks; its area lets them through to it.
        let card = NSView(frame: NSRect(x: 200, y: 20, width: 150, height: 60))
        window.contentView?.addSubview(card, positioned: .above, relativeTo: shield)
        let inFront = CursorArea.Area(frame: NSRect(x: 10, y: 10, width: 100, height: 40))
        inFront.cursor = .iBeam
        card.addSubview(inFront)
        #expect(inFront.shownRect == inFront.bounds)
    }
}
