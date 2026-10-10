import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
final class TimeRefreshTests {
    final class Clock {
        var now = berlinTime(2026, 10, 10, 23, 59)
        var calendar = berlin
        var locale = Locale(identifier: "en_US")
    }

    let clock = Clock()
    let center = NotificationCenter()
    let table = MessageTableTests.CountingTableView()
    let controller: MessageTableController
    let window: NSWindow

    init() {
        controller = MessageTableController(
            tableView: table, calendar: { [clock] in clock.calendar },
            locale: { [clock] in clock.locale }, now: { [clock] in clock.now },
            pointer: { nil }, notifications: center)
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
    }

    func show(_ rows: [MessageListModel.Row]) {
        controller.show(MessageTableState(rows: rows))
        window.layoutIfNeeded()
    }

    // The observers run on the main queue.
    func post(_ name: Notification.Name) async {
        center.post(name: name, object: nil)
        try? await Task.sleep(for: .milliseconds(50))
        window.layoutIfNeeded()
    }

    func index(of raw: UInt64) -> Int {
        controller.timeline.items.firstIndex { $0.id == .message(MessageId(rawValue: raw)) } ?? -1
    }

    func texts(ofRow row: Int) -> [String] {
        guard let cell = table.view(atColumn: 0, row: row, makeIfNecessary: true) else {
            return []
        }
        cell.layoutSubtreeIfNeeded()
        return fields(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }.map(\.stringValue)
    }

    func fields(in view: NSView) -> [NSTextField] {
        view.subviews.flatMap { subview in
            (subview as? NSTextField).map { [$0] } ?? fields(in: subview)
        }
    }

    var dividers: Int {
        controller.timeline.items.filter { if case .day = $0 { true } else { false } }.count
    }

    @Test
    func midnightTurnsTodayIntoYesterday() async {
        var rows = [row(1, at: berlinTime(2026, 10, 10, 23, 58))]
        show(rows)
        #expect(texts(ofRow: index(of: 1)).contains("Today at 11:58\u{202F}PM"))

        clock.now = berlinTime(2026, 10, 11, 0, 1)
        await post(.NSCalendarDayChanged)
        #expect(texts(ofRow: index(of: 1)).contains("Yesterday at 11:58\u{202F}PM"))

        rows.append(row(2, at: berlinTime(2026, 10, 11, 0, 2)))
        show(rows)
        #expect(dividers == 2)
    }

    @Test
    func aNoticeRowTakesItsNewHeightWhenItsTimeChanges() async throws {
        let start = berlinTime(2026, 10, 10, 23, 58)
        let notice = row(1, by: author(1, "Mira Kowalczyk-Lindqvist"), at: start, kind: .userJoin)
        let rows = [notice] + (2...60).map { row(UInt64($0), at: start + Double($0)) }
        let message = notice.message
        let text = try #require(message.notice)
        let height = { (time: String, width: CGFloat) in
            NoticeCell.height(message, notice: text, time: time, width: width)
        }
        var width: CGFloat = 0
        for windowWidth in stride(from: 300, through: 1000, by: 2) {
            window.setContentSize(NSSize(width: CGFloat(windowWidth), height: 400))
            window.layoutIfNeeded()
            let column = table.tableColumns[0].width
            if height("Today at 11:58\u{202F}PM", column)
                != height("Yesterday at 11:58\u{202F}PM", column)
            {
                width = column
                break
            }
        }
        try #require(width > 0)
        show(rows)
        let clip = controller.scrollView.contentView
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: index(of: 40)).minY))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        let place = table.rect(ofRow: index(of: 40)).minY - clip.bounds.minY

        clock.now = berlinTime(2026, 10, 11, 0, 1)
        await post(.NSCalendarDayChanged)

        #expect(
            table.rect(ofRow: index(of: 1)).height == height("Yesterday at 11:58\u{202F}PM", width))
        #expect(abs(table.rect(ofRow: index(of: 40)).minY - clip.bounds.minY - place) <= 0.5)
    }

    @Test
    func aTimeZoneChangeMovesDayDividers() async {
        show([
            row(1, at: berlinTime(2026, 10, 10, 23, 30)),
            row(2, by: author(2), at: berlinTime(2026, 10, 11, 0, 30)),
        ])
        #expect(dividers == 2)

        var utc = Calendar(identifier: .gregorian)
        utc.timeZone = TimeZone(identifier: "UTC")!
        clock.calendar = utc
        await post(.NSSystemTimeZoneDidChange)

        #expect(dividers == 1)
        #expect(table.numberOfRows == controller.timeline.items.count)
        let shown = (0..<table.numberOfRows).map { row in
            (table.view(atColumn: 0, row: row, makeIfNecessary: true) as? NSTableCellView)?
                .objectValue as? MessageTimeline.ItemId
        }
        #expect(shown == controller.timeline.items.map(\.id))
    }

    @Test
    func aLocaleChangeReformatsTimes() async {
        clock.now = berlinTime(2026, 10, 10, 15, 0)
        show([row(1, at: berlinTime(2026, 10, 10, 14, 5))])
        #expect(texts(ofRow: index(of: 1)).contains("Today at 2:05\u{202F}PM"))

        clock.locale = Locale(identifier: "de_DE")
        await post(NSLocale.currentLocaleDidChangeNotification)

        #expect(texts(ofRow: index(of: 1)).contains("Today at 14:05"))
    }

    @Test
    func refreshingNeverReloadsTheWholeTable() async {
        show(
            (1...40).map { row(UInt64($0), at: berlinTime(2026, 10, 10, 23, 0) + Double($0) * 60) })
        clock.now = berlinTime(2026, 10, 11, 0, 1)
        await post(.NSCalendarDayChanged)
        await post(.NSSystemTimeZoneDidChange)

        #expect(table.fullReloads == 0)
    }
}
