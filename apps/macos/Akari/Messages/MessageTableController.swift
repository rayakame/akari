import AkariKit
import AppKit

// Row updates only: a full reloadData() would lose the scroll position.
final class MessageTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let tableView: MessageTableView
    let scrollView = NSScrollView()
    private(set) var timeline: MessageTimeline
    private let calendar: Calendar
    private let locale = Locale.current
    private let now: () -> Date
    private var filled = false
    private var sticksToBottom = true
    private var anchor: (id: MessageTimeline.ItemId, offset: CGFloat)?
    // Clip moves from the controller's own updates and scrolls aren't the user's.
    private var ownChanges = 0
    private var lastClipOrigin = NSPoint.zero

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
        let firstFill = !filled && !next.items.isEmpty
        let pin = firstFill || (sticksToBottom && atPresent)
        if !pin {
            anchor = visibleAnchor(skipping: changes.removed) ?? anchor
        }
        ownChanges += 1
        defer { ownChanges -= 1 }
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
        if firstFill {
            filled = true
            sticksToBottom = true
            FirstChannelTimer.tableShowedRows()
        }
        if pin {
            scrollToBottom()
        } else {
            restoreAnchor()
        }
    }

    var isPinnedToBottom: Bool {
        scrollView.contentView.bounds.maxY >= tableView.frame.height - 2
    }

    func numberOfRows(in tableView: NSTableView) -> Int {
        timeline.items.count
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
        -> NSView?
    {
        let cell = cell(for: timeline.items[row])
        cell.objectValue = timeline.items[row].id
        return cell
    }

    private func cell(for item: MessageTimeline.Item) -> NSTableCellView {
        switch item {
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
            guard let self, self.filled else {
                return
            }
            self.ownChanges += 1
            defer { self.ownChanges -= 1 }
            if self.sticksToBottom {
                if !self.isPinnedToBottom {
                    self.scrollToBottom()
                }
            } else {
                self.restoreAnchor()
            }
        }
        // Wheel, keyboard and accessibility scrolls post no live-scroll notifications; every
        // scroll moves the clip view.
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(clipMoved), name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView)
    }

    @objc private func clipMoved(_ notification: Notification) {
        let origin = scrollView.contentView.bounds.origin
        defer { lastClipOrigin = origin }
        // The table re-anchors inside its own layout; that isn't the user scrolling either.
        guard ownChanges == 0, !tableView.isLayingOut, origin != lastClipOrigin else {
            return
        }
        sticksToBottom = isPinnedToBottom
        anchor = sticksToBottom ? nil : visibleAnchor(skipping: [])
    }

    // Read before the update: `timeline` and the table still hold the old rows.
    private func visibleAnchor(skipping removed: IndexSet) -> (
        id: MessageTimeline.ItemId, offset: CGFloat
    )? {
        let top = scrollView.contentView.bounds.minY
        let visible = tableView.rows(in: scrollView.contentView.bounds)
        for row in visible.location..<(visible.location + visible.length)
        where !removed.contains(row) && row < timeline.items.count {
            return (timeline.items[row].id, tableView.rect(ofRow: row).minY - top)
        }
        return nil
    }

    private func restoreAnchor() {
        guard let anchor, let row = timeline.items.firstIndex(where: { $0.id == anchor.id }) else {
            return
        }
        let clip = scrollView.contentView
        let y = tableView.rect(ofRow: row).minY - anchor.offset
        guard abs(clip.bounds.minY - y) > 0.5 else {
            return
        }
        clip.scroll(to: NSPoint(x: clip.bounds.minX, y: y))
        scrollView.reflectScrolledClipView(clip)
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

// NSTableView re-anchors the scroll position inside its layout, so pinning comes after it.
class MessageTableView: NSTableView {
    var didLayout: (() -> Void)?
    private(set) var isLayingOut = false

    override func layout() {
        isLayingOut = true
        super.layout()
        isLayingOut = false
        didLayout?()
    }
}
