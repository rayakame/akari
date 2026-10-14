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

    // Timers that fire when the test says so.
    final class Timers {
        struct Entry {
            let date: Date
            let fire: @MainActor () -> Void
            var cancelled = false
        }

        var entries: [Entry] = []
        var active: [Entry] { entries.filter { !$0.cancelled } }

        func schedule(_ date: Date, _ fire: @escaping @MainActor () -> Void) -> () -> Void {
            let index = entries.count
            entries.append(Entry(date: date, fire: fire))
            return { [weak self] in self?.entries[index].cancelled = true }
        }

        func fire() {
            let due = active
            for index in entries.indices {
                entries[index].cancelled = true
            }
            for entry in due {
                entry.fire()
            }
        }
    }

    final class Screen {
        var shows = true
    }

    let clock = Clock()
    let center = NotificationCenter()
    let timers = Timers()
    let screen = Screen()
    let table = MessageTableTests.CountingTableView()
    let controller: MessageTableController
    let window: NSWindow

    init() {
        controller = MessageTableController(
            tableView: table, calendar: { [clock] in clock.calendar },
            locale: { [clock] in clock.locale }, now: { [clock] in clock.now },
            pointer: { nil }, notifications: center,
            schedule: { [timers] date, fire in timers.schedule(date, fire) },
            onScreen: { [screen] _ in screen.shows })
        window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 400),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
    }

    func show(_ rows: [MessageListModel.Row]) {
        controller.show(MessageTableState(rows: rows, reachedOldest: true))
        window.layoutIfNeeded()
    }

    func body(ofRow row: Int) -> String {
        guard let cell = table.view(atColumn: 0, row: row, makeIfNecessary: true) else {
            return ""
        }
        return MessageListFixture.messageText(in: cell)?.string ?? ""
    }

    var visibleRows: Range<Int> {
        let rows = table.rows(in: table.visibleRect)
        return rows.location..<NSMaxRange(rows)
    }

    func seconds(_ date: Date) -> Int64 {
        Int64(date.timeIntervalSince1970)
    }

    // Every other row counts down to its own time, a few minutes apart.
    func relativeRows(_ count: Int) -> [MessageListModel.Row] {
        (1...count).map { raw in
            let at = berlinTime(2026, 10, 10, 23, 0) + Double(raw) * 30
            let content =
                raw % 2 == 0
                ? "due <t:\(seconds(clock.now) + Int64(raw) * 200 + 30):R>" : "plain \(raw)"
            return row(UInt64(raw), by: author(UInt64(raw % 2 + 1)), at: at, content: content)
        }
    }

    @Test
    func oneTimerServesAllRelativeTimes() {
        let rows = relativeRows(40)
        show(rows)
        let visible = visibleRows
        let times = TimestampText(
            locale: clock.locale, timeZone: clock.calendar.timeZone, calendar: clock.calendar)
        let earliest = visible.compactMap { index -> Date? in
            guard case .message(let row, _) = controller.timeline.items[index],
                row.message.content.hasPrefix("due")
            else {
                return nil
            }
            return MessageText.build(row.message, revealed: [], now: clock.now, times: times)
                .nextChange
        }.min()

        #expect(timers.active.count == 1)
        #expect(timers.active.first?.date == earliest)

        table.forget()
        let before = visible.map(body(ofRow:))
        clock.now = earliest ?? clock.now
        timers.fire()
        window.layoutIfNeeded()

        let reloaded = table.reloaded.reduce(IndexSet()) { $0.union($1) }
        #expect(!reloaded.isEmpty)
        #expect(
            reloaded.allSatisfy { index in
                guard case .message(let row, _) = controller.timeline.items[index] else {
                    return false
                }
                return row.message.content.hasPrefix("due")
            })
        #expect(table.fullReloads == 0)
        #expect(visible.map(body(ofRow:)) != before)
        #expect(timers.active.count == 1)
    }

    @Test
    func aRelativeTimeThatWrapsKeepsTheAnchor() throws {
        // A row whose text wraps once its relative time turns from minutes to seconds.
        let due = seconds(clock.now) + 61
        let fire = clock.now + 2
        let times = TimestampText(
            locale: clock.locale, timeZone: clock.calendar.timeZone, calendar: clock.calendar)
        let width = table.tableColumns[0].width - 88
        let wrapping = try #require(
            (1...160).lazy.map { "x" + String(repeating: "a", count: $0) + " <t:\(due):R>" }
                .first { content in
                    let message = row(1, at: self.clock.now, content: content).message
                    let height = { (now: Date) in
                        TextMeasurer.shared.height(
                            MessageText.build(message, revealed: [], now: now, times: times)
                                .text, width: width)
                    }
                    return height(self.clock.now) != height(fire)
                })
        var rows = (1...60).map { raw in
            row(UInt64(raw), by: author(UInt64(raw % 2 + 1)), at: clock.now + Double(raw) * 30)
        }
        rows[19] = row(20, by: author(1), at: clock.now + 600, content: wrapping)
        rows[24] = row(25, by: author(2), at: clock.now + 750, content: "soon <t:\(due):R>")
        show(rows)
        let clip = controller.scrollView.contentView
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: index(of: 21)).minY))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        let place = table.rect(ofRow: index(of: 21)).minY - clip.bounds.minY
        let height = table.rect(ofRow: index(of: 20)).height

        clock.now = fire
        timers.fire()
        window.layoutIfNeeded()

        #expect(table.rect(ofRow: index(of: 20)).height != height)
        #expect(table.rect(ofRow: index(of: 21)).minY - clip.bounds.minY == place)
    }

    // A row's relative text can change while it's off screen, with the timer stopped.
    @Test
    func aRowWhoseTimeChangedOffScreenComesBackAtItsNewHeight() throws {
        let due = seconds(clock.now) + 61
        let times = TimestampText(
            locale: clock.locale, timeZone: clock.calendar.timeZone, calendar: clock.calendar)
        let width = table.tableColumns[0].width
        let height = { (message: Message, now: Date) in
            MessageCell.height(
                message,
                body: MessageText.build(message, revealed: [], now: now, times: times).text,
                startsGroup: true, width: width)
        }
        let later = clock.now + 120
        let wrapping = try #require(
            (1...160).lazy.map { "x" + String(repeating: "a", count: $0) + " <t:\(due):R>" }
                .first { content in
                    let message = row(1, at: self.clock.now, content: content).message
                    return height(message, self.clock.now) != height(message, later)
                })
        var rows = (1...80).map { raw in
            row(UInt64(raw), by: author(UInt64(raw % 2 + 1)), at: clock.now + Double(raw) * 30)
        }
        rows[1] = row(2, by: author(2), at: clock.now + 60, content: wrapping)
        show(rows)
        table.scrollRowToVisible(table.numberOfRows - 1)
        window.layoutIfNeeded()
        #expect(timers.active.isEmpty)

        clock.now = later
        table.scrollRowToVisible(0)
        window.layoutIfNeeded()
        timers.fire()
        window.layoutIfNeeded()

        let shown = table.rect(ofRow: index(of: 2)).height
        guard case .message(let edited, let startsGroup) = controller.timeline.items[index(of: 2)]
        else {
            Issue.record("not a message")
            return
        }
        // It's the first message after midnight, right below its day's divider.
        let followsDay =
            if case .day = controller.timeline.items[index(of: 2) - 1] { true } else { false }
        #expect(
            shown
                == MessageCell.height(
                    edited.message,
                    body: MessageText.build(edited.message, revealed: [], now: later, times: times)
                        .text,
                    startsGroup: startsGroup, followsDay: followsDay, width: width))
    }

    @Test
    func theTimerStopsWithoutRelativeTimesOnScreen() {
        var rows = relativeRows(6)
        rows += (7...80).map { raw in
            row(UInt64(raw), by: author(UInt64(raw % 2 + 1)), at: berlinTime(2026, 10, 10, 23, 30) + Double(raw) * 10)
        }
        show(rows)
        #expect(timers.active.isEmpty)

        table.scrollRowToVisible(0)
        window.layoutIfNeeded()
        #expect(timers.active.count == 1)

        table.scrollRowToVisible(table.numberOfRows - 1)
        window.layoutIfNeeded()
        #expect(timers.active.isEmpty)
    }

    @Test
    func theTimerWaitsWhileTheWindowIsHidden() {
        let rows = relativeRows(40)
        show(rows)
        let date = timers.active.first?.date
        #expect(timers.active.count == 1)

        for (hidden, shown) in [
            (NSWindow.didChangeOcclusionStateNotification, NSWindow.didChangeOcclusionStateNotification),
            (NSWindow.didMiniaturizeNotification, NSWindow.didDeminiaturizeNotification),
        ] {
            screen.shows = false
            center.post(name: hidden, object: window)
            #expect(timers.active.isEmpty, "\(hidden)")
            show(rows)
            #expect(timers.active.isEmpty, "\(hidden)")

            screen.shows = true
            center.post(name: shown, object: window)
            #expect(timers.active.map(\.date) == [date], "\(shown)")
        }
    }

    @Test
    func theTimerGoesWithTheList() {
        let timers = Timers()
        weak var gone: MessageTableController?
        autoreleasepool {
            let controller = MessageTableController(
                calendar: { [clock] in clock.calendar }, locale: { [clock] in clock.locale },
                now: { [clock] in clock.now }, pointer: { nil }, notifications: center,
                schedule: { date, fire in timers.schedule(date, fire) }, onScreen: { _ in true })
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled],
                backing: .buffered, defer: false)
            window.contentView = controller.scrollView
            controller.show(MessageTableState(rows: relativeRows(10), reachedOldest: true))
            window.layoutIfNeeded()
            #expect(timers.active.count == 1)
            window.contentView = nil
            gone = controller
        }

        #expect(gone == nil)
        #expect(timers.active.isEmpty)
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
        return texts(in: cell)
    }

    func texts(in cell: NSView) -> [String] {
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
        #expect(texts(ofRow: index(of: 1)).contains("11:58\u{202F}PM"))

        clock.now = berlinTime(2026, 10, 11, 0, 1)
        await post(.NSCalendarDayChanged)
        #expect(texts(ofRow: index(of: 1)).contains("Yesterday at 11:58\u{202F}PM"))

        rows.append(row(2, at: berlinTime(2026, 10, 11, 0, 2)))
        show(rows)
        #expect(dividers == 2)
    }

    // AppKit keeps row views ready beyond the visible rows; a slight scroll shows them as they are.
    @Test
    func midnightReachesRowViewsPreparedOutOfSight() async throws {
        let start = berlinTime(2026, 10, 10, 23, 58)
        // Every author change starts a group, so every row shows its time.
        show((1...60).map { row(UInt64($0), by: author(UInt64($0 % 2 + 1)), at: start + 1) })
        let first = index(of: 1)
        try #require(!table.visibleRect.intersects(table.rect(ofRow: first)))
        // As AppKit's overdraw does ahead of a scroll.
        let prepared = try #require(table.view(atColumn: 0, row: first, makeIfNecessary: true))
        window.layoutIfNeeded()
        try #require(table.view(atColumn: 0, row: first, makeIfNecessary: false) === prepared)
        #expect(texts(in: prepared).contains("11:58\u{202F}PM"))
        // An offscreen table drops it at its next tile, which the refresh brings; on screen it
        // stays. So it's read as the refresh reloads it.
        var shown: [String] = []
        table.afterReload = { [table] rows in
            if rows.contains(first),
                let cell = table.view(atColumn: 0, row: first, makeIfNecessary: false)
            {
                shown = self.texts(in: cell)
            }
        }

        clock.now = berlinTime(2026, 10, 11, 0, 1)
        await post(.NSCalendarDayChanged)

        #expect(shown.contains("Yesterday at 11:58\u{202F}PM"))
    }

    @Test
    func aNoticeRowTakesItsNewHeightWhenItsTimeChanges() async throws {
        let start = berlinTime(2026, 10, 10, 23, 58)
        let notice = row(1, by: author(1, "Mira Kowalczyk-Lindqvist"), at: start, kind: .userJoin)
        let rows = [notice] + (2...60).map { row(UInt64($0), at: start + Double($0)) }
        let message = notice.message
        let text = try #require(message.notice)
        let height = { (time: String, width: CGFloat) in
            // The list's first message, right below its day's divider.
            NoticeCell.height(message, notice: text, time: time, followsDay: true, width: width)
        }
        var width: CGFloat = 0
        for windowWidth in stride(from: 300, through: 1000, by: 2) {
            window.setContentSize(NSSize(width: CGFloat(windowWidth), height: 400))
            window.layoutIfNeeded()
            let column = table.tableColumns[0].width
            if height("11:58\u{202F}PM", column)
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
        #expect(texts(ofRow: index(of: 1)).contains("2:05\u{202F}PM"))

        clock.locale = Locale(identifier: "de_DE")
        await post(NSLocale.currentLocaleDidChangeNotification)

        #expect(texts(ofRow: index(of: 1)).contains("14:05"))
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
