import AkariKit
import AppKit

// Row updates only: a full reloadData() would lose the scroll position.
final class MessageTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let tableView: MessageTableView
    let scrollView = NSScrollView()
    private let clipView = BottomClipView()
    private static var created = 0
    // Tells tables apart in the scroll log, so a replaced scroll view shows.
    private let number: Int
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
    private static let edgeHeight: CGFloat = 48
    // Exact heights before a row shows: automatic heights start as estimates and change when a
    // fling first reaches a row, which moves the bottom under it.
    private var rowHeights: [MessageTimeline.ItemId: CGFloat] = [:]
    private var rowHeightsWidth: CGFloat = 0

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
        Self.created += 1
        number = Self.created
        super.init()
        configure()
        ScrollLog.log(
            "table \(number) created, elasticity "
                + ScrollLog.name(scrollView.verticalScrollElasticity))
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
        for index in changes.removed {
            rowHeights[timeline.items[index].id] = nil
        }
        for index in changes.reloaded {
            rowHeights[next.items[index].id] = nil
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

    func tableView(_ tableView: NSTableView, heightOfRow row: Int) -> CGFloat {
        let width = tableView.tableColumns.first?.width ?? 0
        guard width > 0 else {
            return 1
        }
        if width != rowHeightsWidth {
            rowHeights.removeAll()
            rowHeightsWidth = width
        }
        let item = timeline.items[row]
        if let height = rowHeights[item.id] {
            return height
        }
        let height = height(of: item, width: width)
        rowHeights[item.id] = height
        return height
    }

    private func height(of item: MessageTimeline.Item, width: CGFloat) -> CGFloat {
        switch item {
        case .edge:
            return Self.edgeHeight
        case .day(let day):
            return DayDividerCell.height(
                MessageFormat.dayDivider(day, calendar: calendar, locale: locale))
        case .message(let row, let startsGroup):
            let message = row.message
            if let notice = message.notice {
                let time = MessageFormat.groupTime(
                    message.timestamp, now: now(), calendar: calendar, locale: locale)
                return NoticeCell.height(message, notice: notice, time: time, width: width)
            }
            return MessageCell.height(message, startsGroup: startsGroup, width: width)
        }
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
        -> NSView?
    {
        let cell = cell(for: timeline.items[row])
        cell.objectValue = timeline.items[row].id
        return cell
    }

    func cell(for item: MessageTimeline.Item) -> NSTableCellView {
        switch item {
        case .edge:
            return NSTableCellView()
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
        tableView.usesAutomaticRowHeights = false
        tableView.intercellSpacing = .zero
        tableView.selectionHighlightStyle = .none
        tableView.gridStyleMask = []
        tableView.backgroundColor = Palette.chat
        tableView.columnAutoresizingStyle = .uniformColumnAutoresizingStyle
        tableView.dataSource = self
        tableView.delegate = self
        scrollView.contentView = clipView
        scrollView.documentView = tableView
        scrollView.hasVerticalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.backgroundColor = Palette.chat
        scrollView.automaticallyAdjustsContentInsets = false
        // The native bounce at both ends; the list never scrolls sideways.
        scrollView.verticalScrollElasticity = .automatic
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
        center.addObserver(
            self, selector: #selector(columnResized), name: NSTableView.columnDidResizeNotification,
            object: tableView)
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

    @objc private func columnResized(_ notification: Notification) {
        guard let width = tableView.tableColumns.first?.width, width != rowHeightsWidth else {
            return
        }
        // The table keeps no row in place when heights change; settle() restores this one.
        if !pinsToBottom, let place = visibleAnchor(skipping: []) {
            anchor = place
        }
        tableView.noteHeightOfRows(
            withIndexesChanged: IndexSet(integersIn: 0..<tableView.numberOfRows))
    }

    @objc private func liveScrollStarted(_ notification: Notification) {
        ScrollLog.log("table \(number) live scroll starts")
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
            ScrollLog.log("table \(self.number) live scroll went quiet")
            self.settle()
        }
    }

    @objc private func liveScrollEnded(_ notification: Notification) {
        ScrollLog.log("table \(number) live scroll ends")
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
        ScrollLog.log(
            "table \(number) y \(origin.y) range \(clipView.originRange) rows "
                + "\(tableView.contentHeight) frame \(tableView.frame.height) clip "
                + "\(clipView.bounds.height) elasticity "
                + "\(ScrollLog.name(scrollView.verticalScrollElasticity)) live \(isLiveScrolling) "
                + "own \(ownChanges > 0) layout \(tableView.isLayingOut)")
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
        let range = clipView.originRange
        let target = tableView.rect(ofRow: row).minY - anchor.offset
        let y = min(max(target, range.lowerBound), range.upperBound)
        guard abs(clipView.bounds.minY - y) > 0.5 else {
            return
        }
        ScrollLog.log("table \(number) restores its anchor: y \(clipView.bounds.minY) to \(y)")
        clipView.scroll(to: NSPoint(x: clipView.bounds.minX, y: y))
        scrollView.reflectScrolledClipView(clipView)
    }

    // The table's frame stretches to fill the view, so new rows never re-constrain the clip view
    // on their own. Longer lists are left alone: their origin may be mid-bounce.
    private func alignShortContent() {
        let range = clipView.originRange
        guard range.lowerBound == range.upperBound, clipView.bounds.minY != range.lowerBound else {
            return
        }
        ScrollLog.log(
            "table \(number) realigns: y \(clipView.bounds.minY) to \(range.lowerBound)")
        clipView.setBoundsOrigin(NSPoint(x: clipView.bounds.minX, y: range.lowerBound))
        scrollView.reflectScrolledClipView(clipView)
    }

    private func reuse(_ identifier: NSUserInterfaceItemIdentifier) -> NSView? {
        tableView.makeView(withIdentifier: identifier, owner: nil)
    }

    private func scrollToBottom() {
        let bottom = clipView.originRange.upperBound
        guard tableView.numberOfRows > 0, clipView.bounds.minY != bottom else {
            return
        }
        ScrollLog.log(
            "table \(number) scrolls to the bottom: y \(clipView.bounds.minY) to \(bottom)")
        clipView.scroll(to: NSPoint(x: clipView.bounds.minX, y: bottom))
        scrollView.reflectScrolledClipView(clipView)
    }
}

// A conversation shorter than the view sits at its bottom, as in Discord: a negative origin in
// the flipped clip view moves the table down.
final class BottomClipView: NSClipView {
    // Where our own scrolls may go; a single origin, below zero, while the rows are shorter
    // than the view. The elastic bounce goes past it.
    var originRange: ClosedRange<CGFloat> {
        let rows = (documentView as? MessageTableView)?.contentHeight ?? 0
        let top = -contentInsets.top
        let bottom = rows + contentInsets.bottom - bounds.height
        return bottom < top ? bottom...bottom : top...bottom
    }

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
