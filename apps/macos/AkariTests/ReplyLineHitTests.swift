import AkariKit
import AppKit
import Testing

@testable import Akari

// What a click and the pointer find in a reply's line: the window's own hit test, not a call on
// the button.
@MainActor
struct ReplyLineHitTests {
    let list = BarList()

    func cell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    // The view the window gives a click at a point of `view`.
    func hit(_ point: NSPoint, in view: NSView) throws -> NSView? {
        let content = try #require(list.window.contentView)
        let place = view.convert(point, to: nil)
        return content.hitTest(content.superview?.convert(place, from: nil) ?? place)
    }

    @Test
    func theTextItsSymbolAndTheSpineAreTheButtonsThemselves() throws {
        list.show([replyRow(5, at: list.noon, content: "an answer", target("a picture", attachments: true))])
        let cell = try cell(5)
        let content = cell.replyLine.content
        let symbol = try #require(content.trailingFrame)

        #expect(try hit(NSPoint(x: content.textFrame.midX, y: 9), in: content) === content)
        #expect(try hit(NSPoint(x: symbol.midX, y: symbol.midY), in: content) === content)
        #expect(try hit(NSPoint(x: 6, y: 16), in: cell.spine) === cell.spine)
        #expect(try hit(NSPoint(x: 30, y: 2), in: cell.spine) === cell.spine, "all of its box")
    }

    @Test
    func theNameAndTheSmallAvatarAreNoButtons() throws {
        list.show([replyRow(5, at: list.noon, content: "an answer", target())])
        let line = try cell(5).replyLine
        let name = try #require(line.nameFrame)

        #expect(!(try hit(NSPoint(x: name.midX, y: name.midY), in: line) is NSButton))
        #expect(!(try hit(NSPoint(x: 8, y: 9), in: line) is NSButton))
    }

    @Test
    func aClickGoesToWhatTheWindowFindsAtTheText() throws {
        list.show([replyRow(5, at: list.noon, content: "an answer", target("a picture", attachments: true))])
        let content = try cell(5).replyLine.content
        let symbol = try #require(content.trailingFrame)

        (try hit(NSPoint(x: content.textFrame.midX, y: 9), in: content) as? NSButton)?.performClick(nil)
        (try hit(NSPoint(x: symbol.midX, y: symbol.midY), in: content) as? NSButton)?.performClick(nil)

        #expect(list.actions.calls.filter { $0.hasPrefix("jump") } == ["jump to target of 5", "jump to target of 5"])
    }

    @Test
    func theNameIsTheStrongTextAtTwoThirds() throws {
        list.show([replyRow(5, at: list.noon, content: "an answer", target())])
        let name = try #require(try cell(5).replyLine.nameText)

        #expect(name.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor === Palette.replyName)
        for appearance in [NSAppearance.Name.darkAqua, .aqua] {
            var color: NSColor?
            NSAppearance(named: appearance)?.performAsCurrentDrawingAppearance {
                color = NSColor(cgColor: Palette.replyName.cgColor)?.usingColorSpace(.sRGB)
            }
            var strong: NSColor?
            NSAppearance(named: appearance)?.performAsCurrentDrawingAppearance {
                strong = NSColor(cgColor: Palette.textStrong.cgColor)?.usingColorSpace(.sRGB)
            }
            let (got, base) = (try #require(color), try #require(strong))
            #expect(abs(got.alphaComponent - 0.64) < 0.001)
            #expect(abs(got.redComponent - base.redComponent) < 0.001)
            #expect(abs(got.blueComponent - base.blueComponent) < 0.001)
        }
    }
}
