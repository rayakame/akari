import AkariKit
import AppKit

struct MessageTableState: Equatable {
    var rows: [MessageListModel.Row]
    var atPresent = true
    var reachedOldest = false
    var loading: MessageListModel.Load?
    var failedLoads: [MessageListModel.Load: RequestError] = [:]
    var start: ChannelStart?
    var channelId: ChannelId?
    var lastLoad: LoadTrace?
    var openedAt: ContinuousClock.Instant?
    var openedFrom = MessageListModel.Origin.network
    var revealedSpoilers: Set<SpoilerKey> = []
    var newMessages: MessageId?
    var unreadJumps = 0
    var messageJump: MessageListModel.MessageJump?
    // Kept clear above the start's icon for what covers the list's top.
    var startRoom: CGFloat = 0
}

// Row updates only: a full reloadData() would lose the scroll position.
final class MessageTableController: NSObject, NSTableViewDataSource, NSTableViewDelegate {
    typealias Schedule = @MainActor (Date, @escaping @MainActor () -> Void) -> () -> Void
    typealias Build =
        @MainActor (Message, Set<UInt16>, Date, TimestampText) -> MessageText.Built

    let tableView: MessageTableView
    let scrollView = NSScrollView()
    // Strong: the bridge that implements it holds nothing that leads back here.
    var actions: MessageListActions?
    private let clipView = BottomClipView()
    private static var created = 0
    // Tells tables apart in the scroll log, so a replaced scroll view shows.
    private let number: Int
    private(set) var timeline: MessageTimeline {
        didSet {
            lineRow = timeline.items.firstIndex {
                if case .newMessages = $0 { true } else { false }
            }
        }
    }
    // Kept with the timeline: every scroll asks for it.
    private(set) var lineRow: Int?
    // Measured inside SwiftUI's update or the table's layout, so it reaches the model on the next
    // turn; the last measure of a turn wins.
    private var measuredLine: (onScreen: Bool, line: MessageId)?
    private(set) var lineReport: Task<Void, Never>?
    var reducesMotion: () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }
    // The row a jump landed on, and when: Discord flashes it.
    private var flash: (row: MessageTimeline.ItemId, since: Date)?
    private let calendarSource: () -> Calendar
    private let localeSource: () -> Locale
    private var calendar: Calendar
    private var locale: Locale
    private var times: TimestampText
    private let now: () -> Date
    private let notifications: NotificationCenter
    nonisolated(unsafe) private var timeObservers: [NSObjectProtocol] = []
    // Where the pointer is, in window coordinates; nil when hover shouldn't show.
    private let pointer: () -> NSPoint?
    private var filled = false
    private var sticksToBottom = true {
        didSet {
            if !sticksToBottom {
                heldPinBottom = nil
            }
            if sticksToBottom != oldValue {
                actions?.atBottomChanged(sticksToBottom)
            }
        }
    }
    // Where the rows ended when pinning began waiting for a gesture to end: measured from it,
    // rows arriving below don't unpin the list, only the user's own scroll to older ones does.
    private var heldPinBottom: CGFloat?
    private var atPresent = true
    private(set) var state = MessageTableState(rows: [])
    private var revealed: [MessageId: Set<UInt16>] = [:]
    private var relativeTimes: [MessageId: Date] = [:]
    private let schedule: Schedule
    private let onScreen: @MainActor (NSWindow) -> Bool
    private let build: Build
    // Shared by the height pass and the cell, so a row's text is built once per change (D71).
    private var texts: [MessageId: (message: Message, revealed: Set<UInt16>, built: MessageText.Built)] =
        [:]
    nonisolated(unsafe) private var relativeTimer: (date: Date, cancel: () -> Void)?
    private var lastRelativeFire = Date.distantPast
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
    // The status strip above the composer lies over the list's bottom; the official client
    // leaves just that below its newest message.
    private static let restingInset = ComposerStatus.height
    // Exact heights before a row shows: automatic heights start as estimates and change when a
    // fling first reaches a row, which moves the bottom under it.
    private var rowHeights: [MessageTimeline.ItemId: CGFloat] = [:]
    private var rowHeightsWidth: CGFloat = 0
    private var placeholderHeight = PlaceholderCell.height(forViewHeight: 0)
    private struct Applied {
        let started: ContinuousClock.Instant
        let render: Duration
        let inserted: Set<MessageId>
        let held: Duration?
        var shown: ContinuousClock.Instant?
    }
    // The last few updates, so a page's line finds the one that brought its rows.
    private var applied: [Applied] = []
    private var heldSince: ContinuousClock.Instant?
    private var loggedLoad = 0
    // Pages waiting for their rows to be shown; one whose rows never show drops out after a few.
    private var pendingLoads: [LoadTrace] = []
    private var openLogged = false
    // A channel opened from disk logs its line once the page that refreshed it is shown.
    private var diskOpen: (channel: ChannelId, at: ContinuousClock.Instant, after: Duration)?
    private var latestShown: ContinuousClock.Instant?
    private var freshShown = false
    var milestone: (String) -> Void = { LaunchLog.mark($0) }
    private var pendingOpen:
        (
            channel: ChannelId, from: MessageListModel.Origin, at: ContinuousClock.Instant,
            update: ContinuousClock.Instant?
        )?

    init(
        tableView: MessageTableView = MessageTableView(),
        calendar: @escaping () -> Calendar = { .autoupdatingCurrent },
        locale: @escaping () -> Locale = { .autoupdatingCurrent },
        now: @escaping () -> Date = Date.init, pointer: (() -> NSPoint?)? = nil,
        notifications: NotificationCenter = .default,
        schedule: @escaping Schedule = MessageTableController.timer,
        onScreen: @escaping @MainActor (NSWindow) -> Bool = MessageTableController.isOnScreen,
        build: @escaping Build = MessageText.build
    ) {
        self.schedule = schedule
        self.onScreen = onScreen
        self.build = build
        self.tableView = tableView
        calendarSource = calendar
        localeSource = locale
        self.calendar = calendar()
        self.locale = locale()
        times = TimestampText(
            locale: self.locale, timeZone: self.calendar.timeZone, calendar: self.calendar)
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
        // Before SwiftUI sizes the list its column has AppKit's default width, where formatted
        // rows cost several times as much to build and measure; settling after the size shows it.
        guard scrollView.frame.width > 0 else {
            heldState = state
            return
        }
        if isLiveScrolling && isPastTheTop {
            heldState = state
            heldSince = heldSince ?? .now
            return
        }
        heldState = nil
        let wasFilled = filled
        let held = heldSince.map { $0.duration(to: .now) }
        heldSince = nil
        let started = ContinuousClock.now
        let previous = self.state
        self.state = state
        requested = false
        // Before the early return: leaving the present ends pinning even without new rows.
        atPresent = state.atPresent
        // The jump bar covers the bottom while older messages are shown.
        scrollView.contentInsets.bottom =
            Self.restingInset + (atPresent ? 0 : JumpBar.height + JumpBar.margin)
        let revealedNow = revealed(previous.revealedSpoilers, state.revealedSpoilers)
        let next = timeline(of: state)
        let changes = TimelineChanges(from: timeline.items, to: next.items)
        if !changes.isEmpty {
            Signposts.interval("page lands") { apply(next, changes) }
            noteApply(started, inserting: changes.inserted, of: next, held: held)
        }
        reload(revealedNow)
        // A new table for a model that jumped before doesn't jump again.
        if wasFilled, state.unreadJumps != previous.unreadJumps {
            scrollToTop(lineRow, below: state.startRoom)
        }
        if wasFilled, let jump = state.messageJump, jump != previous.messageJump {
            // Unless the line has been seen, the unread bar shows once it's out of sight.
            let bar = state.newMessages == nil ? 0 : UnreadBar.margin + UnreadBar.height
            let target = row(atOrAfter: jump.messageId)
            scrollToTop(target, below: state.startRoom + bar)
            startFlash(at: target, of: jump.messageId)
        }
        reportLine()
        updateRelativeTimer()
        let startChanged = previous.start != state.start || previous.startRoom != state.startRoom
        if startChanged {
            remeasureStart()
        }
        if previous.loading != state.loading || previous.failedLoads != state.failedLoads
            || previous.reachedOldest != state.reachedOldest || startChanged
        {
            configureEdges()
        }
        askForMore()
        if !freshShown, let load = state.lastLoad, load.report != nil {
            freshShown = true
            milestone("first fresh page rendered")
        }
        noteLoad(state)
    }

    private func noteApply(
        _ started: ContinuousClock.Instant, inserting rows: IndexSet, of next: MessageTimeline,
        held: Duration?
    ) {
        guard LoadLog.enabled else {
            return
        }
        let inserted = rows.compactMap { index -> MessageId? in
            if case .message(let row, _) = next.items[index] { row.id } else { nil }
        }
        applied.append(
            Applied(
                started: started, render: started.duration(to: .now), inserted: Set(inserted),
                held: held))
        applied.removeFirst(max(0, applied.count - 8))
        // A nested transaction's completion runs once the outermost one is committed.
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak self] in
            self?.applyShown(started)
        }
        CATransaction.commit()
    }

    private func applyShown(_ started: ContinuousClock.Instant) {
        if let index = applied.lastIndex(where: { $0.started == started }) {
            applied[index].shown = .now
        }
        writeLoadLines()
    }

    private func noteLoad(_ state: MessageTableState) {
        guard LoadLog.enabled else {
            return
        }
        if !openLogged, filled, let channel = state.channelId, let openedAt = state.openedAt {
            openLogged = true
            pendingOpen = (channel, state.openedFrom, openedAt, applied.last?.started)
        }
        if let load = state.lastLoad, load.sequence > loggedLoad {
            loggedLoad = load.sequence
            if load.failure != nil {
                LoadLog.log(LoadLog.failure(load))
            } else if load.report != nil {
                pendingLoads.append(load)
                pendingLoads.removeFirst(max(0, pendingLoads.count - 8))
            }
        }
        writeLoadLines()
    }

    // A page's rows may have come in an earlier update than its trace, with the store's batch,
    // or in a later one, after a live scroll held them back.
    private func writeLoadLines() {
        if let open = pendingOpen,
            let shown = applied.last(where: { $0.started == open.update })?.shown
        {
            pendingOpen = nil
            let after = open.at.duration(to: shown)
            if open.from == .disk {
                diskOpen = (open.channel, open.at, after)
            } else {
                LoadLog.log(LoadLog.opened(open.channel, from: open.from, after: after))
            }
        }
        var waiting: [LoadTrace] = []
        for load in pendingLoads {
            guard let (line, shown) = pageLine(load) else {
                waiting.append(load)
                continue
            }
            LoadLog.log(line)
            if load.kind == .latest, latestShown == nil {
                latestShown = shown
            }
        }
        pendingLoads = waiting
        // An empty fresh page can be shown before the rows from disk are.
        if let open = diskOpen, let latestShown {
            diskOpen = nil
            LoadLog.log(
                LoadLog.opened(
                    open.channel, from: .disk, after: open.after,
                    fresh: open.at.duration(to: latestShown)))
        }
    }

    private func pageLine(_ load: LoadTrace) -> (String, ContinuousClock.Instant)? {
        if load.page.isEmpty {
            let now = ContinuousClock.now
            let line = LoadLog.page(
                load, render: nil, rows: 0, held: nil, shown: load.started.duration(to: now))
            return (line, now)
        }
        guard let update = applied.last(where: { !$0.inserted.isDisjoint(with: load.page) }),
            let shown = update.shown
        else {
            return nil
        }
        let line = LoadLog.page(
            load, render: update.render, rows: update.inserted.intersection(load.page).count,
            held: update.held, shown: load.started.duration(to: shown))
        return (line, shown)
    }

    deinit {
        for observer in timeObservers {
            notifications.removeObserver(observer)
        }
        relativeTimer?.cancel()
    }

    // Every height is measured again: system rows carry the time in their wrapped text.
    func refreshTimes() {
        MessageFormat.forgetFormats()
        calendar = calendarSource()
        locale = localeSource()
        times = TimestampText(locale: locale, timeZone: calendar.timeZone, calendar: calendar)
        texts.removeAll()
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
        // Not only the visible rows: AppKit prepares row views beyond them, and a slight scroll
        // shows those as they are.
        var held = IndexSet()
        tableView.enumerateAvailableRowViews { _, row in
            if row >= 0 {
                held.insert(row)
            }
        }
        if !held.isEmpty {
            tableView.reloadData(forRowIndexes: held, columnIndexes: [0])
        }
        tableView.noteHeightOfRows(
            withIndexesChanged: IndexSet(integersIn: 0..<tableView.numberOfRows))
        if pinsToBottom {
            scrollToBottom()
        } else {
            restoreAnchor()
        }
        updateRelativeTimer()
    }

    private func revealed(_ old: Set<SpoilerKey>, _ new: Set<SpoilerKey>) -> Set<MessageId> {
        guard old != new else {
            return []
        }
        revealed = Dictionary(grouping: new, by: \.messageId).mapValues { Set($0.map(\.index)) }
        return Set(old.symmetricDifference(new).map(\.messageId))
    }

    // A revealed spoiler keeps the height, but a relative time in the same row may not.
    private func reload(_ messages: Set<MessageId>) {
        guard !messages.isEmpty else {
            return
        }
        var rows = IndexSet()
        for (index, item) in timeline.items.enumerated() {
            if case .message(let row, _) = item, messages.contains(row.id) {
                rows.insert(index)
            }
        }
        guard !rows.isEmpty else {
            return
        }
        for index in rows {
            rowHeights[timeline.items[index].id] = nil
        }
        ownChanges += 1
        defer { ownChanges -= 1 }
        tableView.reloadData(forRowIndexes: rows, columnIndexes: [0])
        tableView.noteHeightOfRows(withIndexesChanged: rows)
    }

    private func follow(_ url: URL, in row: MessageListModel.Row) {
        guard url.scheme == "akari" else {
            actions?.openLink(url, masked: false)
            return
        }
        guard let value = url.pathComponents.last.flatMap({ UInt64($0) }) else {
            return
        }
        switch url.host() {
        case "spoiler":
            actions?.reveal(SpoilerKey(messageId: row.id, index: UInt16(clamping: value)))
        case "channel":
            actions?.openChannel(ChannelId(rawValue: value))
        case "thread":
            actions?.openThread(ChannelId(rawValue: value), from: row.message.channelId)
        case "message":
            actions?.jump(to: MessageId(rawValue: value))
        case "guild":
            actions?.openGuild(GuildId(rawValue: value))
        case "link":
            let links = row.shown.formatted.links
            if value < links.count, let target = URL(string: links[Int(value)].url) {
                actions?.openLink(target, masked: true)
            }
        default:
            break
        }
    }

    private func timeline(of state: MessageTableState) -> MessageTimeline {
        var edges: Set<MessageTimeline.Edge> = [state.reachedOldest ? .beginning : .older]
        if !state.atPresent {
            edges.insert(.newer)
        }
        return MessageTimeline(
            rows: state.rows, calendar: calendar, edges: edges, newMessages: state.newMessages)
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

    // Keeps the reader's place as clipMoved does after a user's scroll; it skips our own.
    // `cover` is what the overlays over the list's top hide.
    private func scrollToTop(_ row: Int?, below cover: CGFloat) {
        guard let row else {
            return
        }
        let range = clipView.originRange
        let y = min(
            max(tableView.rect(ofRow: row).minY - cover, range.lowerBound), range.upperBound)
        ownChanges += 1
        defer { ownChanges -= 1 }
        clipView.scroll(to: NSPoint(x: clipView.bounds.minX, y: y))
        scrollView.reflectScrolledClipView(clipView)
        sticksToBottom = isPinnedToBottom
        anchor = sticksToBottom ? nil : visibleAnchor(skipping: [])
        askForMore()
    }

    // Discord flashes the message itself only, not the one after a deleted target.
    private func startFlash(at row: Int?, of message: MessageId) {
        guard let row, case .message(let shown, _) = timeline.items[row],
            shown.message.id == message
        else {
            flash = nil
            return
        }
        flash = (timeline.items[row].id, now())
        (tableView.view(atColumn: 0, row: row, makeIfNecessary: true) as? MessageCell)?
            .flash(elapsed: 0, reducesMotion: reducesMotion())
    }

    private func flashElapsed(_ id: MessageTimeline.ItemId) -> TimeInterval? {
        guard let flash, flash.row == id else {
            return nil
        }
        return now().timeIntervalSince(flash.since)
    }

    // A message deleted since leaves the one after it.
    private func row(atOrAfter message: MessageId) -> Int? {
        timeline.items.firstIndex { item in
            if case .message(let row, _) = item { row.message.id >= message } else { false }
        }
    }

    // Where it's drawn, mid-row, and not under the strip over the list's bottom. Only where rows
    // have their final places: a line passing by mid-update would count as seen for good.
    private func reportLine() {
        guard let row = lineRow, case .newMessages(let line) = timeline.items[row],
            timeline.items.count == tableView.numberOfRows, let window = tableView.window,
            onScreen(window)
        else {
            return
        }
        let y = tableView.rect(ofRow: row).midY
        let bounds = clipView.bounds
        let shown = y >= bounds.minY && y <= bounds.maxY - scrollView.contentInsets.bottom
        measuredLine = (shown, line)
        guard lineReport == nil else {
            return
        }
        lineReport = Task { [weak self] in
            guard let self else {
                return
            }
            lineReport = nil
            if let measured = measuredLine {
                measuredLine = nil
                actions?.newMessagesLineOnScreen(measured.onScreen, line: measured.line)
            }
        }
    }

    private var isPastTheTop: Bool {
        clipView.bounds.minY < clipView.originRange.lowerBound - 0.5
    }

    private func apply(_ next: MessageTimeline, _ changes: TimelineChanges) {
        let firstFill = !filled && !state.rows.isEmpty
        let pin = firstFill || pinsToBottom
        let bottomBefore = tableView.contentHeight
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
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        defer { CATransaction.commit() }
        for index in changes.removed {
            rowHeights[timeline.items[index].id] = nil
            if case .message(let row, _) = timeline.items[index] {
                relativeTimes[row.id] = nil
                texts[row.id] = nil
            }
        }
        for index in changes.reloaded {
            rowHeights[next.items[index].id] = nil
        }
        timeline = next
        Signposts.interval("row heights") {
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
                // Outside an update the rows below keep their views' old places until a later
                // layout; as in remeasureStart, inside one at zero duration they move at once.
                NSAnimationContext.beginGrouping()
                NSAnimationContext.current.duration = 0
                tableView.beginUpdates()
                tableView.noteHeightOfRows(withIndexesChanged: changes.reloaded)
                tableView.endUpdates()
                NSAnimationContext.endGrouping()
            }
        }
        if firstFill {
            filled = true
            sticksToBottom = true
            milestone("first channel rendered")
        }
        measureRows()
        if pin {
            // Pinning moves the content, so it waits until the user lets go.
            if !isLiveScrolling {
                scrollToBottom()
            } else if heldPinBottom == nil {
                heldPinBottom = bottomBefore
            }
        } else if let place, let before {
            restoreAnchor()
            logCompensation(place, before: before, y: y, changes: changes)
        }
        // Row views reach their rows only in the table's layout, which inside a SwiftUI update
        // can come a frame later: that frame showed them out of place.
        tableView.layoutSubtreeIfNeeded()
        logFills(fills, changes: changes)
        updateHover()
    }

    // The table measures new rows only when asked for spans from the top and answers estimates
    // meanwhile; pinning or keeping the reader's place on estimates moves the text.
    private func measureRows() {
        for row in 0..<tableView.numberOfRows {
            _ = tableView.rect(ofRow: row)
        }
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
            Signposts.event("page requested")
            ScrollLog.log("table \(number) asks for older messages at y \(bounds.minY)")
            actions?.loadMore(.older)
        } else if !state.atPresent, state.failedLoads[.newer] == nil,
            tableView.rect(ofRow: rows.last).maxY - bounds.maxY < distance
        {
            requested = true
            Signposts.event("page requested")
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
        if let cell = cell as? ChannelStartCell {
            cell.show(state.start, room: state.startRoom)
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

    // The channel's name arrives after READY or changes with a rename, and the header's height
    // with it; the reader's place stays.
    private func remeasureStart() {
        guard
            let index = timeline.items.firstIndex(where: {
                if case .edge(.beginning) = $0 { true } else { false }
            })
        else {
            return
        }
        let place = pinsToBottom ? nil : visibleAnchor(skipping: [])
        if let place {
            anchor = place
        }
        rowHeights[timeline.items[index].id] = nil
        ownChanges += 1
        defer { ownChanges -= 1 }
        // Inside an update the table moves its row views, as for apply's inserts; at zero
        // duration it does so at once.
        NSAnimationContext.beginGrouping()
        NSAnimationContext.current.duration = 0
        tableView.beginUpdates()
        tableView.noteHeightOfRows(withIndexesChanged: [index])
        tableView.endUpdates()
        NSAnimationContext.endGrouping()
        if pinsToBottom {
            scrollToBottom()
        } else {
            restoreAnchor()
        }
        // As in apply: inside a SwiftUI update the table's own layout can come a frame later.
        tableView.layoutSubtreeIfNeeded()
        updateHover()
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
        isPinned(to: tableView.contentHeight)
    }

    private func isPinned(to bottom: CGFloat) -> Bool {
        scrollView.contentView.bounds.maxY >= bottom - 2
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
        let height = height(
            of: item, followsDay: followsDay(row), followsLine: followsLine(row), width: width)
        rowHeights[item.id] = height
        return height
    }

    // A day's first message sits closer to its divider; TimelineChanges reloads it when that
    // changes.
    private func followsDay(_ row: Int) -> Bool {
        guard row > 0, case .day = timeline.items[row - 1] else {
            return false
        }
        return true
    }

    private func followsLine(_ row: Int) -> Bool {
        guard row > 0, case .newMessages = timeline.items[row - 1] else {
            return false
        }
        return true
    }

    private func height(
        of item: MessageTimeline.Item, followsDay: Bool, followsLine: Bool, width: CGFloat
    ) -> CGFloat {
        switch item {
        case .edge(.beginning):
            return ChannelStartCell.height(state.start, width: width, room: state.startRoom)
        case .edge:
            return placeholderHeight
        case .day:
            return DayDividerCell.height
        case .newMessages:
            return NewMessagesDivider.height
        case .message(let row, let startsGroup):
            let message = row.shown
            if let notice = row.noticeParts {
                let time = MessageFormat.groupTime(
                    message.timestamp, now: now(), calendar: calendar, locale: locale)
                return NoticeCell.height(
                    message, notice: notice, time: time, followsDay: followsDay,
                    followsLine: followsLine, width: width)
            }
            return MessageCell.height(
                message, body: body(row), startsGroup: startsGroup, followsDay: followsDay,
                followsLine: followsLine, width: width)
        }
    }

    func tableView(_ tableView: NSTableView, viewFor tableColumn: NSTableColumn?, row: Int)
        -> NSView?
    {
        let cell = cell(forRow: row)
        cell.objectValue = timeline.items[row].id
        return cell
    }

    func cell(forRow index: Int) -> NSTableCellView {
        let item = timeline.items[index]
        let followsDay = followsDay(index)
        let followsLine = followsLine(index)
        switch item {
        case .edge(.beginning):
            let cell = reuse(ChannelStartCell.identifier) as? ChannelStartCell ?? ChannelStartCell()
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
        case .newMessages:
            return reuse(NewMessagesDivider.identifier) as? NewMessagesDivider
                ?? NewMessagesDivider()
        case .message(let row, let startsGroup):
            let message = row.shown
            let groupTime = MessageFormat.groupTime(
                message.timestamp, now: now(), calendar: calendar, locale: locale)
            let fullDate = MessageFormat.fullDate(
                message.timestamp, calendar: calendar, locale: locale)
            if let notice = row.noticeParts {
                let cell = reuse(NoticeCell.identifier) as? NoticeCell ?? NoticeCell()
                cell.onLink = { [weak self] url in self?.follow(url, in: row) }
                cell.configure(
                    message, notice: notice, time: groupTime, followsDay: followsDay,
                    followsLine: followsLine, fullDate: fullDate)
                return cell
            }
            let cell =
                reuse(MessageCell.identifier) as? MessageCell
                ?? Signposts.interval("message cell made") { MessageCell() }
            cell.isHovered = item.id == hovered
            cell.onRetry = { [weak self] in self?.actions?.retry(row.message.id) }
            cell.onDelete = { [weak self] in self?.actions?.delete(row.message.id) }
            cell.onLink = { [weak self] url in self?.follow(url, in: row) }
            cell.configure(
                message, body: body(row), startsGroup: startsGroup, followsDay: followsDay,
                followsLine: followsLine, groupTime: groupTime,
                shortTime: MessageFormat.shortTime(
                    message.timestamp, calendar: calendar, locale: locale),
                fullDate: fullDate)
            cell.flash(elapsed: flashElapsed(item.id), reducesMotion: reducesMotion())
            return cell
        }
    }

    private func body(_ row: MessageListModel.Row) -> NSAttributedString {
        let revealed = revealed[row.id] ?? []
        let now = now()
        // A relative time that changed waits for the timer, which re-measures its row too.
        if let cached = texts[row.id], cached.revealed == revealed, cached.message == row.shown {
            return cached.built.text
        }
        let built = build(row.shown, revealed, now, times)
        texts[row.id] = (row.shown, revealed, built)
        relativeTimes[row.id] = built.nextChange
        return built.text
    }

    static func isOnScreen(_ window: NSWindow) -> Bool {
        window.occlusionState.contains(.visible) && !window.isMiniaturized
    }

    static func timer(_ date: Date, _ fire: @escaping @MainActor () -> Void) -> () -> Void {
        let timer = Timer(fire: date, interval: 0, repeats: false) { _ in
            MainActor.assumeIsolated { fire() }
        }
        RunLoop.main.add(timer, forMode: .common)
        return { timer.invalidate() }
    }

    // One timer for the list, at the earliest change on screen, at least a second apart (D68),
    // and none while the window is hidden.
    private func updateRelativeTimer() {
        let shown = tableView.window.map(onScreen) ?? false
        let visible = shown ? tableView.rows(in: tableView.visibleRect) : NSRange()
        var earliest: Date?
        for index in visible.location..<min(NSMaxRange(visible), timeline.items.count) {
            if case .message(let row, _) = timeline.items[index], let next = relativeTimes[row.id] {
                earliest = min(earliest ?? next, next)
            }
        }
        guard let earliest else {
            relativeTimer?.cancel()
            relativeTimer = nil
            return
        }
        let date = max(earliest, lastRelativeFire + 1)
        guard relativeTimer?.date != date else {
            return
        }
        relativeTimer?.cancel()
        relativeTimer = (
            date,
            schedule(date) { [weak self] in
                self?.relativeTimeChanged()
            }
        )
    }

    // A message's box keeps a gutter unless a legacy scroller shows.
    @objc private func scrollerStyleChanged() {
        tableView.enumerateAvailableRowViews { rowView, _ in
            (rowView.view(atColumn: 0) as? MessageCell)?.needsDisplay = true
        }
        // The scroll view takes the new style on the same notification; hover waits for it.
        Task { @MainActor [weak self] in
            self?.updateHover()
        }
    }

    @objc private func windowShownOrHidden(_ notification: Notification) {
        if notification.object as? NSWindow === tableView.window {
            updateRelativeTimer()
            reportLine()
        }
    }

    // Rows whose relative text changed, shown or not: an unshown one's height is kept too.
    private func relativeTimeChanged() {
        relativeTimer = nil
        let now = now()
        lastRelativeFire = now
        var due = IndexSet()
        for (index, item) in timeline.items.enumerated() {
            if case .message(let row, _) = item, let next = relativeTimes[row.id], next <= now {
                due.insert(index)
                rowHeights[item.id] = nil
                texts[row.id] = nil
            }
        }
        if !due.isEmpty {
            let place = pinsToBottom ? nil : visibleAnchor(skipping: [])
            if let place {
                anchor = place
            }
            ownChanges += 1
            defer { ownChanges -= 1 }
            let visible = tableView.rows(in: tableView.visibleRect)
            let shown = due.filteredIndexSet { NSLocationInRange($0, visible) }
            if !shown.isEmpty {
                tableView.reloadData(forRowIndexes: shown, columnIndexes: [0])
            }
            tableView.noteHeightOfRows(withIndexesChanged: due)
            measureRows()
            if pinsToBottom {
                scrollToBottom()
            } else {
                restoreAnchor()
            }
        }
        updateRelativeTimer()
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
        scrollView.contentInsets = NSEdgeInsets(top: 0, left: 0, bottom: Self.restingInset, right: 0)
        tableView.didLayout = { [weak self] in
            self?.settle()
            self?.reportLine()
        }
        // The composer below grows and shrinks the list without a layout of the table.
        scrollView.postsFrameChangedNotifications = true
        NotificationCenter.default.addObserver(
            self, selector: #selector(listResized), name: NSView.frameDidChangeNotification,
            object: scrollView)
        tableView.onEscape = { [weak self] in
            self?.actions?.escape()
        }
        tableView.onShiftEscape = { [weak self] in
            self?.actions?.markServerRead()
        }
        tableView.onShiftPageUp = { [weak self] in
            self?.actions?.jumpToUnread()
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
        for name in [
            NSWindow.didChangeOcclusionStateNotification, NSWindow.didMiniaturizeNotification,
            NSWindow.didDeminiaturizeNotification,
        ] {
            notifications.addObserver(
                self, selector: #selector(windowShownOrHidden(_:)), name: name, object: nil)
        }
        notifications.addObserver(
            self, selector: #selector(scrollerStyleChanged),
            name: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil)
        tableView.windowChanged = { [weak self] in
            self?.updateRelativeTimer()
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
        heldPinBottom = nil
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
        // The table keeps no row in place when heights change; settle() restores this one. Not in
        // our own update: a scroller appearing in it narrows the column before the rows are placed.
        if ownChanges == 0, !pinsToBottom, let place = visibleAnchor(skipping: []) {
            anchor = place
        }
        tableView.noteHeightOfRows(
            withIndexesChanged: IndexSet(integersIn: 0..<tableView.numberOfRows))
    }

    @objc private func liveScrollStarted(_ notification: Notification) {
        ScrollLog.log("table \(number) live scroll starts")
        Signposts.event("live scroll starts")
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
            Signposts.event("live scroll settles")
            self.settle()
        }
    }

    @objc private func liveScrollEnded(_ notification: Notification) {
        ScrollLog.log("table \(number) live scroll ends")
        Signposts.event("live scroll settles")
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
            // The space before a group and the scrollbar's gutter belong to no message.
            if row >= 0,
                let cell = tableView.view(atColumn: 0, row: row, makeIfNecessary: false)
                    as? MessageCell,
                !cell.box.contains(cell.convert(point, from: tableView))
            {
                row = -1
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
        updateRelativeTimer()
        // The table re-anchors inside its own layout; that isn't the user scrolling either.
        guard ownChanges == 0, !tableView.isLayingOut, origin != lastClipOrigin else {
            return
        }
        reportLine()
        if let held = heldPinBottom {
            // Follows the user down into rows that came, and up to where rows now end.
            let bottom = min(
                tableView.contentHeight,
                max(held, clipView.bounds.maxY - scrollView.contentInsets.bottom))
            heldPinBottom = bottom
            sticksToBottom = isPinned(to: bottom)
        } else {
            sticksToBottom = isPinnedToBottom
        }
        actions?.userScrolled(by: lastClipOrigin.y - origin.y, height: clipView.bounds.height)
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
            return (timeline.items[row].id, anchorTop(ofRow: row) - top)
        }
        return nil
    }

    // A message under the line has given its space above to the line's row; anchored where its
    // row would start without the line, its text stays put when the line comes or goes.
    private func anchorTop(ofRow row: Int) -> CGFloat {
        let top = tableView.rect(ofRow: row).minY
        guard followsLine(row), case .message(let item, let startsGroup) = timeline.items[row]
        else {
            return top
        }
        let afterDay = followsDay(row - 1)
        if item.notice != nil {
            return top + NoticeCell.textTop(followsDay: false, followsLine: true)
                - NoticeCell.textTop(followsDay: afterDay, followsLine: false)
        }
        let spacing = MessageCell.spacing(
            startsGroup: startsGroup, followsDay: afterDay, followsLine: false)
        return top - spacing
    }

    private func restoreAnchor() {
        guard let anchor, let row = timeline.items.firstIndex(where: { $0.id == anchor.id }) else {
            return
        }
        let range = clipView.originRange
        let target = anchorTop(ofRow: row) - anchor.offset
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
        heldPinBottom = nil
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
    var onShiftEscape: (() -> Void)?
    var onShiftPageUp: (() -> Void)?
    var currentEvent: () -> NSEvent? = { NSApp.currentEvent }
    var pointerMoved: (() -> Void)?
    var windowChanged: (() -> Void)?
    private(set) var isLayingOut = false

    // The rows' extent; the frame itself stretches to fill a taller clip view.
    var contentHeight: CGFloat {
        numberOfRows > 0 ? rect(ofRow: numberOfRows - 1).maxY : 0
    }

    // Shift+Escape and Cmd+. arrive as the same command.
    override func cancelOperation(_ sender: Any?) {
        guard let event = currentEvent(), event.isEscapeKey else {
            super.cancelOperation(sender)
            return
        }
        if event.modifierFlags.contains(.shift) {
            onShiftEscape?()
        } else {
            onEscape?()
        }
    }

    // Page Up doesn't reach AppKit's key bindings in a table, so Shift+Page Up (Shift+Fn+Up on a
    // laptop) is caught here.
    override func keyDown(with event: NSEvent) {
        let flags = event.modifierFlags.intersection([.shift, .control, .option, .command])
        if event.specialKey == .pageUp, flags == .shift {
            onShiftPageUp?()
        } else {
            super.keyDown(with: event)
        }
    }

    // Shift+Page Up's command, from a message's text.
    override func pageUpAndModifySelection(_ sender: Any?) {
        onShiftPageUp?()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        windowChanged?()
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

extension NSEvent {
    static let escapeKeyCode: UInt16 = 53

    var isEscapeKey: Bool {
        type == .keyDown && keyCode == Self.escapeKeyCode
    }
}
