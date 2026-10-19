import AkariKit
import AppKit
import Testing

@testable import Akari

// D122: a notice's time on its text's baseline, one space after it (images 1, 2).
@MainActor
struct NoticeTimeTests {
    let list = MessageTableTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
    }

    func shown() throws -> (cell: NoticeCell, text: MessageTextView, time: String) {
        list.show([row(1, by: author(2, "Mira"), at: noon, kind: .userJoin)])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)
        let text = try #require(list.messageTexts(in: cell).first)
        let time = MessageFormat.groupTime(
            noon, now: berlinTime(2026, 10, 10, 15, 0), calendar: berlin,
            locale: .autoupdatingCurrent)
        return (cell, text, time)
    }

    // The lowest pixel row with ink in `rect`, in points from the view's top.
    func inkBottom(in rect: NSRect, of rep: NSBitmapImageRep) -> CGFloat? {
        let columns = Int(rect.minX * 2)..<Int(rect.maxX * 2)
        let rows = Int(rect.minY * 2)..<Int(rect.maxY * 2)
        return rows.reversed().first { y in
            columns.contains { x in (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 }
        }.map { CGFloat($0 + 1) / 2 }
    }

    @Test
    func aNoticesTimeSitsOnTheTextsBaseline() throws {
        let (cell, text, time) = try shown()
        let rep = try renderedAt2x(cell)

        // "server" and the time's digits have no descenders: their ink ends on the baseline.
        let words = try glyphs(of: "server", in: text, within: cell)
        let digits = try glyphs(of: String(time.prefix(4)), in: text, within: cell)
        let wordsBottom = try #require(inkBottom(in: words, of: rep))
        let digitsBottom = try #require(inkBottom(in: digits, of: rep))

        #expect(abs(wordsBottom - digitsBottom) <= 0.5, "\(wordsBottom) \(digitsBottom)")
    }

    @Test
    func oneSpaceBeforeANoticesTime() throws {
        let (cell, text, time) = try shown()

        #expect(text.string == "Mira joined the server. \(time)")
        let space = (text.string as NSString).range(of: ". ").location + 1
        #expect(
            text.textStorage?.attribute(.font, at: space, effectiveRange: nil) as? NSFont
                == TypeScale.body.nsFont)
        // Measured at 2x: the gap between the period's ink and the time's is about a space.
        let rep = try renderedAt2x(cell)
        let period = try glyphs(of: ".", in: text, within: cell)
        let digits = try glyphs(of: String(time.prefix(1)), in: text, within: cell)
        let inkRight = (Int(period.minX * 2)..<Int(period.maxX * 2) + 2).last { x in
            (Int(period.minY * 2)..<Int(period.maxY * 2)).contains { y in
                (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5
            }
        }
        let inkLeft = (Int(digits.minX * 2) - 2..<Int(digits.maxX * 2)).first { x in
            (Int(digits.minY * 2)..<Int(digits.maxY * 2)).contains { y in
                (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5
            }
        }
        let gap = CGFloat(try #require(inkLeft) - (try #require(inkRight)) - 1) / 2
        let spaceWidth = (" " as NSString).size(withAttributes: [.font: TypeScale.body.nsFont])
            .width
        #expect(gap > spaceWidth - 1 && gap < spaceWidth + 2.5, "\(gap) \(spaceWidth)")
    }
}
