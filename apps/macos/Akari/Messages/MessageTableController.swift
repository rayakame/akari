import AkariKit
import AppKit

/// Shows a channel's rows in an `NSTableView` by applying row updates: removals, insertions
/// and reloads of changed rows, never `reloadData()` (docs: D14 in the PR 3 plan).
final class MessageTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let tableView: MessageTableView
    let scrollView = NSScrollView()
    private(set) var timeline: MessageTimeline
    private let calendar: Calendar
    private let locale = Locale.current
    private let now: () -> Date
    private var filled = false
    // Follows the user's scrolling: at the bottom, new messages and re-wrapping keep it there.
    private var sticksToBottom = true

    init(
        tableView: MessageTableView = MessageTableView(), calendar: Calendar = .current,
        now: @escaping () -> Date = Date.init
    ) {
        self.tableView = tableView
        self.calendar = calendar
        self.now = now
        timeline = MessageTimeline(rows: [], calendar: calendar)
        super.init()
        configure()
    }

    func show(_ rows: [MessageListModel.Row], atPresent: Bool) {
        let next = MessageTimeline(rows: rows, calendar: calendar)
        let changes = TimelineChanges(from: timeline.items, to: next.items)
        guard !changes.isEmpty else {
            return
        }
        timeline = next
        tableView.beginUpdates()
        if !changes.removed.isEmpty {
            tableView.removeRows(at: changes.removed, withAnimation: [])
        }
        if !changes.inserted.isEmpty {
            tableView.insertRows(at: changes.inserted, withAnimation: [])
        }
        tableView.endUpdates()
        if !changes.reloaded.isEmpty {
            tableView.reloadData(forRowIndexes: changes.reloaded, columnIndexes: [0])
            tableView.noteHeightOfRows(withIndexesChanged: changes.reloaded)
        }
        let firstFill = !filled && !next.items.isEmpty
        if firstFill {
            filled = true
            sticksToBottom = true
            FirstChannelTimer.tableShowedRows()
        }
        if firstFill || (sticksToBottom && atPresent) {
            scrollToBottom()
        }
    }

    var isPinnedToBottom: Bool {
        scrollView.contentView.bounds.maxY >= tableView.frame.height - 2
    }

    // MARK: NSTableViewDataSource, NSTableViewDelegate

    func numberOfRows(in tableView: NSTableView) -> Int {
        timeline.items.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
        -> NSView?
    {
        switch timeline.items[row] {
        case .day(let day):
            let cell = reuse(DayDividerCell.identifier) as? DayDividerCell ?? DayDividerCell()
            cell.configure(MessageFormat.dayDivider(day, calendar: calendar, locale: locale))
            return cell
        case .message(let row, let startsGroup):
            let message = row.message
            let groupTime = MessageFormat.groupTime(
                message.timestamp, now: now(), calendar: calendar, locale: locale)
            let fullDate = MessageFormat.fullDate(
                message.timestamp, calendar: calendar, locale: locale)
            if let notice = message.notice {
                let cell = reuse(NoticeCell.identifier) as? NoticeCell ?? NoticeCell()
                cell.configure(message, notice: notice, time: groupTime, fullDate: fullDate)
                return cell
            }
            let cell = reuse(MessageCell.identifier) as? MessageCell ?? MessageCell()
            cell.configure(
                message, startsGroup: startsGroup, groupTime: groupTime,
                shortTime: MessageFormat.shortTime(
                    message.timestamp, calendar: calendar, locale: locale),
                fullDate: fullDate)
            return cell
        }
    }

    func tableView(_ tableView: NSTableView, shouldSelectRow row: Int) -> Bool {
        false
    }

    // MARK: Private

    private func configure() {
        let column = NSTableColumn(identifier: NSUserInterfaceItemIdentifier("messages"))
        column.resizingMask = .autoresizingMask
        tableView.addTableColumn(column)
        tableView.headerView = nil
        tableView.style = .plain
        tableView.usesAutomaticRowHeights = true
        tableView.rowHeight = 30
        tableView.intercellSpacing = .zero
        tableView.selectionHighlightStyle = .none
        tableView.gridStyleMask = []
        tableView.backgroundColor = Palette.chat
        tableView.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        tableView.dataSource = self
        tableView.delegate = self
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.backgroundColor = Palette.chat
        scrollView.automaticallyAdjustsContentInsets = false
        scrollView.contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: 16, right: 0)
        tableView.didLayout = { [weak self] in
            guard let self, self.filled, self.sticksToBottom, !self.isPinnedToBottom else {
                return
            }
            self.scrollToBottom()
        }
        let center = NotificationCenter.default
        center.addObserver(
            self, selector: #selector(userStartedScrolling),
            name: NSScrollView.willStartLiveScrollNotification, object: scrollView)
        for name in [
            NSScrollView.didLiveScrollNotification, NSScrollView.didEndLiveScrollNotification,
        ] {
            center.addObserver(
                self, selector: #selector(userScrolled), name: name, object: scrollView)
        }
    }

    // While the user scrolls, nothing pulls the list back down.
    @objc private func userStartedScrolling(_ notification: Notification) {
        sticksToBottom = false
    }

    @objc private func userScrolled(_ notification: Notification) {
        sticksToBottom = isPinnedToBottom
    }

    private func reuse(_ identifier: NSUserInterfaceItemIdentifier) -> NSView? {
        tableView.makeView(withIdentifier: identifier, owner: nil)
    }

    private func scrollToBottom() {
        let last = tableView.numberOfRows - 1
        guard last >= 0 else {
            return
        }
        tableView.scrollRowToVisible(last)
        let clip = scrollView.contentView
        let insets = scrollView.contentInsets
        let bottom = max(-insets.top, tableView.frame.height - clip.bounds.height + insets.bottom)
        clip.scroll(to: NSPoint(x: clip.bounds.minX, y: bottom))
        scrollView.reflectScrolledClipView(clip)
    }
}

/// Reports each finished layout. While measuring rows, NSTableView re-anchors the scroll
/// position inside its layout, so pinning to the bottom has to happen after it.
class MessageTableView: NSTableView {
    var didLayout: (() -> Void)?

    override func layout() {
        super.layout()
        didLayout?()
    }
}
