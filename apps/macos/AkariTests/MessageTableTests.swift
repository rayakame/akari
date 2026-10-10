import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
struct MessageTableTests {
    final class CountingTableView: MessageTableView {
        var fullReloads = 0
        var removes = 0
        var inserts = 0
        var reloaded: [IndexSet] = []

        override func reloadData() {
            fullReloads += 1
            super.reloadData()
        }

        override func reloadData(forRowIndexes rows: IndexSet, columnIndexes columns: IndexSet) {
            reloaded.append(rows)
            super.reloadData(forRowIndexes: rows, columnIndexes: columns)
        }

        override func removeRows(at indexes: IndexSet, withAnimation options: AnimationOptions = [])
        {
            removes += 1
            super.removeRows(at: indexes, withAnimation: options)
        }

        override func insertRows(at indexes: IndexSet, withAnimation options: AnimationOptions = [])
        {
            inserts += 1
            super.insertRows(at: indexes, withAnimation: options)
        }

        func forget() {
            (fullReloads, removes, inserts, reloaded) = (0, 0, 0, [])
        }
    }

    // Where the pointer is, in window coordinates; nil while it's outside the window.
    final class Pointer {
        var location: NSPoint?
    }

    let noon = berlinTime(2026, 10, 10, 12, 0)
    let table = CountingTableView()
    let pointer = Pointer()
    let controller: MessageTableController
    let window: NSWindow

    init() {
        controller = MessageTableController(
            tableView: table, calendar: berlin, now: { berlinTime(2026, 10, 10, 15, 0) },
            pointer: { [pointer] in pointer.location })
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
    }

    func messages(_ count: Int, from first: UInt64 = 1, longText: Bool = false) -> [MessageListModel
        .Row]
    {
        (0..<count).map { index in
            let raw = first + UInt64(index)
            return row(
                raw, by: author(raw % 3 == 0 ? 2 : 1), at: noon + Double(raw) * 120,
                content: longText ? String(repeating: "words that wrap ", count: 12) : nil)
        }
    }

    // From one line to many, so no two neighbours are likely to share a height.
    func variedMessages(_ count: Int) -> [MessageListModel.Row] {
        (0..<count).map { index in
            let raw = UInt64(index + 1)
            return row(
                raw, by: author(raw % 3 == 0 ? 2 : 1), at: noon + Double(raw) * 120,
                content: String(repeating: "words that wrap ", count: Int(raw % 7) * 6 + 1))
        }
    }

    // What Auto Layout gives the row's cell at the table's width, as automatic row heights do.
    func fittingHeight(ofRow row: Int) -> CGFloat {
        let cell = controller.cell(for: controller.timeline.items[row])
        cell.translatesAutoresizingMaskIntoConstraints = false
        let width = cell.widthAnchor.constraint(equalToConstant: table.tableColumns[0].width)
        width.isActive = true
        defer {
            width.isActive = false
            cell.translatesAutoresizingMaskIntoConstraints = true
        }
        cell.layoutSubtreeIfNeeded()
        return cell.fittingSize.height
    }

    func show(_ rows: [MessageListModel.Row]) {
        controller.show(rows, atPresent: true)
        window.layoutIfNeeded()
        #expect(table.numberOfRows == controller.timeline.items.count)
    }

    // Every row's view shows the item at its index, not just the right number of rows.
    func expectRowsMatchTheTimeline() {
        let shown = (0..<table.numberOfRows).map { row in
            (table.view(atColumn: 0, row: row, makeIfNecessary: true) as? NSTableCellView)?
                .objectValue as? MessageTimeline.ItemId
        }
        #expect(shown == controller.timeline.items.map(\.id))
    }

    var visibleRows: Range<Int> {
        let range = table.rows(in: table.visibleRect)
        return range.location..<(range.location + range.length)
    }

