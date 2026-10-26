import AkariKit
import AppKit
import Testing

@testable import Akari

// A follow-up's hover time sits where a plain first line's baseline would be, the same distance
// below the row's top whatever the row holds, as in the official client: not on a jumbo line's
// baseline, not above a sticker. Rendered at 2x.
@MainActor @Suite
final class HoverTimeTests {
    let noon = Date(timeIntervalSince1970: 1_800_000_000)
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))
    var retained: [NSWindow] = []

    // Below the row's top, in points: 3 to the content, then a 16 pt line's baseline on its
    // 22 pt line.
    static let baseline: CGFloat = 19

    struct Ink {
        let top: CGFloat
        let bottom: CGFloat
    }

    // Where the time's glyphs are in a hovered follow-up, from the row's top: the column left of
    // the text holds nothing else.
    func timeInk(content: String, stickers: [String] = [], attachments: [String] = []) throws -> Ink {
        let message = row(
            2, at: noon, content: content, attachments: attachments, stickers: stickers,
            stickerFormat: .png
        ).message
        let body = MessageText.build(message, revealed: [], now: noon, times: times).text
        let width: CGFloat = 600
        let height = MessageCell.height(message, body: body, startsGroup: false, width: width)
        let window = ScaledWindow(scale: 2)
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

        // The hover fill is translucent; the digits are opaque.
        let column = 0..<Int(60 * scale)
        let inked = (0..<rep.pixelsHigh).filter { y in
            column.contains { x in (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 }
        }
        let first = try #require(inked.first, "no time in \(content.debugDescription), \(stickers)")
        let last = try #require(inked.last)
        return Ink(top: CGFloat(first) / scale, bottom: CGFloat(last + 1) / scale)
    }

    @Test(arguments: [
        ("text", "hello there", [String](), [String]()),
        ("jumbo", "<:wave:5>", [], []),
        ("sticker", "", ["wave"], []),
        ("mixed", "hi <:wave:5> and :)", ["wave"], []),
        ("attachment", "", [], ["cat.png"]),
    ])
    func theTimeSitsOnAFirstLinesBaseline(
        kind: String, content: String, stickers: [String], attachments: [String]
    ) throws {
        let ink = try timeInk(content: content, stickers: stickers, attachments: attachments)

        // Digits sit on the baseline; antialiasing reaches a little past it.
        #expect(
            ink.bottom >= Self.baseline && ink.bottom <= Self.baseline + 1,
            "\(kind): the time's glyphs end at \(ink.bottom)")
        // Whole, inside the row: an 11 pt digit is about 8 tall.
        #expect(ink.top > 0, "\(kind): starts at \(ink.top)")
        #expect(ink.bottom - ink.top >= 7, "\(kind): only \(ink.bottom - ink.top) tall")
    }
}
