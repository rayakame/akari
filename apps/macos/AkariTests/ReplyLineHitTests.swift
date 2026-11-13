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

    // An Option-click marks unread from a reply too, its line's name included; the line's text
    // and the spine are buttons and keep their click.
    @Test
    func anOptionClickMarksAReplyUnreadOutsideItsLinesButtons() throws {
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "the original message"),
            replyRow(5, at: list.noon + 600, content: "an answer", target()),
        ])
        list.actions.marksUnread = [1, 5]
        list.pointer.location = NSPoint(x: 10, y: 10)
        let cell = try cell(5)
        let line = cell.replyLine
        let name = try #require(line.nameFrame)
        func click(_ point: NSPoint, in view: NSView) throws -> Bool {
            list.controller.optionClick(try list.click(.option, at: list.table.convert(point, from: view)))
        }

        #expect(!(try click(NSPoint(x: line.content.textFrame.midX, y: 9), in: line.content)))
        #expect(!(try click(NSPoint(x: 6, y: 16), in: cell.spine)))
        #expect(!list.actions.calls.contains { $0.hasPrefix("mark unread") })

        #expect(try click(NSPoint(x: name.midX, y: name.midY), in: line))
        #expect(list.actions.calls.filter { $0.hasPrefix("mark unread") } == ["mark unread 5"])
    }

    // The message being answered can be marked unread: the mark leaves the row's tint.
    @Test
    func anOptionClickOnTheAnsweredRowMarksItUnreadAndKeepsItsTint() throws {
        let rows = [
            row(1, by: author(2, "Ray"), at: list.noon, content: "the original message"),
            row(2, by: author(3, "Kim"), at: list.noon + 600, content: "another"),
        ]
        list.actions.marksUnread = [1, 2]
        list.actions.bars[1] = (BarList.plain, BarList.shifted)
        list.actions.bars[2] = (BarList.plain, BarList.shifted)
        list.pointer.location = NSPoint(x: 10, y: 10)
        list.controller.show(
            MessageTableState(rows: rows, reachedOldest: true, replyingTo: MessageId(rawValue: 1)))
        list.window.layoutIfNeeded()
        let box = try list.box(of: 1)

        #expect(list.controller.optionClick(try list.click(.option, at: NSPoint(x: box.midX, y: box.midY))))
        list.controller.show(
            MessageTableState(
                rows: rows, reachedOldest: true, newMessages: MessageId(rawValue: 1),
                replyingTo: MessageId(rawValue: 1)))
        list.window.layoutIfNeeded()

        #expect(list.actions.calls.filter { $0.hasPrefix("mark unread") } == ["mark unread 1"])
        #expect(try cell(1).isReplyTarget)
        #expect(try !cell(2).isReplyTarget)
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