    @Test
    func updatesNeverReloadTheWholeTable() {
        var rows = messages(60)
        show(rows)
        table.forget()

        expectRowsMatchTheTimeline()
        rows += messages(3, from: 61)
        show(rows)
        expectRowsMatchTheTimeline()
        rows[62] = row(500, by: rows[62].message.author, at: rows[62].message.timestamp, key: 63)
        show(rows)
        expectRowsMatchTheTimeline()
        // Message 4 starts a group that message 5 continues; 5 takes over the header.
        let reloadsBefore = table.reloaded.count
        rows.removeAll { $0.message.id.rawValue == 4 }
        show(rows)
        expectRowsMatchTheTimeline()
        #expect(table.reloaded.count > reloadsBefore)
        rows.removeFirst(10)
        rows += messages(2, from: 70)
        show(rows)
        expectRowsMatchTheTimeline()
        rows.append(row(80, at: noon + 86_400))
        show(rows)
        expectRowsMatchTheTimeline()

        #expect(table.fullReloads == 0)
        #expect(table.inserts > 0 && table.removes > 0)
    }

    @Test
    func aConfirmationReloadsOneRow() {
        var rows = messages(5)
        show(rows)
        table.forget()
        let index = controller.timeline.items.count - 1

        rows[4] = row(99, by: rows[4].message.author, at: rows[4].message.timestamp, key: 5)
        show(rows)

        #expect(table.reloaded == [IndexSet(integer: index)])
        #expect(table.removes == 0 && table.inserts == 0)
        #expect(table.fullReloads == 0)
    }

