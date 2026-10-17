import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct NewMessagesLineTests {
    let list = MessageTableTests()
    let drawing = MessageTextDrawingTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    var controller: MessageTableController { list.controller }
    var table: NSTableView { list.table }

    func line(before raw: UInt64) -> MessageId {
        MessageId(rawValue: raw)
    }

    var lineRow: Int? {
        controller.timeline.items.firstIndex { if case .newMessages = $0 { true } else { false } }
    }

    var contentHeight: CGFloat {
        table.rect(ofRow: table.numberOfRows - 1).maxY
    }

    @Test
    func theLineMovesNoText() throws {
        let page = list.page

        // Opening an unread channel: one update brings the rows and the line, pinned.
        controller.show(MessageTableState(rows: page, newMessages: line(before: 170)))
        let shown = try #require(lineRow)
        #expect(controller.timeline.items[shown + 1].id == .message(MessageId(rawValue: 170)))
        #expect(table.rect(ofRow: shown).height == NewMessagesDivider.height)
        #expect(controller.isPinnedToBottom)
        let opened = list.visibleViews()
        list.window.layoutIfNeeded()
        list.expectTheSameViewsInPlace(opened)

        // Scrolled below the line: a new message, then the line going (a mark as read).
        list.scroll(toRowOf: 175)
        let reading = list.visibleViews()
        let arrived = page + [row(201, by: author(2), at: noon + 201 * 120)]
        controller.show(MessageTableState(rows: arrived, newMessages: line(before: 170)))
        list.expectTheSameViewsInPlace(reading)
        controller.show(MessageTableState(rows: arrived))
        list.expectTheSameViewsInPlace(reading)
        #expect(lineRow == nil)

        // Scrolled up: a line for a later message comes below the reader.
        list.scroll(toRowOf: 120)
        let above = list.visibleViews()
        controller.show(MessageTableState(rows: arrived, newMessages: line(before: 201)))
        list.expectTheSameViewsInPlace(above)
    }

    // Scrolled up to the line: its message is the reader's top one, as when the line goes with
    // a mark as read or moves with READY or an older page.
    @Test(arguments: ["group", "notice", "day"])
    func theLineAboveTheTopMessageMovesNoText(under kind: String) {
        let rows = list.page.map { shown -> MessageListModel.Row in
            let raw = shown.id.rawValue
            let time = shown.message.timestamp
            switch kind {
            case "notice" where raw == 171:
                return row(raw, by: author(2), at: time, kind: .userJoin)
            case "day" where raw >= 171:
                return row(raw, by: shown.message.author, at: time + 86_400)
            default:
                return shown
            }
        }
        list.show(MessageTableState(rows: rows, newMessages: line(before: 171)))
        list.scroll(toRowOf: 171)
        var reading = list.visibleViews()
        reading[171] = nil
        #expect(reading.count > 3)

        controller.show(MessageTableState(rows: rows))
        list.expectTheSameViewsInPlace(reading)

        list.window.layoutIfNeeded()
        reading = list.visibleViews()
        reading[171] = nil
        controller.show(MessageTableState(rows: rows, newMessages: line(before: 171)))
        list.expectTheSameViewsInPlace(reading)
    }

    @Test
    func aNewMessageWithItsLineKeepsAPinnedListAtTheBottom() throws {
        let page = list.page
        list.show(MessageTableState(rows: page))

        let arrived = page + [row(201, by: author(2), at: noon + 201 * 120)]
        list.show(MessageTableState(rows: arrived, newMessages: line(before: 201)))

        #expect(controller.isPinnedToBottom)
        #expect(lineRow == table.numberOfRows - 2)
    }

    // The line takes the space a group leaves before its first message.
    @Test
    func theLineSitsInTheGapAboveAGroupsFirstMessage() throws {
        let rows = [
            row(1, by: author(1), at: noon),
            row(2, by: author(2), at: noon + 60),
            row(3, by: author(2), at: noon + 120),
        ]
        list.show(MessageTableState(rows: rows, reachedOldest: true))
        let plain = contentHeight

        list.show(MessageTableState(rows: rows, reachedOldest: true, newMessages: line(before: 2)))

        #expect(abs(contentHeight - plain) <= 0.5)
        let shown = try #require(lineRow)
        let divider = try #require(try list.cell(at: shown) as? NewMessagesDivider)
        let message = try #require(try list.cell(of: 2) as? MessageCell)
        let box = message.convert(message.box, to: table)
        let drawn = divider.convert(divider.line, to: table)
        #expect(abs(box.minY - drawn.maxY - 7) <= 0.5)
        let below = list.index(of: 2)
        #expect(table.rect(ofRow: below).height == list.fittingHeight(ofRow: below))

        // A follow-up keeps its group, so the line adds its own row.
        list.show(MessageTableState(rows: rows, reachedOldest: true, newMessages: line(before: 3)))
        #expect(abs(contentHeight - plain - NewMessagesDivider.height) <= 0.5)
    }

    @Test(arguments: [UInt64(2), 3, 4])
    func rowsAroundTheLineAreAsTallAsTheirCells(before raw: UInt64) {
        let rows = [
            row(1, by: author(1), at: noon),
            row(2, by: author(2), at: noon + 86_400, kind: .reply),
            row(3, by: author(2), at: noon + 86_460),
            row(4, at: noon + 86_520, kind: .userJoin),
        ]
        list.show(MessageTableState(rows: rows, newMessages: line(before: raw)))

        for width: CGFloat in [600, 380] {
            list.window.setContentSize(NSSize(width: width, height: 400))
            list.window.layoutIfNeeded()
            let wrong = (0..<table.numberOfRows).filter { row in
                table.rect(ofRow: row).height != list.fittingHeight(ofRow: row)
            }
            #expect(wrong.isEmpty, "at \(width): rows \(wrong) differ from their cells")
        }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theLineAndItsTagDrawInThePalettesRed(appearance: NSAppearance.Name) throws {
        let backdrop = MessageTextDrawingTests.Backdrop(
            frame: NSRect(x: 0, y: 0, width: 600, height: NewMessagesDivider.height))
        let divider = NewMessagesDivider()
        divider.translatesAutoresizingMaskIntoConstraints = true
        divider.frame = backdrop.bounds
        backdrop.addSubview(divider)
        drawing.window.appearance = NSAppearance(named: appearance)
        drawing.window.contentView?.addSubview(backdrop)
        divider.layoutSubtreeIfNeeded()
        let line = divider.line
        let tag = divider.tagFrame

        #expect(line.height == 1)
        #expect(line.minX == 16)
        #expect(abs(tag.maxX - (600 - 16)) <= 0.5)
        #expect(abs(tag.midY - line.midY) <= 0.5)
        let onTheLine = try drawing.pixel(divider, at: NSPoint(x: 100, y: line.minY + 0.5))
        #expect(try drawing.matches(onTheLine, Palette.newMessages))
        let tagEdge = try drawing.pixel(divider, at: NSPoint(x: tag.minX + 1.5, y: tag.midY))
        #expect(try drawing.matches(tagEdge, Palette.newMessages))
        let besideTheLine = try drawing.pixel(divider, at: NSPoint(x: 100, y: line.minY - 2))
        #expect(try drawing.matches(besideTheLine, Palette.chat))

        // "NEW" in white inside the tag, away from its rounded ends.
        let text = tag.insetBy(dx: 4, dy: 0)
        let rep = try #require(backdrop.bitmapImageRepForCachingDisplay(in: backdrop.bounds))
        backdrop.cacheDisplay(in: backdrop.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / backdrop.bounds.width
        let white = (Int(text.minX * scale)..<Int(text.maxX * scale)).contains { x in
            (Int(text.minY * scale)..<Int(text.maxY * scale)).contains { y in
                guard let color = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
                    return false
                }
                return min(color.redComponent, color.greenComponent, color.blueComponent) > 0.9
            }
        }
        #expect(white)
    }
}
