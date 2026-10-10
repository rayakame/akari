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
    // Where the pointer is, in window coordinates; nil when hover shouldn't show.
    private let pointer: () -> NSPoint?
    private var filled = false
    private var sticksToBottom = true
    private var atPresent = true
    private var anchor: (id: MessageTimeline.ItemId, offset: CGFloat)?
    // Clip moves from the controller's own updates and scrolls aren't the user's.
    private var ownChanges = 0
    private var lastClipOrigin = NSPoint.zero
    private var hovered: MessageTimeline.ItemId?
    // Paging posts willStart and didLive but no didEnd, so a live scroll also ends when its
    // notifications stop.
    private var liveScrollEnded = true
    private var lastLiveScroll = Date.distantPast
    private var liveScrollGeneration = 0
    private static let liveScrollQuiet: TimeInterval = 0.5

    init(
        tableView: MessageTableView = MessageTableView(), calendar: Calendar = .current,
        now: @escaping () -> Date = Date.init, pointer: (() -> NSPoint?)? = nil
    ) {
        self.tableView = tableView
        self.calendar = calendar
        self.now = now
        self.pointer =
            pointer ?? { [weak tableView] in
                guard let window = tableView?.window, window.isKeyWindow else {
                    return nil
                }
                return window.mouseLocationOutsideOfEventStream
            }
        timeline = MessageTimeline(rows: [], calendar: calendar)
        super.init()
        configure()
    }

    func show(_ rows: [MessageListModel.Row], atPresent: Bool) {
        // Before the early return: leaving the present ends pinning even without new rows.
        self.atPresent = atPresent
        let next = MessageTimeline(rows: rows, calendar: calendar)
        let changes = TimelineChanges(from: timeline.items, to: next.items)
        guard !changes.isEmpty else {
            return
        }
        let firstFill = !filled && !next.items.isEmpty
        let pin = firstFill || pinsToBottom
        if !pin && !isLiveScrolling {
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
            LaunchLog.mark("first channel rendered")
        }
        // Nothing scrolls under the user's hands; settling catches up once they let go.
        if !isLiveScrolling {
            if pin {
                scrollToBottom()
            } else {
                restoreAnchor()
            }
        }
        updateHover()
    }

    // Away from the present, rows below the reader are an older page's continuation, not news.
    private var pinsToBottom: Bool {
        sticksToBottom && atPresent
    }

    func pointerMoved() {
        updateHover()
    }

    private var isLiveScrolling: Bool {
        !liveScrollEnded && Date().timeIntervalSince(lastLiveScroll) < Self.liveScrollQuiet
    }

    var isPinnedToBottom: Bool {
        scrollView.contentView.bounds.maxY >= tableView.contentHeight - 2
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
            cell.isHovered = item.id == hovered
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
        scrollView.contentView = BottomClipView()
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.backgroundColor = Palette.chat
        scrollView.automaticallyAdjustsContentInsets = false
        // The official client's list stops at both ends.
        scrollView.verticalScrollElasticity = .none
        scrollView.horizontalScrollElasticity = .none
        scrollView.contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: 16, right: 0)
        tableView.didLayout = { [weak self] in
            self?.settle()
        }
        tableView.pointerMoved = { [weak self] in
            self?.updateHover()
        }
        let center = NotificationCenter.default
        center.addObserver(
            self, selector: #selector(liveScrollStarted),
            name: NSScrollView.willStartLiveScrollNotification, object: scrollView)
        center.addObserver(
            self, selector: #selector(liveScrolled), name: NSScrollView.didLiveScrollNotification,
            object: scrollView)
        center.addObserver(
            self, selector: #selector(liveScrollEnded(_:)),
            name: NSScrollView.didEndLiveScrollNotification, object: scrollView)
        // Wheel, keyboard and accessibility scrolls post no live-scroll notifications; every
        // scroll moves the clip view.
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(clipMoved), name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView)
    }

    // After a layout, a live scroll's end, or its quiet timeout: pin or keep the reader's place.
    private func settle() {
        guard filled, !isLiveScrolling else {
            return
        }
        ownChanges += 1
        defer { ownChanges -= 1 }
        if pinsToBottom {
            if !isPinnedToBottom {
                scrollToBottom()
            }
        } else {
            restoreAnchor()
        }
        alignShortContent()
    }

    @objc private func liveScrollStarted(_ notification: Notification) {
        liveScrollEnded = false
        liveScrolled(notification)
    }

    @objc private func liveScrolled(_ notification: Notification) {
        lastLiveScroll = Date()
        liveScrollGeneration += 1
        let generation = liveScrollGeneration
        Task { [weak self] in
            try? await Task.sleep(for: .seconds(Self.liveScrollQuiet + 0.05))
            guard let self, self.liveScrollGeneration == generation else {
                return
            }
            self.settle()
        }
    }

    @objc private func liveScrollEnded(_ notification: Notification) {
        liveScrollEnded = true
        settle()
    }

    private func updateHover() {
        var row = -1
        if let location = pointer() {
            let point = tableView.convert(location, from: nil)
            if tableView.visibleRect.contains(point) {
                row = tableView.row(at: point)
            }
        }
        hovered = timeline.items.indices.contains(row) ? timeline.items[row].id : nil
        let visible = tableView.rows(in: tableView.visibleRect)
        for index in visible.location..<(visible.location + visible.length) {
            let cell = tableView.view(atColumn: 0, row: index, makeIfNecessary: false)
            (cell as? MessageCell)?.isHovered = index == row
        }
    }

    @objc private func clipMoved(_ notification: Notification) {
        let origin = scrollView.contentView.bounds.origin
        defer { lastClipOrigin = origin }
        // Content moves under a still pointer on every scroll, the controller's own included.
        updateHover()
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

    // The table's frame stretches to fill the view, so new rows never re-constrain the clip
    // view on their own.
    private func alignShortContent() {
        let clip = scrollView.contentView
        let origin = clip.constrainBoundsRect(clip.bounds).origin
        guard origin != clip.bounds.origin else {
            return
        }
        clip.setBoundsOrigin(origin)
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
        let bottom = max(-insets.top, tableView.contentHeight - clip.bounds.height + insets.bottom)
        clip.scroll(to: NSPoint(x: clip.bounds.minX, y: bottom))
        scrollView.reflectScrolledClipView(clip)
    }
}

