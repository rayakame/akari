import AkariKit
import AppKit

struct MessageTableState: Equatable {
    var rows: [MessageListModel.Row]
    var atPresent = true
    var reachedOldest = false
    var loading: MessageListModel.Load?
    var failedLoads: [MessageListModel.Load: RequestError] = [:]
    var beginning = ""
}

// Row updates only: a full reloadData() would lose the scroll position.
final class MessageTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    let tableView: MessageTableView
    let scrollView = NSScrollView()
    // Strong: the bridge that implements it holds nothing that leads back here.
    var actions: MessageListActions?
    private let clipView = BottomClipView()
    private static var created = 0
    // Tells tables apart in the scroll log, so a replaced scroll view shows.
    private let number: Int
    private(set) var timeline: MessageTimeline
    private let calendarSource: () -> Calendar
    private let localeSource: () -> Locale
    private var calendar: Calendar
    private var locale: Locale
    private let now: () -> Date
    private let notifications: NotificationCenter
    nonisolated(unsafe) private var timeObservers: [NSObjectProtocol] = []
    // Where the pointer is, in window coordinates; nil when hover shouldn't show.
    private let pointer: () -> NSPoint?
    private var filled = false
    private var sticksToBottom = true
    private var atPresent = true
    private var state = MessageTableState(rows: [])
    // A page that lands while the list is bounced past its top waits for the bounce to end:
    // moving the clip under AppKit's rubber band would fight it.
    private var heldState: MessageTableState?
    // A request since the last state, so scrolling doesn't ask twice before the model answers.
    private var requested = false
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
    private static let bottomInset: CGFloat = 8
    // Exact heights before a row shows: automatic heights start as estimates and change when a
    // fling first reaches a row, which moves the bottom under it.
    private var rowHeights: [MessageTimeline.ItemId: CGFloat] = [:]
    private var rowHeightsWidth: CGFloat = 0
    private var placeholderHeight = PlaceholderCell.height(forViewHeight: 0)

    init(
        tableView: MessageTableView = MessageTableView(),
        calendar: @escaping () -> Calendar = { .autoupdatingCurrent },
        locale: @escaping () -> Locale = { .autoupdatingCurrent },
        now: @escaping () -> Date = Date.init, pointer: (() -> NSPoint?)? = nil,
        notifications: NotificationCenter = .default
    ) {
        self.tableView = tableView
        calendarSource = calendar
        localeSource = locale
        self.calendar = calendar()
        self.locale = locale()
        self.now = now
        self.notifications = notifications
        self.pointer =
            pointer ?? { [weak tableView] in
                guard let window = tableView?.window, window.isKeyWindow else {
                    return nil
                }
                return window.mouseLocationOutsideOfEventStream
            }
        timeline = MessageTimeline(rows: [], calendar: self.calendar)
        Self.created += 1
        number = Self.created
        super.init()
        configure()
        ScrollLog.log(
            "table \(number) created, elasticity "
                + ScrollLog.name(scrollView.verticalScrollElasticity))
    }

    func show(_ state: MessageTableState) {
        if isLiveScrolling && isPastTheTop {
            heldState = state
            return
        }
        heldState = nil
        let previous = self.state
        self.state = state
        requested = false
        // Before the early return: leaving the present ends pinning even without new rows.
        atPresent = state.atPresent
        // The jump bar covers the bottom while older messages are shown.
        scrollView.contentInsets.bottom =
            Self.bottomInset + (atPresent ? 0 : JumpBar.height + JumpBar.margin)
        let next = timeline(of: state)
        let changes = TimelineChanges(from: timeline.items, to: next.items)
        if !changes.isEmpty {
            apply(next, changes)
        }
        if previous.loading != state.loading || previous.failedLoads != state.failedLoads
            || previous.reachedOldest != state.reachedOldest
            || previous.beginning != state.beginning
        {
            configureEdges()
        }
        askForMore()
    }

    deinit {
        for observer in timeObservers {
            notifications.removeObserver(observer)
        }
    }

    // Every height is measured again: system rows carry the time in their wrapped text.
    func refreshTimes() {
        calendar = calendarSource()
        locale = localeSource()
        let place = pinsToBottom ? nil : visibleAnchor(skipping: [])
        if let place {
            anchor = place
        }
        rowHeights.removeAll()
        let next = timeline(of: state)
        let changes = TimelineChanges(from: timeline.items, to: next.items)
        if !changes.isEmpty {
            apply(next, changes)
        }
        ownChanges += 1
        defer { ownChanges -= 1 }
        let visible = tableView.rows(in: tableView.visibleRect)
        if visible.length > 0 {
            tableView.reloadData(
                forRowIndexes: IndexSet(integersIn: visible.location..<NSMaxRange(visible)),
                columnIndexes: [0])
        }
        tableView.noteHeightOfRows(
            withIndexesChanged: IndexSet(integersIn: 0..<tableView.numberOfRows))
        if pinsToBottom {
            scrollToBottom()
        } else {
            restoreAnchor()
        }
    }

    private func timeline(of state: MessageTableState) -> MessageTimeline {
        var edges: Set<MessageTimeline.Edge> = [state.reachedOldest ? .beginning : .older]
        if !state.atPresent {
            edges.insert(.newer)
        }
        return MessageTimeline(rows: state.rows, calendar: calendar, edges: edges)
    }

    // A scroll before the newest page lands cancels following the bottom.
    func jumpToPresent(load: Bool) {
        sticksToBottom = true
        anchor = nil
        if atPresent {
            ownChanges += 1
            defer { ownChanges -= 1 }
            scrollToBottom()
        } else if load {
            actions?.jumpToLatest()
        }
    }

    private var isPastTheTop: Bool {
        clipView.bounds.minY < clipView.originRange.lowerBound - 0.5
    }

    private func apply(_ next: MessageTimeline, _ changes: TimelineChanges) {
        let firstFill = !filled && !state.rows.isEmpty
        let pin = firstFill || pinsToBottom
        // Taken right before the update, live scroll or not: compensating for it moves nothing
        // the user sees, so it never fights a gesture.
        let place = pin ? nil : visibleAnchor(skipping: changes.removed)
        if let place {
            anchor = place
        }
        let before = place.flatMap { place in
            timeline.items.firstIndex { $0.id == place.id }.map { tableView.rect(ofRow: $0).minY }
        }
        let y = clipView.bounds.minY
        let fills = placeholderPlaces()
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
        if pin {
            // Pinning moves the content, so it waits until the user lets go.
            if !isLiveScrolling {
                scrollToBottom()
            }
        } else if let place, let before {
            restoreAnchor()
            logCompensation(place, before: before, y: y, changes: changes)
        }
        logFills(fills, changes: changes)
        updateHover()
    }

    // Before an update: each placeholder block with the message next to it, where that message
    // sits on screen, and whether the block shows.
    private func placeholderPlaces() -> [(
        edge: MessageTimeline.Edge, message: MessageTimeline.ItemId, y: CGFloat, visible: Bool
    )] {
        guard ScrollLog.enabled, let rows = messageRows else {
            return []
        }
        var places: [(MessageTimeline.Edge, MessageTimeline.ItemId, CGFloat, Bool)] = []
        for (index, item) in timeline.items.enumerated() {
            guard case .edge(let edge) = item, edge != .beginning else {
                continue
            }
            let row = edge == .older ? rows.first : rows.last
            places.append(
                (
                    edge, timeline.items[row].id,
                    tableView.rect(ofRow: row).minY - clipView.bounds.minY,
                    tableView.rect(ofRow: index).intersects(clipView.bounds)
                ))
        }
        return places
    }

    private func logFills(
        _ places: [(
            edge: MessageTimeline.Edge, message: MessageTimeline.ItemId, y: CGFloat, visible: Bool
        )], changes: TimelineChanges
    ) {
        for place in places {
            guard let index = timeline.items.firstIndex(where: { $0.id == place.message }) else {
                continue
            }
            let inserted =
                place.edge == .older
                ? changes.inserted.count(in: 1..<index)
                : changes.inserted.count(in: (index + 1)..<timeline.items.count)
            guard inserted > 0 else {
                continue
            }
            ScrollLog.log(
                "table \(number) fills the \(place.edge == .older ? "older" : "newer") "
                    + "placeholders: inserted \(inserted), visible \(place.visible), next message "
                    + "y \(place.y) to \(tableView.rect(ofRow: index).minY - clipView.bounds.minY)")
        }
    }

    // The first and last message rows, between the edges.
    private var messageRows: (first: Int, last: Int)? {
        let isMessage = { (item: MessageTimeline.Item) -> Bool in
            if case .message = item { true } else { false }
        }
        guard let first = timeline.items.firstIndex(where: isMessage),
            let last = timeline.items.lastIndex(where: isMessage)
        else {
            return nil
        }
        return (first, last)
    }

    private func logCompensation(
        _ place: (id: MessageTimeline.ItemId, offset: CGFloat), before: CGFloat, y: CGFloat,
        changes: TimelineChanges
    ) {
        guard ScrollLog.enabled,
            let index = timeline.items.firstIndex(where: { $0.id == place.id })
        else {
            return
        }
        let delta = tableView.rect(ofRow: index).minY - before
        let removed = changes.removed.count(in: 0..<max(0, index))
        ScrollLog.log(
            "table \(number) compensates: removed \(removed) inserted "
                + "\(changes.inserted.count(in: 0..<index)) reloaded "
                + "\(changes.reloaded.count(in: 0..<index)) above, delta \(delta), y \(y) to "
                + "\(clipView.bounds.minY), live \(isLiveScrolling)")
    }

    private func askForMore() {
        guard filled, !requested, state.loading == nil, !state.rows.isEmpty else {
            return
        }
        guard let rows = messageRows else {
            return
        }
        let bounds = clipView.bounds
        // Well before the reader reaches the placeholders, as in the official client.
        let distance = 3 * max(bounds.height, 200)
        if !state.reachedOldest, state.failedLoads[.older] == nil,
            bounds.minY - tableView.rect(ofRow: rows.first).minY < distance
        {
            requested = true
            ScrollLog.log("table \(number) asks for older messages at y \(bounds.minY)")
            actions?.loadMore(.older)
        } else if !state.atPresent, state.failedLoads[.newer] == nil,
            tableView.rect(ofRow: rows.last).maxY - bounds.maxY < distance
        {
            requested = true
            ScrollLog.log("table \(number) asks for newer messages at y \(bounds.minY)")
            actions?.loadMore(.newer)
        }
    }

    private func configureEdges() {
        for (index, item) in timeline.items.enumerated() {
            guard case .edge(let edge) = item,
                let cell = tableView.view(atColumn: 0, row: index, makeIfNecessary: false)
            else {
                continue
            }
            configure(cell, edge)
        }
    }

    private func configure(_ cell: NSView, _ edge: MessageTimeline.Edge) {
        if let cell = cell as? EdgeCell {
            cell.show(state.beginning)
        } else if let cell = cell as? PlaceholderCell {
            let failure: String? =
                if state.rows.isEmpty {
                    state.failedLoads[.latest] == nil ? nil : "Couldn't load messages."
                } else if edge == .older {
                    state.failedLoads[.older] == nil ? nil : "Couldn't load older messages."
                } else {
                    state.failedLoads[.newer] == nil ? nil : "Couldn't load newer messages."
                }
            cell.show(edge, height: placeholderHeight, failure: failure)
        }
    }

    // Without rows the placeholders stand in for the latest page.
    private func retry(_ edge: MessageTimeline.Edge) {
        if state.rows.isEmpty {
            actions?.jumpToLatest()
        } else {
            actions?.loadMore(edge)
        }
    }

    // The placeholders cover one and a half views of whatever height the list has.
    private func updatePlaceholderHeight() {
        let height = PlaceholderCell.height(forViewHeight: clipView.bounds.height)
        guard height != placeholderHeight else {
            return
        }
        placeholderHeight = height
        var rows = IndexSet()
        for (index, item) in timeline.items.enumerated() {
            if case .edge(let edge) = item, edge != .beginning {
                rowHeights[item.id] = nil
                rows.insert(index)
            }
        }
        if !rows.isEmpty {
            tableView.noteHeightOfRows(withIndexesChanged: rows)
            configureEdges()
        }
    }

    // Away from the present, rows below the reader are an older page's continuation, not news.
    private var pinsToBottom: Bool {
        sticksToBottom && atPresent
    }

    func pointerMoved() {
        updateHover()
    }

    // Fingers resting past an edge post nothing, yet the gesture still holds the list.
    private var isLiveScrolling: Bool {
        !liveScrollEnded
            && (Date().timeIntervalSince(lastLiveScroll) < Self.liveScrollQuiet || isPastAnEdge)
    }

    private var isPastAnEdge: Bool {
        let range = clipView.originRange
        let y = clipView.bounds.minY
        return y < range.lowerBound - 0.5 || y > range.upperBound + 0.5
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
        case .edge(.beginning):
            return EdgeCell.height
        case .edge:
            return placeholderHeight
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
        case .edge(.beginning):
            let cell = reuse(EdgeCell.identifier) as? EdgeCell ?? EdgeCell()
            configure(cell, .beginning)
            return cell
        case .edge(let edge):
            let cell = reuse(PlaceholderCell.identifier) as? PlaceholderCell ?? PlaceholderCell()
            cell.onRetry = { [weak self] in self?.retry(edge) }
            configure(cell, edge)
            return cell
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
            cell.onRetry = { [weak self] in self?.actions?.retry(message.id) }
            cell.onDelete = { [weak self] in self?.actions?.delete(message.id) }
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
        scrollView.contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: Self.bottomInset, right: 0)
        tableView.didLayout = { [weak self] in
            self?.settle()
        }
        // The composer below grows and shrinks the list without a layout of the table.
        scrollView.postsFrameChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(listResized), name: NSView.frameDidChangeNotification,
            object: scrollView)
        tableView.onEscape = { [weak self] in
            self?.actions?.escape()
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
        // Not necessarily posted on the main thread.
        for name in [
            Notification.Name.NSCalendarDayChanged, .NSSystemTimeZoneDidChange,
            NSLocale.currentLocaleDidChangeNotification,
        ] {
            timeObservers.append(
                notifications.addObserver(forName: name, object: nil, queue: .main) {
                    [weak self] _ in
                    MainActor.assumeIsolated {
                        self?.refreshTimes()
                    }
                })
        }
        // Wheel, keyboard and accessibility scrolls post no live-scroll notifications; every
        // scroll moves the clip view.
        scrollView.contentView.postsBoundsChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(clipMoved), name: NSView.boundsDidChangeNotification,
            object: scrollView.contentView)
    }

    // After a layout, a live scroll's end, or its quiet timeout: pin or keep the reader's place.
    private func settle() {
        guard !isLiveScrolling else {
            return
        }
        // Also the first page of an empty list, which isn't filled until it lands.
        if let held = heldState {
            show(held)
        }
        guard filled else {
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

    // A list that grew below its rows looks like a bounce past the bottom, which settling leaves
    // to AppKit; here it's the frame, so a pinned list goes to the exact bottom.
    @objc private func listResized(_ notification: Notification) {
        updatePlaceholderHeight()
        if pinsToBottom, !isLiveScrolling {
            ownChanges += 1
            defer { ownChanges -= 1 }
            scrollToBottom()
        }
        settle()
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
            repeat {
                try? await Task.sleep(for: .seconds(Self.liveScrollQuiet + 0.05))
                guard let self, self.liveScrollGeneration == generation else {
                    return
                }
            } while self?.isLiveScrolling == true
            guard let self else {
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
        askForMore()
    }

    // Read before the update: `timeline` and the table still hold the old rows.
    private func visibleAnchor(skipping removed: IndexSet) -> (
        id: MessageTimeline.ItemId, offset: CGFloat
    )? {
        let top = scrollView.contentView.bounds.minY
        let visible = tableView.rows(in: scrollView.contentView.bounds)
        // Edges and day dividers keep their place when a page lands next to them, so only a
        // message holds the reader's, even when it's below the visible rows.
        for row in visible.location..<timeline.items.count where !removed.contains(row) {
            guard case .message = timeline.items[row] else {
                continue
            }
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
    var onEscape: (() -> Void)?
    var pointerMoved: (() -> Void)?
    private(set) var isLayingOut = false

    // The rows' extent; the frame itself stretches to fill a taller clip view.
    var contentHeight: CGFloat {
        numberOfRows > 0 ? rect(ofRow: numberOfRows - 1).maxY : 0
    }

    override func cancelOperation(_ sender: Any?) {
        onEscape?()
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
