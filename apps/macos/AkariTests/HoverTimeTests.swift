import AkariKit
import AppKit
import Testing

@testable import Akari

// A follow-up's hover time sits in the middle of a plain first line, the same distance below the
// row's top whatever the row holds, as in the official client: centered on a one-line message's
// text, not on a jumbo line, not above a sticker. Rendered at 2x.
@MainActor @Suite
final class HoverTimeTests {
    let noon = Date(timeIntervalSince1970: 1_800_000_000)
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    var retained: [NSWindow] = []

    // Below the row's top, in points: where a one-line follow-up's capitals are centered, 3 to
    // its 22 line and the line's middle.
    static let middle: CGFloat = 13.25

    struct Ink {
        let top: CGFloat
        let bottom: CGFloat
        var middle: CGFloat { (top + bottom) / 2 }
    }

    struct Row {
        let rep: NSBitmapImageRep
        let scale: CGFloat
        let height: CGFloat

        // The glyphs in `columns` (points from the left), from the row's top; antialiased edges
        // count from half cover.
        func ink(_ columns: Range<CGFloat>) -> Ink? {
            let xs = Int(columns.lowerBound * scale)..<Int(columns.upperBound * scale)
            let inked = (0..<rep.pixelsHigh).filter { y in
                xs.contains { x in (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 }
            }
            guard let first = inked.first, let last = inked.last else {
                return nil
            }
            return Ink(top: CGFloat(first) / scale, bottom: CGFloat(last + 1) / scale)
        }
    }

    // The time's column, left of the text, holds nothing else; the hover fill is translucent.
    static let timeColumn: Range<CGFloat> = 0..<60
    static let textColumn: Range<CGFloat> = 72..<300

    func hovered(content: String, stickers: [String] = [], attachments: [String] = []) throws -> Row {
        let message = row(
            2, at: noon, content: content, attachments: attachments, stickers: stickers,
            stickerFormat: .png
        ).message
        let body = MessageText.build(message, revealed: [], now: noon, times: times).text
        let width: CGFloat = 600
        let height = MessageCell.height(message, body: body, startsGroup: false, width: width)
        let window = ScaledWindow.make(scale: 2)
        retained.append(window)
        window.appearance = NSAppearance(named: .darkAqua)
        window.setContentSize(NSSize(width: width, height: height))
        let cell = MessageCell()
        cell.frame = NSRect(x: 0, y: 0, width: width, height: height)
        window.contentView = cell
        cell.configure(
            message, body: body, startsGroup: false, groupTime: "", shortTime: "21:58",
            fullDate: "")
        cell.isHovered = true
        cell.layoutSubtreeIfNeeded()
        let rep = try #require(cell.bitmapImageRepForCachingDisplay(in: cell.bounds))
        cell.cacheDisplay(in: cell.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / width
        #expect(scale == 2)
        return Row(rep: rep, scale: scale, height: height)
    }

    @Test
    func theTimeIsCenteredOnAOneLineMessage() throws {
        let row = try hovered(content: "HELLO")
        let time = try #require(row.ink(Self.timeColumn), "no time")
        let text = try #require(row.ink(Self.textColumn), "no text")

        #expect(abs(time.middle - text.middle) <= 0.5, "time at \(time), text at \(text)")
        #expect(abs(time.middle - row.height / 2) <= 0.5, "time at \(time), row \(row.height) tall")
    }

    @Test(arguments: [
        ("text", "hello there", [String](), [String]()),
        ("jumbo", "<:wave:5>", [], []),
        ("sticker", "", ["wave"], []),
        ("mixed", "hi <:wave:5> and :)", ["wave"], []),
        ("attachment", "", [], ["cat.png"]),
    ])
    func theTimeSitsAtTheSameHeightWhateverTheRowHolds(
        kind: String, content: String, stickers: [String], attachments: [String]
    ) throws {
        let row = try hovered(content: content, stickers: stickers, attachments: attachments)
        let ink = try #require(row.ink(Self.timeColumn), "\(kind): no time")

        #expect(abs(ink.middle - Self.middle) <= 0.5, "\(kind): the time's glyphs are at \(ink)")
        // Whole, inside the row: an 11 pt digit is about 8 tall.
        #expect(ink.top > 0, "\(kind): starts at \(ink.top)")
        #expect(ink.bottom - ink.top >= 7, "\(kind): only \(ink.bottom - ink.top) tall")
    }
}