// A conversation shorter than the view sits at its bottom, as in Discord: a negative origin in
// the flipped clip view moves the table down.
final class BottomClipView: NSClipView {
    override func constrainBoundsRect(_ proposedBounds: NSRect) -> NSRect {
        var bounds = super.constrainBoundsRect(proposedBounds)
        if let table = documentView as? MessageTableView {
            let content = table.contentHeight + contentInsets.bottom
            if content < bounds.height {
                bounds.origin.y = content - bounds.height
            }
        }
        return bounds
    }
}

// NSTableView re-anchors the scroll position inside its layout, so pinning comes after it.
class MessageTableView: NSTableView {
    var didLayout: (() -> Void)?
    var pointerMoved: (() -> Void)?
    private(set) var isLayingOut = false

    // The rows' extent; the frame itself stretches to fill a taller clip view.
    var contentHeight: CGFloat {
        numberOfRows > 0 ? rect(ofRow: numberOfRows - 1).maxY : 0
    }

    override func layout() {
        isLayingOut = true
        super.layout()
        isLayingOut = false
        didLayout?()
    }

    override func updateTrackingAreas() {
        super.updateTrackingAreas()
        for area in trackingAreas where area.owner === self {
            removeTrackingArea(area)
        }
        addTrackingArea(
            NSTrackingArea(
                rect: .zero,
                options: [.mouseMoved, .mouseEnteredAndExited, .activeInKeyWindow, .inVisibleRect],
                owner: self))
    }

    override func mouseMoved(with event: NSEvent) {
        super.mouseMoved(with: event)
        pointerMoved?()
    }

    override func mouseEntered(with event: NSEvent) {
        super.mouseEntered(with: event)
        pointerMoved?()
    }

    override func mouseExited(with event: NSEvent) {
        super.mouseExited(with: event)
        pointerMoved?()
    }
}