    @Test
    func aScrollWithoutLiveScrollNotificationsUnpins() {
        var rows = messages(80, longText: true)
        show(rows)
        let clip = controller.scrollView.contentView

        // Like a classic mouse wheel, Page Up or an arrow key: no live-scroll notification.
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: 30).minY))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        let first = visibleRows.lowerBound
        let offset = table.rect(ofRow: first).minY - clip.bounds.minY
        rows += messages(1, from: 81)
        show(rows)

        #expect(visibleRows.lowerBound == first)
        #expect(abs(table.rect(ofRow: first).minY - clip.bounds.minY - offset) <= 1)
    }

    @Test
    func rowsAwayFromThePresentDontPinOnLayout() {
        var rows = messages(80, longText: true)
        show(rows)
        let clip = controller.scrollView.contentView
        let first = visibleRows.lowerBound
        let offset = table.rect(ofRow: first).minY - clip.bounds.minY

        rows += messages(5, from: 81)
        controller.show(rows, atPresent: false)
        window.layoutIfNeeded()
        table.needsLayout = true
        window.layoutIfNeeded()

        #expect(visibleRows.lowerBound == first)
        #expect(abs(table.rect(ofRow: first).minY - clip.bounds.minY - offset) <= 1)
    }

    @Test
    func leavingThePresentWithoutNewRowsStopsPinning() {
        let rows = messages(80, longText: true)
        show(rows)
        let clip = controller.scrollView.contentView

        controller.show(rows, atPresent: false)
        window.layoutIfNeeded()
        let first = visibleRows.lowerBound
        let offset = table.rect(ofRow: first).minY - clip.bounds.minY
        window.setContentSize(NSSize(width: 300, height: 400))
        window.layoutIfNeeded()

        #expect(visibleRows.lowerBound == first)
        #expect(abs(table.rect(ofRow: first).minY - clip.bounds.minY - offset) <= 1)
    }

    @Test
    func newMessagesFollowTheBottomOnlyWhenPinned() {
        var rows = messages(80, longText: true)
        show(rows)
        #expect(visibleRows.contains(table.numberOfRows - 1))

        rows += messages(1, from: 81)
        show(rows)
        #expect(visibleRows.contains(table.numberOfRows - 1))

        controller.scrollView.contentView.scroll(to: .zero)
        controller.scrollView.reflectScrolledClipView(controller.scrollView.contentView)
        NotificationCenter.default.post(
            name: NSScrollView.didLiveScrollNotification, object: controller.scrollView)
        let firstVisible = visibleRows.lowerBound
        rows += messages(1, from: 82)
        show(rows)
        #expect(visibleRows.lowerBound == firstVisible)

        table.scrollRowToVisible(table.numberOfRows - 1)
        NotificationCenter.default.post(
            name: NSScrollView.didLiveScrollNotification, object: controller.scrollView)
        window.setContentSize(NSSize(width: 300, height: 400))
        window.layoutIfNeeded()
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func aListScrolledUpStaysPutWhenRowsChangeAboveIt() throws {
        var rows = messages(200)
        show(rows)
        let clip = controller.scrollView.contentView
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: 100).minY))
        controller.scrollView.reflectScrolledClipView(clip)
        for name in [
            NSScrollView.willStartLiveScrollNotification, NSScrollView.didLiveScrollNotification,
            NSScrollView.didEndLiveScrollNotification,
        ] {
            NotificationCenter.default.post(name: name, object: controller.scrollView)
        }
        window.layoutIfNeeded()
        let first = visibleRows.lowerBound
        let key = controller.timeline.items[first].id
        let offset = table.rect(ofRow: first).minY - clip.bounds.minY

        rows.removeFirst()
        rows += messages(1, from: 201)
        show(rows)
        rows.remove(at: 10)
        show(rows)

        let moved = try #require(controller.timeline.items.firstIndex { $0.id == key })
        #expect(abs(table.rect(ofRow: moved).minY - clip.bounds.minY - offset) <= 1)
    }

    @Test
    func aShortConversationSitsAtTheBottom() {
        var rows = messages(3)
        show(rows)
        let clip = controller.scrollView.contentView

        let below = { clip.bounds.maxY - self.table.rect(ofRow: self.table.numberOfRows - 1).maxY }
        #expect(abs(below() - 16) <= 1)
        #expect(table.rect(ofRow: 0).minY - clip.bounds.minY > 100)

        rows += messages(1, from: 4)
        show(rows)
        #expect(abs(below() - 16) <= 1)

        // A short list is at its bottom, so a resize doesn't unpin it and later messages follow.
        window.setContentSize(NSSize(width: 600, height: 300))
        window.layoutIfNeeded()
        rows += messages(60, from: 5, longText: true)
        show(rows)
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    func textFields(in view: NSView) -> [NSTextField] {
        view.subviews.flatMap { subview in
            (subview as? NSTextField).map { [$0] } ?? textFields(in: subview)
        }
    }

    func cell(at row: Int) throws -> NSView {
        let cell = try #require(table.view(atColumn: 0, row: row, makeIfNecessary: true))
        cell.layoutSubtreeIfNeeded()
        return cell
    }

    var hoveredRows: [Int] {
        visibleRows.filter { row in
            (table.view(atColumn: 0, row: row, makeIfNecessary: false) as? MessageCell)?
                .isHovered == true
        }
    }

    func postLiveScroll(_ name: Notification.Name) {
        NotificationCenter.default.post(name: name, object: controller.scrollView)
    }

    // Positive scrolls up. Without a gesture phase it scrolls as a mouse wheel does, on a later
    // pass of the run loop; offscreen, phased trackpad events don't scroll at all.
    func scrollWheel(by pixels: Int32) async throws {
        let event = try #require(
            CGEvent(
                scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: pixels,
                wheel2: 0, wheel3: 0))
        event.setIntegerValueField(.scrollWheelEventIsContinuous, value: 1)
        let clip = controller.scrollView.contentView
        let start = clip.bounds.origin
        controller.scrollView.scrollWheel(with: try #require(NSEvent(cgEvent: event)))
        for _ in 0..<40 where clip.bounds.origin == start {
            try await Task.sleep(for: .milliseconds(50))
        }
        try await Task.sleep(for: .milliseconds(200))
    }

    @Test
    func theHoveredRowFollowsThePointerOnEveryScroll() {
        show(messages(80, longText: true))
        let clip = controller.scrollView.contentView
        pointer.location = NSPoint(x: 300, y: 200)
        let underPointer = {
            self.table.row(at: self.table.convert(NSPoint(x: 300, y: 200), from: nil))
        }

        controller.pointerMoved()
        #expect(hoveredRows == [underPointer()])

        // Scrolled under a still pointer, with no live-scroll notification.
        clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY - 300))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        #expect(hoveredRows == [underPointer()])

        pointer.location = nil
        controller.pointerMoved()
        #expect(hoveredRows.isEmpty)
    }

    @Test
    func clickingMessageTextKeepsItsFont() throws {
        show([row(1, at: noon, content: "hello there")])
        let field = try #require(
            textFields(in: try cell(at: 1)).first { $0.stringValue == "hello there" })

        // What a click on selectable text does: the field editor takes over the text.
        field.selectText(nil)

        let editor = try #require(field.currentEditor() as? NSTextView)
        let attribute = { (key: NSAttributedString.Key) in
            editor.textStorage?.attribute(key, at: 0, effectiveRange: nil)
        }
        #expect((attribute(.font) as? NSFont)?.pointSize == 16)
        #expect((attribute(.paragraphStyle) as? NSParagraphStyle)?.minimumLineHeight == 22)
    }

    @Test
    func scrollingPastTheBottomStopsThereWithoutBouncing() async throws {
        var rows = messages(80, longText: true)
        show(rows)
        let scrollView = controller.scrollView
        #expect(scrollView.verticalScrollElasticity == .none)
        #expect(scrollView.horizontalScrollElasticity == .none)
        let clip = scrollView.contentView
        let bottom = clip.bounds.origin

        try await scrollWheel(by: 80)
        #expect(clip.bounds.origin.y < bottom.y)
        try await scrollWheel(by: -300)
        #expect(clip.bounds.origin == bottom)

        rows += messages(1, from: 81)
        show(rows)
        window.layoutIfNeeded()
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func aFlingToTheBottomNeitherMovesTheBottomNorScrollsOnItsOwn() {
        show(variedMessages(150))
        let clip = controller.scrollView.contentView
        let content = table.contentHeight
        let bottom = clip.bounds.origin.y

        // AppKit moves the clip by large steps per frame, so rows in between never show.
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        var heights: Set<CGFloat> = []
        var moves: [String] = []
        var y: CGFloat = 0
        while true {
            clip.scroll(to: NSPoint(x: 0, y: y))
            controller.scrollView.reflectScrolledClipView(clip)
            postLiveScroll(NSScrollView.didLiveScrollNotification)
            window.layoutIfNeeded()
            heights.insert(table.contentHeight)
            if clip.bounds.origin.y != y {
                moves.append("\(y) → \(clip.bounds.origin.y)")
            }
            if y >= bottom {
                break
            }
            y = min(clip.bounds.origin.y + 700, bottom)
        }
        postLiveScroll(NSScrollView.didEndLiveScrollNotification)
        window.layoutIfNeeded()

        #expect(heights == [content], "the document was \(heights.sorted()) tall")
        #expect(moves.isEmpty, "the list moved itself: \(moves)")
        #expect(clip.bounds.origin.y == bottom)
    }

    @Test
    func everyRowIsAsTallAsItsCellAtEveryWidth() {
        let later = noon + 2 * 86_400
        let rows =
            variedMessages(30) + [
                row(40, by: author(9, "Ferris", bot: true), at: later, kind: .reply),
                row(41, by: author(9, "Ferris", bot: true), at: later + 10, content: ""),
                row(42, at: later + 20, kind: .userJoin),
                row(43, by: author(3), at: later + 30, content: "", componentsV2: true),
                row(
                    44, by: author(3), at: later + 40, attachments: ["a.png", "b.txt"],
                    embedCount: 2, stickers: ["wave"]),
                row(45, by: author(3), at: later + 50, content: "", attachments: ["c.zip"]),
            ]
        show(rows)

        for width: CGFloat in [600, 380, 900] {
            window.setContentSize(NSSize(width: width, height: 400))
            window.layoutIfNeeded()
            let wrong = (0..<table.numberOfRows).filter { row in
                table.rect(ofRow: row).height != fittingHeight(ofRow: row)
            }
            #expect(wrong.isEmpty, "at \(width): rows \(wrong) differ from their cells")
        }
    }

    @Test
    func aLiveScrollHoldsTheListUntilItsNotificationsStop() async throws {
        var rows = messages(80, longText: true)
        show(rows)
        let clip = controller.scrollView.contentView
        let bottom = clip.bounds.origin

        // Paging starts a live scroll and never ends it.
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)
        rows += messages(1, from: 81)
        show(rows)
        window.layoutIfNeeded()
        #expect(clip.bounds.origin == bottom)

        try await Task.sleep(for: .seconds(0.8))
        window.layoutIfNeeded()
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func theAppTagSitsCenteredBesideTheName() throws {
        show([row(1, by: author(9, "Ferris", bot: true), at: noon)])
        let cell = try cell(at: 1)
        let fields = textFields(in: cell)
        let name = try #require(fields.first { $0.stringValue == "Ferris" })
        let tag = try #require(fields.first { $0.stringValue == "APP" })

        // The name's alignment rect ends where its text ends; the tag's pill fills its frame.
        let nameText = name.convert(name.alignmentRect(forFrame: name.bounds), to: cell)
        let tagFrame = tag.convert(tag.bounds, to: cell)
        #expect(abs(tagFrame.midY - nameText.midY) <= 1)
        #expect(abs(tagFrame.minX - nameText.maxX - 4) <= 1)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theAppTagCentersItsCapitalsInThePill(appearance: NSAppearance.Name) throws {
        show([row(1, by: author(9, "Ferris", bot: true), at: noon)])
        let tag = try #require(textFields(in: try cell(at: 1)).first { $0.stringValue == "APP" })
        tag.appearance = NSAppearance(named: appearance)
        let rep = try #require(tag.bitmapImageRepForCachingDisplay(in: tag.bounds))
        tag.cacheDisplay(in: tag.bounds, to: rep)

        // The text is white; the pill isn't, and an unrendered background is transparent.
        let glyphRows = (0..<rep.pixelsHigh).filter { y in
            (0..<rep.pixelsWide).contains { x in
                guard let color = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB) else {
                    return false
                }
                let lightest = min(color.redComponent, color.greenComponent, color.blueComponent)
                return lightest * color.alphaComponent > 0.6
            }
        }
        let first = try #require(glyphRows.first)
        let last = try #require(glyphRows.last)
        let above = first
        let below = rep.pixelsHigh - 1 - last
        #expect(abs(above - below) <= 1, "\(above) px above the capitals, \(below) below")
    }

    @Test
    func aComponentsV2MessageShowsAPlaceholder() throws {
        show([row(1, at: noon, content: "", componentsV2: true)])

        let shown = textFields(in: try cell(at: 1)).filter { !$0.isHidden }.map(\.stringValue)
        #expect(shown.contains { $0.contains("This message uses a layout Akari can't show yet") })
    }

    @Test
    func cellsMatchTheirItemKinds() throws {
        show([
            row(1, at: noon), row(2, at: noon + 60), row(3, at: noon + 120, kind: .userJoin),
        ])

        let cells = (0..<table.numberOfRows).map {
            table.view(atColumn: 0, row: $0, makeIfNecessary: true)
        }

        #expect(cells[0] is DayDividerCell)
        #expect((cells[1] as? MessageCell)?.showsHeader == true)
        #expect((cells[2] as? MessageCell)?.showsHeader == false)
        #expect(cells[3] is NoticeCell)
    }
}
