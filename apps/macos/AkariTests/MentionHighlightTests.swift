import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct MentionHighlightTests {
    let drawing = MessageTextDrawingTests()

    // The chat's background with `tokens` over it in order, drawn as the cell draws.
    final class Swatch: NSView {
        let tokens: [NSColor]

        init(_ tokens: [NSColor]) {
            self.tokens = tokens
            super.init(frame: NSRect(x: 0, y: 0, width: 10, height: 10))
        }

        required init?(coder: NSCoder) {
            nil
        }

        override func draw(_ dirtyRect: NSRect) {
            Palette.chat.setFill()
            bounds.fill()
            for token in tokens {
                token.setFill()
                bounds.fill(using: .sourceOver)
            }
        }
    }

    func cell(
        mentioned: Bool, startsGroup: Bool = true, appearance: NSAppearance.Name = .darkAqua
    ) -> MessageCell {
        var message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "hey").message
        message.mentionsMe = mentioned
        let backdrop = MessageTextDrawingTests.Backdrop(
            frame: NSRect(x: 0, y: 0, width: 600, height: 80))
        let cell = MessageCell()
        configure(cell, message, startsGroup: startsGroup)
        cell.translatesAutoresizingMaskIntoConstraints = true
        cell.frame = backdrop.bounds
        backdrop.addSubview(cell)
        drawing.window.appearance = NSAppearance(named: appearance)
        drawing.window.contentView?.addSubview(backdrop)
        cell.layoutSubtreeIfNeeded()
        return cell
    }

    func configure(_ cell: MessageCell, _ message: Message, startsGroup: Bool = true) {
        cell.configure(
            message, body: NSAttributedString(string: message.content), startsGroup: startsGroup,
            groupTime: "12:00", shortTime: "12:00", fullDate: "")
    }

    // `y` from the cell's top.
    func pixel(_ cell: MessageCell, x: CGFloat, y: CGFloat = 40) throws -> NSColor {
        try drawing.pixel(cell, at: NSPoint(x: x, y: y))
    }

    func matches(_ drawn: NSColor, _ tokens: [NSColor]) throws -> Bool {
        let swatch = Swatch(tokens)
        drawing.window.contentView?.addSubview(swatch)
        defer { swatch.removeFromSuperview() }
        let expected = try drawing.render(swatch, at: NSPoint(x: 5, y: 5))
        return abs(drawn.redComponent - expected.redComponent) < 0.02
            && abs(drawn.greenComponent - expected.greenComponent) < 0.02
            && abs(drawn.blueComponent - expected.blueComponent) < 0.02
    }

    @Test
    func aMentioningMessageIsHighlighted() throws {
        let cell = cell(mentioned: true)

        #expect(try matches(try pixel(cell, x: 1), [Palette.mentionedBar]))
        #expect(try matches(try pixel(cell, x: 560), [Palette.mentionedBackground]))
        #expect(try !matches(try pixel(cell, x: 3), [Palette.mentionedBar]))
    }

    @Test
    func otherMessagesAreNot() throws {
        let cell = cell(mentioned: true)
        var message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "hey").message
        message.mentionsMe = false

        configure(cell, message)

        #expect(try matches(try pixel(cell, x: 1), []))
        #expect(try matches(try pixel(cell, x: 560), []))
    }

    @Test
    func hoverDrawsOverTheHighlight() throws {
        let cell = cell(mentioned: true)

        cell.isHovered = true

        #expect(
            try matches(
                try pixel(cell, x: 560), [Palette.mentionedBackground, Palette.hoverBackground]))
    }

    // The official client keeps the 16 before a group outside the message, so two highlighted
    // groups in a row show a gap.
    @Test
    func theHighlightLeavesOutTheSpaceBeforeAGroup() throws {
        let cell = cell(mentioned: true)

        #expect(try matches(try pixel(cell, x: 560, y: 15), []))
        #expect(try matches(try pixel(cell, x: 1, y: 15), []))
        #expect(try matches(try pixel(cell, x: 560, y: 17), [Palette.mentionedBackground]))
        #expect(try matches(try pixel(cell, x: 1, y: 17), [Palette.mentionedBar]))
    }

    @Test
    func hoverLeavesOutTheSpaceBeforeAGroup() throws {
        let cell = cell(mentioned: false)

        cell.isHovered = true

        #expect(try matches(try pixel(cell, x: 560, y: 15), []))
        #expect(try matches(try pixel(cell, x: 560, y: 17), [Palette.hoverBackground]))
    }

    @Test
    func aContinuationIsHighlightedToItsTop() throws {
        let cell = cell(mentioned: true, startsGroup: false)

        #expect(try matches(try pixel(cell, x: 560, y: 1), [Palette.mentionedBackground]))
    }

    // Where the official client's scrollbar sits.
    @Test
    func theHighlightStopsShortOfTheScrollbar() throws {
        let cell = cell(mentioned: true)
        cell.isHovered = true

        #expect(try matches(try pixel(cell, x: 600 - 15, y: 40), []))
        #expect(
            try matches(
                try pixel(cell, x: 600 - 17, y: 40),
                [Palette.mentionedBackground, Palette.hoverBackground]))
    }

    @Test
    func theBoxLeavesOutTheGroupSpacingAndTheScrollbarGutter() {
        let start = cell(mentioned: false)
        let continuation = cell(mentioned: false, startsGroup: false)
        let fromTop = { (cell: MessageCell) in
            cell.isFlipped ? cell.box.minY : cell.bounds.maxY - cell.box.maxY
        }

        // 600 × 80 cells: 16 less on the right, and 16 less at a group's top.
        #expect(start.box.width == 584)
        #expect(start.box.height == 64)
        #expect(fromTop(start) == 16)
        #expect(continuation.box.width == 584)
        #expect(continuation.box.height == 80)
    }

    // A legacy scroller has a gutter of its own beside the list.
    @Test
    func underALegacyScrollerTheBoxRunsToTheEdge() {
        let cell = cell(mentioned: false)
        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        scrollView.hasVerticalScroller = true
        scrollView.scrollerStyle = .legacy
        let backdrop = cell.superview
        scrollView.documentView = backdrop

        #expect(cell.box.width == 600)
    }

    // A short channel autohides it, which leaves the gutter empty.
    @Test
    func aHiddenLegacyScrollerKeepsTheGutter() throws {
        let cell = cell(mentioned: false)
        let scrollView = NSScrollView(frame: NSRect(x: 0, y: 0, width: 640, height: 200))
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.scrollerStyle = .legacy
        scrollView.documentView = cell.superview
        scrollView.tile()
        try #require(scrollView.verticalScroller?.isHidden == true)

        #expect(cell.box.width == 584)
    }

    @Test
    func anAppearanceChangeRecolorsTheMentionHighlight() throws {
        let cell = cell(mentioned: true)
        let dark = try pixel(cell, x: 560)
        #expect(try matches(dark, [Palette.mentionedBackground]))

        drawing.window.appearance = NSAppearance(named: .aqua)

        let light = try pixel(cell, x: 560)
        #expect(try matches(light, [Palette.mentionedBackground]))
        #expect(abs(dark.redComponent - light.redComponent) > 0.2)
    }
}
