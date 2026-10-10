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

    let noon = berlinTime(2026, 10, 10, 12, 0)
    let table = CountingTableView()
    let controller: MessageTableController
    let window: NSWindow

    init() {
        controller = MessageTableController(
            tableView: table, calendar: berlin, now: { berlinTime(2026, 10, 10, 15, 0) })
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
