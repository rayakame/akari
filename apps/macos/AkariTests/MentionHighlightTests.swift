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

    func cell(mentioned: Bool, appearance: NSAppearance.Name = .darkAqua) -> MessageCell {
        var message = row(1, at: berlinTime(2026, 10, 10, 12, 0), content: "hey").message
        message.mentionsMe = mentioned
        let backdrop = MessageTextDrawingTests.Backdrop(
            frame: NSRect(x: 0, y: 0, width: 600, height: 80))
        let cell = MessageCell()
        configure(cell, message)
        cell.translatesAutoresizingMaskIntoConstraints = true
        cell.frame = backdrop.bounds
        backdrop.addSubview(cell)
        drawing.window.appearance = NSAppearance(named: appearance)
        drawing.window.contentView?.addSubview(backdrop)
        cell.layoutSubtreeIfNeeded()
        return cell
    }

    func configure(_ cell: MessageCell, _ message: Message) {
        cell.configure(
            message, body: NSAttributedString(string: message.content), startsGroup: true,
            groupTime: "Today at 12:00", shortTime: "12:00", fullDate: "")
    }

    func pixel(_ cell: MessageCell, x: CGFloat) throws -> NSColor {
        try drawing.pixel(cell, at: NSPoint(x: x, y: 40))
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
