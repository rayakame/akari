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
            tableView: table, calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) },
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
        show(MessageTableState(rows: rows))
    }

    func show(_ state: MessageTableState) {
        controller.show(state)
        window.layoutIfNeeded()
        #expect(table.numberOfRows == controller.timeline.items.count)
    }

    var clip: NSClipView { controller.scrollView.contentView }

    func index(of raw: UInt64) -> Int {
        controller.timeline.items.firstIndex { $0.id == .message(MessageId(rawValue: raw)) } ?? -1
    }

    // Where the message's row starts, measured from the top of what's visible.
    func place(of raw: UInt64) -> CGFloat {
        table.rect(ofRow: index(of: raw)).minY - clip.bounds.minY
    }

    // As the user would: a scroll that isn't the controller's own.
    func scroll(toRowOf raw: UInt64) {
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: index(of: raw)).minY))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
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
        show(MessageTableState(rows: rows, atPresent: false))
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

        show(MessageTableState(rows: rows, atPresent: false))
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
        show(MessageTableState(rows: rows, reachedOldest: true))
        let clip = controller.scrollView.contentView

        let below = { clip.bounds.maxY - self.table.rect(ofRow: self.table.numberOfRows - 1).maxY }
        #expect(abs(below() - 8) <= 1)
        #expect(table.rect(ofRow: 0).minY - clip.bounds.minY > 100)

        rows += messages(1, from: 4)
        show(MessageTableState(rows: rows, reachedOldest: true))
        #expect(abs(below() - 8) <= 1)

        // A short list is at its bottom, so a resize doesn't unpin it and later messages follow.
        window.setContentSize(NSSize(width: 600, height: 300))
        window.layoutIfNeeded()
        rows += messages(60, from: 5, longText: true)
        show(MessageTableState(rows: rows, reachedOldest: true))
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    func textFields(in view: NSView) -> [NSTextField] {
        view.subviews.flatMap { subview in
            (subview as? NSTextField).map { [$0] } ?? textFields(in: subview)
        }
    }

    func cell(of raw: UInt64) throws -> NSView {
        try cell(at: index(of: raw))
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
            textFields(in: try cell(of: 1)).first { $0.stringValue == "hello there" })

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
    func theListBouncesOnlyVerticallyAndAWheelLandsOnTheBottom() async throws {
        var rows = messages(80, longText: true)
        show(rows)
        let scrollView = controller.scrollView
        #expect(scrollView.verticalScrollElasticity == .automatic)
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
    func anOverscrollPastTheBottomMovesNothingOfOursAndStaysPinned() {
        var rows = messages(80, longText: true)
        show(rows)
        let clip = controller.scrollView.contentView
        let bottom = clip.bounds.origin

        // The elastic bounce takes the clip past the end; scroll(to:) isn't constrained.
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        clip.scroll(to: NSPoint(x: bottom.x, y: bottom.y + 40))
        postLiveScroll(NSScrollView.didLiveScrollNotification)
        let bounced = clip.bounds.origin
        #expect(bounced.y == bottom.y + 40)
        #expect(controller.isPinnedToBottom)
        table.needsLayout = true
        window.layoutIfNeeded()
        #expect(clip.bounds.origin == bounced)

        // The gesture can end before the bounce has settled back.
        postLiveScroll(NSScrollView.didEndLiveScrollNotification)
        window.layoutIfNeeded()
        #expect(clip.bounds.origin == bounced)

        clip.scroll(to: bottom)
        rows += messages(1, from: 81)
        show(rows)
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
                row(46, by: author(3), at: later + 60, delivery: .pending),
                row(47, by: author(3), at: later + 70, delivery: .failed),
                row(
                    48, by: author(3), at: later + 80,
                    content: String(repeating: "x", count: 52), edited: later + 90),
                row(
                    49, by: author(3), at: later + 100, content: "", attachments: ["d.zip"],
                    edited: later + 110),
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
        let cell = try cell(of: 1)
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
        let tag = try #require(textFields(in: try cell(of: 1)).first { $0.stringValue == "APP" })
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

        let shown = textFields(in: try cell(of: 1)).filter { !$0.isHidden }.map(\.stringValue)
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

        #expect(cells[0] is PlaceholderCell)
        #expect(cells[1] is DayDividerCell)
        #expect((cells[2] as? MessageCell)?.showsHeader == true)
        #expect((cells[3] as? MessageCell)?.showsHeader == false)
        #expect(cells[4] is NoticeCell)
    }

    final class RecordingActions: MessageListActions {
        var calls: [String] = []

        func retry(_ message: MessageId) {
            calls.append("retry \(message.rawValue)")
        }

        func delete(_ message: MessageId) {
            calls.append("delete \(message.rawValue)")
        }

        func loadMore(_ edge: MessageTimeline.Edge) {
            calls.append(edge == .older ? "more older" : "more newer")
        }

        func jumpToLatest() {
            calls.append("latest")
        }

        func escape() {
            calls.append("escape")
        }
    }

    func contentColor(of raw: UInt64, text: String) throws -> NSColor? {
        let field = try #require(
            textFields(in: try cell(of: raw)).first { $0.stringValue.hasPrefix(text) })
        return field.attributedStringValue.attribute(.foregroundColor, at: 0, effectiveRange: nil)
            as? NSColor
    }

    @Test
    func pendingIsDimmedAndFailedIsRedWithActions() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show([
            row(1, at: noon), row(2, at: noon + 10, delivery: .pending),
            row(3, at: noon + 20, delivery: .failed),
        ])

        #expect(try contentColor(of: 1, text: "message 1") == Palette.textDefault)
        #expect(try contentColor(of: 2, text: "message 2") == Palette.textMuted)
        #expect(try contentColor(of: 3, text: "message 3") == Palette.danger)
        let failed = try cell(of: 3)
        let visible = { (cell: NSView) in
            self.textFields(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }.map(\.stringValue)
        }
        #expect(visible(failed).contains("Not sent."))
        #expect(!visible(try cell(of: 2)).contains("Not sent."))
        let buttons = buttons(in: failed)
        try #require(buttons.first { $0.title == "Retry" }).performClick(nil)
        try #require(buttons.first { $0.title == "Delete" }).performClick(nil)
        // The short list also asks for older messages; only the row's actions matter here.
        #expect(actions.calls.filter { !$0.hasPrefix("more") } == ["retry 3", "delete 3"])
    }

    func buttons(in view: NSView) -> [NSButton] {
        view.subviews.flatMap { subview in
            (subview as? NSButton).map { [$0] } ?? buttons(in: subview)
        }.filter { !$0.isHiddenOrHasHiddenAncestor }
    }

    @Test
    func editedMessagesEndWithTheMarker() throws {
        show([
            row(1, at: noon, edited: noon + 60),
            row(2, at: noon + 10, content: "", attachments: ["a.zip"], edited: noon + 70),
            row(3, at: noon + 20),
        ])

        let fields = textFields(in: try cell(of: 1)).filter {
            $0.stringValue.hasPrefix("message 1")
        }
        let text = try #require(fields.first).attributedStringValue
        #expect(text.string == "message 1 (edited)")
        let marker = text.attributes(at: text.length - 1, effectiveRange: nil)
        #expect((marker[.font] as? NSFont)?.pointSize == 12)
        #expect(marker[.foregroundColor] as? NSColor == Palette.chatTextMuted)
        let extras = textFields(in: try cell(of: 2)).filter { $0.stringValue.contains("a.zip") }
        #expect(try #require(extras.first).stringValue.hasSuffix(" (edited)"))
        let plain = textFields(in: try cell(of: 3)).filter { $0.stringValue.hasPrefix("message 3") }
        #expect(try #require(plain.first).stringValue == "message 3")
    }

    // 100 one-line messages, 101...200, two minutes apart on one day.
    var page: [MessageListModel.Row] { messages(100, from: 101) }

    func heights(_ raws: ClosedRange<UInt64>) -> CGFloat {
        raws.reduce(0) { $0 + table.rect(ofRow: index(of: $1)).height }
    }

    @Test
    func anOlderPageKeepsTheReaderExactlyInPlace() {
        show(page)
        scroll(toRowOf: 130)
        let before = place(of: 130)
        let y = clip.bounds.minY

        show(messages(50, from: 51) + page)

        // The new rows went in above; 101 losing its header takes a little back.
        #expect(abs(place(of: 130) - before) <= 0.5)
        #expect(clip.bounds.minY - y > heights(52...100))
        #expect(
            controller.timeline.items[visibleRows.lowerBound].id
                == .message(MessageId(rawValue: 130)))
    }

    @Test
    func anOlderPageDuringALiveScrollKeepsTheReaderInPlace() {
        show(page)
        scroll(toRowOf: 130)
        let before = place(of: 130)
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)

        show(messages(50, from: 51) + page)

        #expect(abs(place(of: 130) - before) <= 0.5)
    }

    @Test
    func aPageThatLandsDuringTheTopBounceWaitsForItToEnd() {
        show(page)
        clip.scroll(to: NSPoint(x: 0, y: -40))
        controller.scrollView.reflectScrolledClipView(clip)
        let before = place(of: 101)
        let shown = table.numberOfRows
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)

        show(MessageTableState(rows: messages(50, from: 51) + page))
        #expect(table.numberOfRows == shown)
        #expect(abs(place(of: 101) - before) <= 0.5)

        postLiveScroll(NSScrollView.didEndLiveScrollNotification)
        window.layoutIfNeeded()
        #expect(table.numberOfRows == 152)
        #expect(abs(place(of: 101) - before) <= 0.5)
    }

    @Test
    func aPageThatTrimsTheOtherEndMovesNothing() {
        show(page)
        scroll(toRowOf: 130)
        let before = place(of: 130)

        let older = messages(50, from: 51) + page.prefix(50)
        show(MessageTableState(rows: older, atPresent: false))
        #expect(abs(place(of: 130) - before) <= 0.5)

        let newer = Array(page.prefix(50)) + messages(50, from: 151)
        show(MessageTableState(rows: newer, atPresent: false))
        #expect(abs(place(of: 130) - before) <= 0.5)
    }

    @Test
    func deletingAboveBelowOrTheFirstVisibleRowMovesNothing() {
        var rows = page
        show(rows)
        scroll(toRowOf: 150)
        let before = place(of: 150)

        rows.removeAll { $0.id.rawValue == 140 }
        show(rows)
        #expect(abs(place(of: 150) - before) <= 0.5)

        rows.removeAll { $0.id.rawValue == 195 }
        show(rows)
        #expect(abs(place(of: 150) - before) <= 0.5)

        let next = place(of: 151)
        rows.removeAll { $0.id.rawValue == 150 }
        show(rows)
        #expect(abs(place(of: 151) - next) <= 0.5)
    }

    @Test
    func aDeleteWhilePinnedStaysPinned() {
        var rows = page
        show(rows)
        rows.removeLast()
        show(rows)

        #expect(visibleRows.contains(table.numberOfRows - 1))
        #expect(controller.isPinnedToBottom)
    }

    @Test
    func anEditAboveTheReaderThatGrowsMovesNothing() {
        var rows = page
        show(rows)
        scroll(toRowOf: 150)
        let before = place(of: 150)
        let long = String(repeating: "words that wrap ", count: 30)

        rows[39] = row(
            140, by: rows[39].message.author, at: rows[39].message.timestamp, content: long)
        show(rows)
        #expect(abs(place(of: 150) - before) <= 0.5)

        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)
        rows[44] = row(
            145, by: rows[44].message.author, at: rows[44].message.timestamp, content: long)
        show(rows)
        #expect(abs(place(of: 150) - before) <= 0.5)
    }

    @Test
    func everyCompensationIsLogged() {
        var lines: [String] = []
        let enabled = ScrollLog.enabled
        let write = ScrollLog.write
        ScrollLog.enabled = true
        ScrollLog.write = { lines.append($0) }
        defer {
            ScrollLog.enabled = enabled
            ScrollLog.write = write
        }
        show(page)
        scroll(toRowOf: 130)
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)

        show(messages(50, from: 51) + page)

        let compensations = lines.filter { $0.contains("compensates") }
        #expect(compensations.count == 1, "\(compensations)")
        let line = compensations.first ?? ""
        #expect(line.contains("inserted 50 reloaded 1 above"), "\(line)")
        #expect(line.contains("live true"), "\(line)")
        #expect(line.contains("delta"), "\(line)")
    }

    @Test
    func nearTheTopAsksForOlderOnceUntilItLands() {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        #expect(actions.calls.isEmpty)

        scroll(toRowOf: 103)
        #expect(actions.calls == ["more older"])
        // Before the model says it's loading.
        scroll(toRowOf: 104)
        #expect(actions.calls == ["more older"])
        show(MessageTableState(rows: page, loading: .older))
        scroll(toRowOf: 102)
        #expect(actions.calls == ["more older"])

        show(messages(5, from: 96) + page)
        #expect(actions.calls == ["more older", "more older"])

        show(MessageTableState(rows: messages(5, from: 96) + page, reachedOldest: true))
        scroll(toRowOf: 97)
        #expect(actions.calls == ["more older", "more older"])
    }

    @Test
    func aFailedEdgeWaitsForTryAgain() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(MessageTableState(rows: page, failedLoads: [.older: .ServerError(status: 500)]))

        scroll(toRowOf: 102)
        #expect(actions.calls.isEmpty)
        let edge = try cell(at: 0)
        #expect(textFields(in: edge).contains { $0.stringValue == "Couldn't load older messages." })
        try #require(buttons(in: edge).first { $0.title == "Try again" }).performClick(nil)
        #expect(actions.calls == ["more older"])
    }

    @Test
    func nearTheBottomAsksForNewerOnlyWhileDetached() {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        #expect(actions.calls.isEmpty)

        show(MessageTableState(rows: page, atPresent: false))
        #expect(actions.calls == ["more newer"])
    }

    @Test
    func noLoadsBeforeTheFirstFill() {
        let actions = RecordingActions()
        controller.actions = actions
        show(MessageTableState(rows: [], loading: .latest))
        show(MessageTableState(rows: []))
        #expect(actions.calls.isEmpty)
    }

    @Test
    func aShortChannelLoadsUntilItFillsOrBegins() {
        let actions = RecordingActions()
        controller.actions = actions
        show(messages(5))
        #expect(actions.calls == ["more older"])

        show(MessageTableState(rows: messages(5), reachedOldest: true))
        #expect(actions.calls == ["more older"])
    }

    @Test
    func placeholdersKeepOneHeightInEveryState() throws {
        let failed: [MessageListModel.Load: RequestError] = [.older: .ServerError(status: 500)]
        var heights: [CGFloat] = []
        for state in [
            MessageTableState(rows: page), MessageTableState(rows: page, loading: .older),
            MessageTableState(rows: page, failedLoads: failed),
        ] {
            show(state)
            heights.append(table.rect(ofRow: 0).height)
        }
        #expect(Set(heights).count == 1)
        #expect(heights[0] >= 1.5 * clip.bounds.height)
        #expect(try cell(at: 0) is PlaceholderCell)

        show(
            MessageTableState(
                rows: page, reachedOldest: true, beginning: "This is the beginning of #general."))
        #expect(table.rect(ofRow: 0).height == 48)
        #expect(
            textFields(in: try cell(at: 0)).contains {
                $0.stringValue == "This is the beginning of #general."
                    && !$0.isHiddenOrHasHiddenAncestor
            })
    }

    @Test
    func jumpingToThePresentPinsWhenThePageLands() {
        let actions = RecordingActions()
        controller.actions = actions
        show(MessageTableState(rows: page, atPresent: false))
        scroll(toRowOf: 150)
        actions.calls = []

        controller.jumpToPresent(load: true)
        #expect(actions.calls == ["latest"])
        show(messages(100, from: 301))

        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func aScrollBeforeTheJumpLandsCancelsIt() {
        show(MessageTableState(rows: page, atPresent: false))
        scroll(toRowOf: 150)
        controller.jumpToPresent(load: true)
        scroll(toRowOf: 140)

        show(messages(100, from: 301))

        #expect(!visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func jumpingAtThePresentScrollsWithoutLoading() {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        scroll(toRowOf: 120)

        controller.jumpToPresent(load: true)
        window.layoutIfNeeded()

        #expect(!actions.calls.contains("latest"))
        #expect(visibleRows.contains(table.numberOfRows - 1))
    }

    @Test
    func escapeInTheListJumpsToThePresent() {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)

        table.cancelOperation(nil)

        #expect(actions.calls == ["escape"])
    }

    @Test
    func sendingPinsTheList() {
        let actions = RecordingActions()
        controller.actions = actions
        var rows = page
        show(rows)
        scroll(toRowOf: 120)

        controller.jumpToPresent(load: false)
        rows.append(row(201, at: noon + 201 * 120, delivery: .pending))
        show(rows)
        #expect(visibleRows.contains(table.numberOfRows - 1))

        show(MessageTableState(rows: rows, atPresent: false))
        scroll(toRowOf: 120)
        actions.calls = []
        controller.jumpToPresent(load: false)
        #expect(!actions.calls.contains("latest"))
    }

    @Test
    func theJumpBarShowsOnlyWhileDetached() {
        #expect(JumpBar.text(atPresent: true, isStale: false, hasRows: true) == nil)
        #expect(JumpBar.text(atPresent: false, isStale: false, hasRows: false) == nil)
        #expect(
            JumpBar.text(atPresent: false, isStale: false, hasRows: true)
                == "You're reading older messages")
    }

    @Test
    func aStaleWindowSaysCatchingUpOrMissingMessages() {
        #expect(StaleCapsule.isShown(atPresent: true, isStale: true, hasRows: true))
        #expect(!StaleCapsule.isShown(atPresent: false, isStale: true, hasRows: true))
        #expect(!StaleCapsule.isShown(atPresent: true, isStale: false, hasRows: true))
        #expect(!StaleCapsule.isShown(atPresent: true, isStale: true, hasRows: false))
        #expect(
            JumpBar.text(atPresent: false, isStale: true, hasRows: true)
                == "Some messages may be missing")
    }

    @Test
    func theJumpBarLeavesTheNewerEdgeClear() throws {
        show(MessageTableState(rows: page, atPresent: false))
        let bottomClip = try #require(clip as? BottomClipView)
        clip.scroll(to: NSPoint(x: 0, y: bottomClip.originRange.upperBound))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()

        let edge = table.rect(ofRow: table.numberOfRows - 1)
        #expect(edge.maxY <= clip.bounds.maxY - JumpBar.height - JumpBar.margin)

        show(page)
        #expect(controller.scrollView.contentInsets.bottom == 8)
    }

    @Test
    func anEmptyListReleasesAPageHeldDuringTheTopBounce() throws {
        show(MessageTableState(rows: [], loading: .latest))
        let bottomClip = try #require(clip as? BottomClipView)
        clip.scroll(to: NSPoint(x: 0, y: bottomClip.originRange.lowerBound - 40))
        controller.scrollView.reflectScrolledClipView(clip)
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)

        show(page)
        #expect(table.numberOfRows == 1)

        postLiveScroll(NSScrollView.didEndLiveScrollNotification)
        window.layoutIfNeeded()
        #expect(table.numberOfRows == controller.timeline.items.count)
        #expect(table.numberOfRows > 100)
    }

    @Test
    func aRestingFingerPastTheTopIsNeverSnappedBack() async throws {
        show(page)
        let bottomClip = try #require(clip as? BottomClipView)
        let past = bottomClip.originRange.lowerBound - 40
        clip.scroll(to: NSPoint(x: 0, y: past))
        controller.scrollView.reflectScrolledClipView(clip)
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)

        try await Task.sleep(for: .seconds(0.9))
        window.layoutIfNeeded()

        #expect(clip.bounds.minY == past)
    }

    // The composer above changes the list's height the way SwiftUI does: the frame alone.
    func hostInContainer() -> NSView {
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 600, height: 400))
        window.contentView = container
        container.addSubview(controller.scrollView)
        controller.scrollView.frame = container.bounds
        window.layoutIfNeeded()
        return container
    }

    func setListHeight(_ height: CGFloat) {
        controller.scrollView.frame = NSRect(x: 0, y: 400 - height, width: 600, height: height)
        window.layoutIfNeeded()
    }

    var gapBelowTheRows: CGFloat {
        clip.bounds.maxY - (table.contentHeight + controller.scrollView.contentInsets.bottom)
    }

    @Test
    func aPinnedListFollowsTheComposerBothWays() {
        _ = hostInContainer()
        show(page)

        setListHeight(300)
        #expect(visibleRows.contains(table.numberOfRows - 1))
        #expect(abs(gapBelowTheRows) <= 0.5)

        setListHeight(400)
        #expect(visibleRows.contains(table.numberOfRows - 1))
        #expect(abs(gapBelowTheRows) <= 0.5, "a gap of \(gapBelowTheRows) below the rows")
    }

    @Test
    func aScrolledUpListKeepsItsPlaceWhenTheComposerChanges() {
        _ = hostInContainer()
        show(page)
        scroll(toRowOf: 130)
        let before = place(of: 130)

        setListHeight(300)
        #expect(abs(place(of: 130) - before) <= 0.5)
        setListHeight(400)
        #expect(abs(place(of: 130) - before) <= 0.5)
    }

    @Test
    func aFlingIntoThePlaceholdersKeepsMoving() throws {
        show(page)
        let bottomClip = try #require(clip as? BottomClipView)
        let first = table.rect(ofRow: index(of: 101)).minY
        #expect(first - bottomClip.originRange.lowerBound >= 1.5 * clip.bounds.height)

        let inside = first - clip.bounds.height - 50
        clip.scroll(to: NSPoint(x: 0, y: inside))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()

        #expect(inside >= bottomClip.originRange.lowerBound)
        #expect(clip.bounds.minY == inside)
    }

    @Test
    func aPageLandingOverVisiblePlaceholdersKeepsTheFirstMessageInPlace() {
        show(page)
        let first = table.rect(ofRow: index(of: 101)).minY
        clip.scroll(to: NSPoint(x: 0, y: first - 200))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        let before = place(of: 101)

        show(messages(50, from: 51) + page)
        #expect(abs(place(of: 101) - before) <= 0.5)

        // Only placeholders in view: the first real message below them holds the place.
        let next = table.rect(ofRow: index(of: 51)).minY
        clip.scroll(to: NSPoint(x: 0, y: next - clip.bounds.height - 100))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        let hidden = place(of: 51)

        show(messages(50, from: 1) + messages(50, from: 51) + page)
        #expect(abs(place(of: 51) - hidden) <= 0.5)
        #expect(
            visibleRows.contains { row in
                if case .message = controller.timeline.items[row] { true } else { false }
            })
    }

    @Test
    func theNextPageIsRequestedWithinThreeViewHeights() {
        let actions = RecordingActions()
        controller.actions = actions
        show(messages(200, from: 101))
        let first = table.rect(ofRow: index(of: 101)).minY
        let view = clip.bounds.height

        clip.scroll(to: NSPoint(x: 0, y: first + 3.2 * view))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        #expect(actions.calls.isEmpty)

        clip.scroll(to: NSPoint(x: 0, y: first + 2.8 * view))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        #expect(actions.calls == ["more older"])
    }

    @Test
    func everyPlaceholderFillIsLogged() {
        var lines: [String] = []
        let enabled = ScrollLog.enabled
        let write = ScrollLog.write
        ScrollLog.enabled = true
        ScrollLog.write = { lines.append($0) }
        defer {
            ScrollLog.enabled = enabled
            ScrollLog.write = write
        }
        show(page)
        let first = table.rect(ofRow: index(of: 101)).minY
        clip.scroll(to: NSPoint(x: 0, y: first - 200))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()

        show(messages(50, from: 51) + page)

        let fills = lines.filter { $0.contains("fills the older placeholders") }
        #expect(fills.count == 1, "\(lines)")
        #expect(fills.first?.contains("inserted 50") == true, "\(fills)")
        #expect(fills.first?.contains("visible true") == true, "\(fills)")
    }

    @Test
    func anUnloadedChannelsPlaceholdersFillTheAreaFromTheBottom() throws {
        show(MessageTableState(rows: [], loading: .latest))

        #expect(table.numberOfRows == 1)
        #expect(try cell(at: 0) is PlaceholderCell)
        #expect(table.rect(ofRow: 0).height >= clip.bounds.height)
        #expect(controller.isPinnedToBottom)
        #expect(table.rect(ofRow: 0).maxY <= clip.bounds.maxY)
        #expect(table.rect(ofRow: 0).minY < clip.bounds.minY)
    }

    @Test
    func theFirstPageReplacesThePlaceholdersPinnedToTheBottom() {
        show(MessageTableState(rows: [], loading: .latest))
        // Even after the reader moved the placeholders.
        clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY - 100))
        controller.scrollView.reflectScrolledClipView(clip)

        show(page)

        #expect(visibleRows.contains(table.numberOfRows - 1))
        #expect(abs(gapBelowTheRows) <= 0.5)
        #expect(controller.timeline.items.first?.id == .edge(.older))
    }

    @Test
    func aShortFirstPageSitsAtTheBottom() {
        show(MessageTableState(rows: [], loading: .latest))

        show(MessageTableState(rows: messages(3), reachedOldest: true))

        let bottom = clip.bounds.maxY - table.rect(ofRow: table.numberOfRows - 1).maxY
        #expect(abs(bottom - 8) <= 1)
    }

    @Test
    func aFailedFirstLoadOffersTryAgainInThePlaceholders() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(MessageTableState(rows: [], failedLoads: [.latest: .Network(kind: .timeout)]))

        let placeholders = try cell(at: 0)
        #expect(placeholders is PlaceholderCell)
        #expect(
            textFields(in: placeholders).contains {
                $0.stringValue == "Couldn't load messages." && !$0.isHiddenOrHasHiddenAncestor
            })
        try #require(buttons(in: placeholders).first { $0.title == "Try again" }).performClick(nil)

        #expect(actions.calls == ["latest"])
    }
}
