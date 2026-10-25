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
        var afterReload: (IndexSet) -> Void = { _ in }

        override func reloadData() {
            fullReloads += 1
            super.reloadData()
        }

        override func reloadData(forRowIndexes rows: IndexSet, columnIndexes columns: IndexSet) {
            reloaded.append(rows)
            super.reloadData(forRowIndexes: rows, columnIndexes: columns)
            afterReload(rows)
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
    // Below the newest row when pinned: the status strip over the list's bottom.
    let restingInset = ComposerStatus.height
    let table = CountingTableView()
    let pointer = Pointer()
    let controller: MessageTableController
    let window: NSWindow

    init() {
        self.init(onScreen: MessageTableController.isOnScreen)
    }

    init(
        onScreen: @escaping @MainActor (NSWindow) -> Bool,
        now: @escaping () -> Date = { berlinTime(2026, 10, 10, 15, 0) }
    ) {
        controller = MessageTableController(
            tableView: table, calendar: { berlin }, now: now,
            pointer: { [pointer] in pointer.location }, onScreen: onScreen)
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
        let cell = controller.cell(forRow: row)
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

    // The cache's copy shows first; the page that refreshes it edits, deletes and adds.
    func refreshed(_ cached: [MessageListModel.Row]) -> [MessageListModel.Row] {
        let deleted = MessageId(rawValue: 35)
        let edited = MessageId(rawValue: 30)
        let edit: MessageListModel.Row = row(
            30, by: author(1), at: noon + 3600, content: "edited after the cache")
        var fresh: [MessageListModel.Row] = cached.filter { $0.id != deleted }
        if let index = fresh.firstIndex(where: { $0.id == edited }) {
            fresh[index] = edit
        }
        return fresh + messages(1, from: 81)
    }

    @Test
    func aFreshPageOverCachedRowsKeepsAScrolledReaderInPlace() {
        let cached = messages(80, longText: true)
        show(cached)
        scroll(toRowOf: 40)
        let offset = place(of: 40)
        let reader = table.rowView(atRow: index(of: 40), makeIfNecessary: false)

        show(refreshed(cached))

        #expect(abs(place(of: 40) - offset) <= 1)
        #expect(table.rowView(atRow: index(of: 40), makeIfNecessary: false) === reader)
    }

    @Test
    func cachedRowsOlderThanTheFreshPageLeaveAScrolledReaderInPlace() {
        let cached = messages(80, longText: true)
        show(cached)
        scroll(toRowOf: 40)
        let offset = place(of: 40)
        let reader = table.rowView(atRow: index(of: 40), makeIfNecessary: false)

        show(Array(refreshed(cached).dropFirst(3)) + messages(2, from: 82))

        #expect(abs(place(of: 40) - offset) <= 1)
        #expect(table.rowView(atRow: index(of: 40), makeIfNecessary: false) === reader)
    }

    @Test
    func aFreshPageOverCachedRowsKeepsTheBottomPinned() {
        let cached = messages(80, longText: true)
        show(cached)
        #expect(controller.isPinnedToBottom)

        show(refreshed(cached))

        #expect(controller.isPinnedToBottom)
        #expect(visibleRows.contains(index(of: 81)))
    }

    // A mouse or "Show scroll bars: Always": the scroller appears when the rows outgrow the
    // view, which narrows the column in the middle of the update.
    @Test
    func aFirstFillAwayFromThePresentShowsTheBottomWithLegacyScrollers() {
        controller.scrollView.scrollerStyle = .legacy

        show(MessageTableState(rows: messages(80, longText: true), atPresent: false))

        #expect(controller.isPinnedToBottom)
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

    // Every third message formatted, as in a channel with a few code blocks and lists.
    func formattedMix(_ count: Int) -> [MessageListModel.Row] {
        let contents = FormattedSamples.contents
        return (1...count).map { number in
            row(
                UInt64(number), by: author(UInt64(2 + number % 2)), at: noon + Double(number) * 600,
                content: number % 3 == 0
                    ? contents[number / 3 % contents.count] : "test \(number)")
        }
    }

    // As SwiftUI makes a list for a channel the user returns to: filled, placed, then sized.
    // The counts put different rows at the top edge.
    @Test
    func aListFilledBeforeItHasASizeLandsOnItsNewestMessage() throws {
        var missed: [Int: CGFloat] = [:]
        for count in 30...70 {
            let controller = MessageTableController(
                calendar: { berlin }, now: { berlinTime(2026, 10, 12, 15, 0) }, pointer: { nil })
            controller.show(MessageTableState(rows: formattedMix(count), reachedOldest: true))
            let window = NSWindow(
                contentRect: NSRect(x: 0, y: 0, width: 725, height: 431), styleMask: [.titled],
                backing: .buffered, defer: false)
            let container = try #require(window.contentView)
            container.addSubview(controller.scrollView)
            controller.scrollView.frame = container.bounds
            window.layoutIfNeeded()

            let table = controller.tableView
            let scroll = controller.scrollView
            let newest = table.rect(ofRow: table.numberOfRows - 1).maxY
            let shown = scroll.contentView.bounds.maxY - scroll.contentInsets.bottom
            if newest > shown + 0.5 {
                missed[count] = newest - shown
            }
        }

        #expect(missed.isEmpty, "\(missed.sorted { $0.key < $1.key })")
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
        #expect(abs(below() - restingInset) <= 1)
        #expect(table.rect(ofRow: 0).minY - clip.bounds.minY > 100)

        rows += messages(1, from: 4)
        show(MessageTableState(rows: rows, reachedOldest: true))
        #expect(abs(below() - restingInset) <= 1)

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

    // What a cell shows as text, labels and selectable text alike, in the order of its views.
    func shownTexts(in view: NSView) -> [String] {
        view.subviews.filter { !$0.isHidden }.flatMap { subview -> [String] in
            if let field = subview as? NSTextField {
                return [field.stringValue]
            }
            if let text = subview as? NSTextView {
                return [text.string]
            }
            return shownTexts(in: subview)
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

    // Where a wheel's step ends (NSClipView's constrained _scrollTo), positive up. AppKit applies
    // a view's first wheel event on the display's next frame, which a locked screen never draws.
    func wheel(by pixels: CGFloat) {
        table.scroll(NSPoint(x: 0, y: clip.bounds.minY - pixels))
        window.layoutIfNeeded()
    }

    @Test
    func theHoveredRowFollowsThePointerOnEveryScroll() {
        show(messages(80, longText: true))
        let clip = controller.scrollView.contentView
        pointer.location = NSPoint(x: 300, y: 200)
        let underPointer = { self.boxRow(at: NSPoint(x: 300, y: 200)) }

        controller.pointerMoved()
        #expect(hoveredRows == underPointer())

        // Scrolled under a still pointer, with no live-scroll notification.
        clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY - 300))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        #expect(hoveredRows == underPointer())

        pointer.location = nil
        controller.pointerMoved()
        #expect(hoveredRows.isEmpty)
    }

    // The message row whose box holds a point in window coordinates; the space before a group
    // and the scrollbar's gutter belong to none.
    func boxRow(at location: NSPoint) -> [Int] {
        let point = table.convert(location, from: nil)
        let row = table.row(at: point)
        guard let cell = table.view(atColumn: 0, row: row, makeIfNecessary: false) as? MessageCell
        else {
            return []
        }
        return cell.box.contains(cell.convert(point, from: table)) ? [row] : []
    }

    @Test
    func theSpaceBeforeAGroupHoversNothing() {
        controller.scrollView.scrollerStyle = .overlay
        show([row(1, at: noon), row(2, by: author(2), at: noon + 30)])
        let second = index(of: 2)
        let rect = table.rect(ofRow: second)
        let at = { (x: CGFloat, y: CGFloat) in self.table.convert(NSPoint(x: x, y: y), to: nil) }

        pointer.location = at(300, rect.minY + 8)
        controller.pointerMoved()
        #expect(hoveredRows.isEmpty)

        pointer.location = at(300, rect.minY + 30)
        controller.pointerMoved()
        #expect(hoveredRows == [second])

        pointer.location = at(table.bounds.maxX - 8, rect.minY + 30)
        controller.pointerMoved()
        #expect(hoveredRows.isEmpty)
    }

    // A legacy scroller that shows up takes the gutter itself; messages then reach it.
    @Test
    func aScrollerStyleChangeRedrawsAndHoversAgain() async throws {
        let center = NotificationCenter()
        let pointer = Pointer()
        let table = MessageTableView()
        let controller = MessageTableController(
            tableView: table, calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) },
            pointer: { pointer.location }, notifications: center)
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 400), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        controller.scrollView.scrollerStyle = .overlay
        // Highlighted, so a box that changes shows.
        let rows = messages(30).map { row in
            var message = row.message
            message.mentionsMe = true
            return MessageListModel.Row(id: row.id, message: message)
        }
        controller.show(MessageTableState(rows: rows))
        window.layoutIfNeeded()
        let visible = table.rows(in: table.visibleRect)
        let cells = (visible.location..<NSMaxRange(visible)).filter {
            table.view(atColumn: 0, row: $0, makeIfNecessary: false) is MessageCell
        }
        let row = try #require(cells.last)
        let cell = try #require(table.view(atColumn: 0, row: row, makeIfNecessary: false))
        let first = try #require(cells.first)
        let other = try #require(table.view(atColumn: 0, row: first, makeIfNecessary: false))
        let hovered = { (cell as? MessageCell)?.isHovered == true }
        let atTheEdge = {
            table.convert(
                NSPoint(x: table.bounds.maxX - 4, y: table.rect(ofRow: row).maxY - 4), to: nil)
        }
        pointer.location = atTheEdge()
        controller.pointerMoved()
        #expect(!hovered())

        controller.scrollView.scrollerStyle = .legacy
        window.layoutIfNeeded()
        try #require(controller.scrollView.verticalScroller?.isHidden == false)
        pointer.location = atTheEdge()
        // Its backing layer says whether it will draw again; NSView's flag reads false there.
        let redraws = { other.layer?.needsDisplay() == true }
        window.displayIfNeeded()
        CATransaction.flush()
        try #require(!redraws())
        #expect(!hovered())

        center.post(name: NSScroller.preferredScrollerStyleDidChangeNotification, object: nil)

        #expect(redraws())
        try await Task.sleep(for: .milliseconds(50))
        #expect(hovered())
    }

    func messageText(in view: NSView) -> MessageTextView? {
        view.subviews.lazy.compactMap { ($0 as? MessageTextView) ?? self.messageText(in: $0) }
            .first
    }

    @Test
    func anEditDuringASelectionKeepsTheRowStable() throws {
        let rows = variedMessages(60)
        show(rows)
        scroll(toRowOf: 20)
        let top = place(of: 20)
        let text = try #require(messageText(in: try cell(of: 21)))
        window.makeFirstResponder(text)
        text.setSelectedRange(NSRange(location: 0, length: 5))

        var edited = rows
        let old = rows[20].message
        edited[20] = row(
            21, by: old.author, at: old.timestamp,
            content: old.content + " and **more** words that wrap onto another line or two",
            edited: noon + 9_999)
        show(edited)

        #expect(place(of: 20) == top)
        let after = try #require(messageText(in: try cell(of: 21)))
        #expect(after.string.contains("more"))
        #expect(after.selectedRange().length == 0)
        #expect((window.firstResponder as? NSTextView)?.selectedRange().length ?? 0 == 0)
    }

    @Test
    func aReusedCellDropsTheSelectionOfItsLastMessage() throws {
        let cell = MessageCell()
        let show = { (message: Message) in
            cell.configure(
                message, body: NSAttributedString(string: message.content), startsGroup: true,
                groupTime: "", shortTime: "", fullDate: "")
        }
        let first = row(1, at: noon, content: "same words").message
        show(first)
        let text = try #require(messageText(in: cell))
        text.setSelectedRange(NSRange(location: 0, length: 4))

        show(first)
        #expect(text.selectedRange().length == 4)

        show(row(2, at: noon, content: "same words").message)
        #expect(text.selectedRange().length == 0)
    }

    @Test
    func clickingMessageTextKeepsItsAttributes() throws {
        show([row(1, at: noon, content: "hello there")])
        let text = try #require(messageText(in: try cell(of: 1)))

        window.makeFirstResponder(text)
        text.selectAll(nil)

        let attribute = { (key: NSAttributedString.Key) in
            text.textStorage?.attribute(key, at: 0, effectiveRange: nil)
        }
        #expect(text.string == "hello there")
        #expect((attribute(.font) as? NSFont)?.pointSize == 16)
        #expect((attribute(.paragraphStyle) as? NSParagraphStyle)?.minimumLineHeight == 22)
    }

    // A test's window never becomes key, and a window that isn't hands a first click to no text
    // view; this one says it is, so clicks reach views as the user's would.
    final class KeyWindow: NSWindow {
        override var isKeyWindow: Bool { true }
        override var canBecomeKey: Bool { true }
    }

    // The list, moved into a window that takes clicks as a key window does.
    func keyWindow() -> NSWindow {
        let key = KeyWindow(
            contentRect: window.contentRect(forFrameRect: window.frame), styleMask: window.styleMask,
            backing: .buffered, defer: false)
        key.isReleasedWhenClosed = false
        key.contentView = controller.scrollView
        key.orderFront(nil)
        key.layoutIfNeeded()
        key.displayIfNeeded()
        return key
    }

    // A click, double click or drag as the window gets it: the release waits in the queue for
    // the tracking loop a press starts.
    func press(_ view: NSView, clicks: Int = 1, drag: Bool = false) throws {
        let window = try #require(view.window)
        let start = view.convert(
            NSPoint(x: min(12, view.bounds.width / 2), y: view.bounds.midY), to: nil)
        let end = drag ? NSPoint(x: start.x + 40, y: start.y) : start
        let event = { (type: NSEvent.EventType, point: NSPoint) in
            NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: clicks, pressure: 1)
        }
        if drag, let dragged = event(.leftMouseDragged, end) {
            NSApp.postEvent(dragged, atStart: false)
        }
        if let up = event(.leftMouseUp, end) {
            NSApp.postEvent(up, atStart: false)
        }
        window.sendEvent(try #require(event(.leftMouseDown, start)))
        window.layoutIfNeeded()
    }

    // The window's pixels around `view`, as drawn now.
    func drawing(around view: NSView) throws -> [UInt8] {
        let content = try #require(view.window?.contentView)
        content.displayIfNeeded()
        let rect = view.convert(view.bounds, to: content).insetBy(dx: -2, dy: -2)
        let rep = try #require(content.bitmapImageRepForCachingDisplay(in: rect))
        content.cacheDisplay(in: rect, to: rep)
        let data = try #require(rep.bitmapData)
        return Array(UnsafeBufferPointer(start: data, count: rep.bytesPerRow * rep.pixelsHigh))
    }

    // Clicks, double clicks and drags change neither the drawing nor the font, and no field editor
    // takes over. Selectable text is a message's text view; chrome isn't selectable.
    func expectClicksKeepTheDrawing(of view: NSView, selectable: Bool) throws {
        let window = try #require(view.window)
        for (clicks, drag) in [(1, false), (2, false), (1, true)] {
            window.makeFirstResponder(nil)
            let before = try drawing(around: view)
            try press(view, clicks: clicks, drag: drag)
            (view as? NSTextView)?.setSelectedRange(NSRange(location: 0, length: 0))
            #expect(try drawing(around: view) == before, "\(clicks) clicks, drag \(drag)")
            #expect((window.firstResponder as? NSTextView)?.isFieldEditor != true)
            #expect((view as? NSTextField)?.currentEditor() == nil)
        }
        if selectable {
            #expect((view as? MessageTextView)?.isSelectable == true)
        } else {
            #expect((view as? NSTextField)?.isSelectable == false)
        }
    }

    @Test
    func clickingANoticeKeepsItsTextAsDrawn() throws {
        show(
            MessageTableState(
                rows: [row(1, at: noon), row(2, at: noon + 60, kind: .userJoin)],
                reachedOldest: true))
        let key = keyWindow()
        defer { key.close() }
        let notice = try #require(messageText(in: try cell(of: 2)))
        // Discord's welcome line 5, which message 2's time picks.
        #expect(notice.string.hasPrefix("Mira just slid into the server."))

        try expectClicksKeepTheDrawing(of: notice, selectable: true)

        let font = { (index: Int) in
            notice.textStorage?.attribute(.font, at: index, effectiveRange: nil) as? NSFont
        }
        #expect(font(0) == TypeScale.noticeName.nsFont, "the actor")
        #expect(font("Mira".count) == TypeScale.body.nsFont)
    }

    @Test
    func clickingTheStartAndAMessagesFilesKeepsThemAsDrawn() throws {
        let general = ChannelName(kind: .guildChannel(.guildText), name: "general")
        show(
            MessageTableState(
                rows: [row(1, at: noon, content: "hello", attachments: ["a.zip"])],
                reachedOldest: true, start: general.start))
        let key = keyWindow()
        defer { key.close() }
        let start = try #require(try cell(at: 0) as? ChannelStartCell)
        let texts = messageTexts(in: start).filter { !$0.isHiddenOrHasHiddenAncestor }
        #expect(texts.map(\.string) == ["Welcome to #general!", "This is the start of the #general channel."])
        let files = try #require(
            messageTexts(in: try cell(of: 1)).first { $0.string.contains("a.zip") })

        for view in texts + [files] {
            try expectClicksKeepTheDrawing(of: view, selectable: true)
        }
    }

    // What the text view's tooltip says at a point, as AppKit asks it while the pointer rests.
    func toolTip(of text: NSTextView, at point: NSPoint) -> String? {
        guard let layoutManager = text.layoutManager, let container = text.textContainer else {
            return nil
        }
        let inContainer = NSPoint(
            x: point.x - text.textContainerOrigin.x, y: point.y - text.textContainerOrigin.y)
        var fraction: CGFloat = 0
        let glyph = layoutManager.glyphIndex(
            for: inContainer, in: container, fractionOfDistanceThroughGlyph: &fraction)
        let rect = layoutManager.boundingRect(
            forGlyphRange: NSRange(location: glyph, length: 1), in: container)
        guard rect.contains(inContainer) else {
            return nil
        }
        let index = layoutManager.characterIndexForGlyph(at: glyph)
        return text.textStorage?.attribute(.toolTip, at: index, effectiveRange: nil) as? String
    }

    // The middle of the glyphs of `part` in the text view, in its own coordinates.
    func middle(of part: String, in text: NSTextView) throws -> NSPoint {
        let range = (text.string as NSString).range(of: part, options: .backwards)
        try #require(range.location != NSNotFound, "\(part) in \(text.string)")
        let layoutManager = try #require(text.layoutManager)
        let glyphs = layoutManager.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
        let rect = layoutManager.boundingRect(
            forGlyphRange: glyphs, in: try #require(text.textContainer))
        return NSPoint(
            x: rect.midX + text.textContainerOrigin.x, y: rect.midY + text.textContainerOrigin.y)
    }

    // A notice and a message's files wrap as their text views lay them out at every width.
    @Test
    func aWrappingNoticeAndFilesAreAsTallAsTheirCells() {
        let long = author(4, String(repeating: "Mira Kowalczyk-Lindqvist ", count: 3))
        show(
            MessageTableState(
                rows: [
                    row(1, by: long, at: noon, kind: .userJoin),
                    row(3, at: noon + 30, kind: .userJoin),
                    row(
                        2, at: noon + 60, content: "",
                        attachments: [String(repeating: "a-long-file-name-", count: 4) + ".zip"],
                        edited: noon + 70),
                ], reachedOldest: true))
        var wrong: [String] = []
        for width in stride(from: 150, through: 700, by: 3) {
            window.setContentSize(NSSize(width: CGFloat(width), height: 400))
            window.layoutIfNeeded()
            for raw in [UInt64(1), 3, 2] where table.rect(ofRow: index(of: raw)).height
                != fittingHeight(ofRow: index(of: raw))
            {
                wrong.append("\(raw) at \(width)")
            }
        }
        #expect(wrong.isEmpty, "\(wrong.prefix(6))")
    }

    // As the official client: the full date over the time only, near the list's top and bottom.
    @Test
    func aNoticesFullDateShowsOverItsTimeOnly() throws {
        let rows = [row(1, at: noon, kind: .userJoin)] + messages(30, from: 2, longText: true)
            + [row(40, at: noon + 4000, kind: .userJoin)]
        show(MessageTableState(rows: rows, reachedOldest: true))
        for raw in [UInt64(1), 40] {
            clip.scroll(
                to: NSPoint(x: 0, y: table.rect(ofRow: index(of: raw)).minY - (raw == 1 ? 0 : 300)))
            window.layoutIfNeeded()
            let notice = try #require(messageText(in: try cell(of: raw)))
            let time = try #require(notice.string.components(separatedBy: ". ").last)

            #expect(notice.toolTip == nil, "no tooltip over the whole line")
            let date = toolTip(of: notice, at: try middle(of: time, in: notice))
            #expect(date?.isEmpty == false, "\(raw)")
            #expect(toolTip(of: notice, at: try middle(of: "slid", in: notice)) == nil)
        }
    }

    // Discord lets none of these be selected.
    @Test
    func namesTimesAndDividersArentSelectable() throws {
        show(
            MessageTableState(
                rows: [
                    row(1, at: noon, kind: .reply), row(2, at: noon + 60),
                    row(3, at: noon + 120, delivery: .failed),
                ], reachedOldest: true))
        let key = keyWindow()
        defer { key.close() }
        let cells = try (0..<table.numberOfRows).map { try cell(at: $0) }
        (try cell(of: 2) as? MessageCell)?.isHovered = true
        let fields = cells.flatMap { textFields(in: $0) }.filter {
            !$0.isHiddenOrHasHiddenAncestor && !$0.stringValue.isEmpty
        }
        let shown = Set(fields.map(\.stringValue))
        #expect(shown.isSuperset(of: ["Mira", "Not sent."]))
        #expect(shown.contains { $0.hasPrefix("12:00") } && shown.contains { $0.hasPrefix("12:01") })
        #expect(shown.contains { $0.contains("Replying to") })
        #expect(shown.contains { $0.contains("October") })

        for field in fields {
            #expect(!field.isSelectable, "\(field.stringValue)")
            try expectClicksKeepTheDrawing(of: field, selectable: false)
        }
    }

    func messageTexts(in view: NSView) -> [MessageTextView] {
        view.subviews.flatMap { subview in
            (subview as? MessageTextView).map { [$0] } ?? messageTexts(in: subview)
        }
    }

    @Test
    func escapeStillReachesTheListWithTextSelected() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        let text = try #require(messageText(in: try cell(of: 200)))
        window.makeFirstResponder(text)
        text.setSelectedRange(NSRange(location: 0, length: 3))

        let escape = try #require(escapeKey())
        table.currentEvent = { escape }
        text.keyDown(with: escape)

        #expect(actions.calls.filter { !$0.hasPrefix("more") } == ["escape"])
    }

    // The lowest row of ink a view draws, in window points.
    func inkBottom(of view: NSView) throws -> CGFloat {
        let rep = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: rep)
        let last = try #require(
            (0..<rep.pixelsHigh).last { y in
                (0..<rep.pixelsWide).contains { x in
                    (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5
                }
            })
        let scale = CGFloat(rep.pixelsHigh) / view.bounds.height
        let fromTop = CGFloat(last + 1) / scale
        let y = view.isFlipped ? fromTop : view.bounds.height - fromTop
        return view.convert(NSPoint(x: 0, y: y), to: nil).y
    }

    @Test
    func theHoverTimeSitsOnTheFirstLinesBaseline() throws {
        show([row(1, at: noon), row(2, at: noon + 30, content: "xxxx xxxx")])
        let cell = try #require(try cell(of: 2) as? MessageCell)
        cell.isHovered = true
        cell.layoutSubtreeIfNeeded()
        let text = try #require(messageText(in: cell))
        let time = try #require(
            textFields(in: cell).first { !$0.isHiddenOrHasHiddenAncestor && $0.stringValue != "" })

        // Neither "x" nor the time's digits reach below the baseline.
        let textBottom = try inkBottom(of: text)
        let timeBottom = try inkBottom(of: time)
        #expect(abs(textBottom - timeBottom) <= 0.5, "time at \(timeBottom), text at \(textBottom)")
    }

    @Test
    func theListBouncesOnlyVerticallyAndAWheelLandsOnTheBottom() {
        var rows = messages(80, longText: true)
        show(rows)
        let scrollView = controller.scrollView
        #expect(scrollView.verticalScrollElasticity == .automatic)
        #expect(scrollView.horizontalScrollElasticity == .none)
        let bottom = clip.bounds.origin

        wheel(by: 80)
        #expect(clip.bounds.origin.y == bottom.y - 80)
        wheel(by: -300)
        #expect(clip.bounds.origin == bottom)

        rows += messages(1, from: 81)
        show(rows)
        window.layoutIfNeeded()
        #expect(visibleRows.contains(table.numberOfRows - 1))
        #expect(controller.isPinnedToBottom)
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
        // Bounded: a clip that stops moving short of the bottom fails below instead of spinning.
        for _ in 0..<500 {
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
            + FormattedSamples.contents.enumerated().map { index, content in
                named(
                    row(
                        UInt64(60 + index), by: author(UInt64(3 + index % 2)),
                        at: later + 200 + Double(index) * 400, content: content,
                        edited: index % 3 == 0 ? later + 210 + Double(index) * 400 : nil))
            }
        // Names known, so pills and channel symbols are measured; one spoiler revealed.
        show(
            MessageTableState(
                rows: rows,
                revealedSpoilers: [SpoilerKey(messageId: MessageId(rawValue: 66), index: 0)]))

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

        let shown = shownTexts(in: try cell(of: 1))
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
        var bottoms: [Bool] = []
        var scrolls: [CGFloat] = []
        var linesOnScreen: [Bool] = []

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

        func markServerRead() {
            calls.append("server read")
        }

        func jumpToUnread() {
            calls.append("unread")
        }

        func newMessagesLineOnScreen(_ onScreen: Bool, line: MessageId) {
            linesOnScreen.append(onScreen)
        }

        func atBottomChanged(_ atBottom: Bool) {
            bottoms.append(atBottom)
        }

        func userScrolled(by distance: CGFloat, height: CGFloat) {
            scrolls.append(distance)
        }

        func reveal(_ spoiler: SpoilerKey) {
            calls.append("reveal \(spoiler.messageId.rawValue) \(spoiler.index)")
        }

        func openChannel(_ id: ChannelId) {
            calls.append("channel \(id.rawValue)")
        }

        func openThread(_ id: ChannelId, from parent: ChannelId) {
            calls.append("thread \(id.rawValue) from \(parent.rawValue)")
        }

        func openThreads(of channel: ChannelId) {
            calls.append("threads of \(channel.rawValue)")
        }

        func jump(to message: MessageId) {
            calls.append("jump to \(message.rawValue)")
        }

        func openGuild(_ id: GuildId) {
            calls.append("guild \(id.rawValue)")
        }

        func openLink(_ url: URL, masked: Bool) {
            calls.append("link \(url.absoluteString)\(masked ? " masked" : "")")
        }

        var startsThreads: Set<UInt64> = []

        func canCreateThread(from message: MessageId) -> Bool {
            startsThreads.contains(message.rawValue)
        }

        func createThread(from message: MessageId) {
            calls.append("create thread from \(message.rawValue)")
        }

        var popouts: [ExpressionTarget] = []

        func expressionPopout(_ target: ExpressionTarget) -> ExpressionPopoutModel? {
            popouts.append(target)
            return ExpressionPopoutModel(
                target: target, channelGuild: nil, store: PopoutStore(), lookups: PopoutLookups())
        }
    }

    // A store and lookups for a popout: the emoji is from server 50, which the user is in.
    struct PopoutStore: ExpressionStore {
        func emojiGuild(emojiId: EmojiId) -> GuildId? { GuildId(rawValue: 50) }
        func guild(id: GuildId) -> Guild? { id.rawValue == 50 ? Guild(id: id, name: "Emoji Hub") : nil }
        func nitro() -> Nitro { .none }
    }

    final class PopoutLookups: ExpressionLookups {
        func emojiSource(emojiId: EmojiId) async throws -> EmojiSource { throw RequestError.UnexpectedResponse }
        func sticker(stickerId: StickerId) async throws -> StickerDetails { throw RequestError.UnexpectedResponse }
        func stickerGuild(stickerId: StickerId) async throws -> SourceGuild { throw RequestError.UnexpectedResponse }
        func stickerPack(packId: StickerPackId) async throws -> StickerPack { throw RequestError.UnexpectedResponse }
    }

    // At the middle of message `raw`'s box.
    func rightClick(on raw: UInt64) throws -> NSEvent {
        let cell = try #require(try cell(of: raw) as? MessageCell)
        let point = cell.convert(NSPoint(x: cell.box.midX, y: cell.box.midY), to: nil)
        return try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: point, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
    }

    // SwiftUI makes new actions on any update, also while the menu is open.
    @Test
    func createThreadReachesTheActionsOfTheMomentItsChosen() throws {
        let opened = RecordingActions()
        opened.startsThreads = [1]
        controller.actions = opened
        show([row(1, at: noon)])
        let menu = try #require(try cell(of: 1).menu(for: try rightClick(on: 1)))

        let current = RecordingActions()
        controller.actions = current
        menu.performActionForItem(at: 0)

        #expect(current.calls.contains("create thread from 1"))
        #expect(!opened.calls.contains("create thread from 1"))
    }

    // Asked when the menu opens: a thread started meanwhile takes the item away.
    @Test
    func aMessageThatCanStartAThreadOffersCreateThreadFirst() throws {
        let actions = RecordingActions()
        actions.startsThreads = [1]
        controller.actions = actions
        show([row(1, at: noon), row(2, at: noon + 10)])

        let menu = try #require(try cell(of: 1).menu(for: try rightClick(on: 1)))
        #expect(menu.items.map(\.title) == ["Create Thread", "", "Copy Text"])
        menu.performActionForItem(at: 0)
        #expect(actions.calls.contains("create thread from 1"))
        let other = try #require(try cell(of: 2).menu(for: try rightClick(on: 2)))
        #expect(other.items.map(\.title) == ["Copy Text"])
        actions.startsThreads = []
        let later = try #require(try cell(of: 1).menu(for: try rightClick(on: 1)))
        #expect(later.items.map(\.title) == ["Copy Text"])
        let text = try #require(messageText(in: try cell(of: 2)))
        actions.startsThreads = [2]
        #expect(text.menu(for: try rightClick(on: 2))?.items.first?.title == "Create Thread")
    }

    func contentColor(of raw: UInt64, text: String) throws -> NSColor? {
        let body = try #require(messageText(in: try cell(of: raw)))
        #expect(body.string.hasPrefix(text))
        return body.textStorage?.attribute(.foregroundColor, at: 0, effectiveRange: nil)
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

        let text = try #require(messageText(in: try cell(of: 1))).attributedString()
        #expect(text.string == "message 1 (edited)")
        let marker = text.attributes(at: text.length - 1, effectiveRange: nil)
        #expect((marker[.font] as? NSFont)?.pointSize == 12)
        #expect(marker[.foregroundColor] as? NSColor == Palette.chatTextMuted)
        let extras = shownTexts(in: try cell(of: 2)).filter { $0.contains("a.zip") }
        #expect(try #require(extras.first).hasSuffix(" (edited)"))
        #expect(try #require(messageText(in: try cell(of: 3))).string == "message 3")
    }

    // 100 one-line messages, 101...200, two minutes apart on one day.
    var page: [MessageListModel.Row] { messages(100, from: 101) }

    func heights(_ raws: ClosedRange<UInt64>) -> CGFloat {
        raws.reduce(0) { $0 + table.rect(ofRow: index(of: $1)).height }
    }

    // What the reader sees: each visible message's row view and where it sits on screen.
    func visibleViews() -> [UInt64: (view: NSView?, y: CGFloat)] {
        var shown: [UInt64: (view: NSView?, y: CGFloat)] = [:]
        for row in visibleRows {
            guard case .message(let item, _) = controller.timeline.items[row] else {
                continue
            }
            let rowView = table.rowView(atRow: row, makeIfNecessary: false)
            shown[item.id.rawValue] = (
                table.view(atColumn: 0, row: row, makeIfNecessary: false),
                (rowView?.frame.minY ?? .nan) - clip.bounds.minY
            )
        }
        return shown
    }

    // Checked right after the update, before any layout pass: the frame AppKit would draw next.
    func expectTheSameViewsInPlace(_ before: [UInt64: (view: NSView?, y: CGFloat)]) {
        for (raw, was) in before {
            let row = index(of: raw)
            let view = table.view(atColumn: 0, row: row, makeIfNecessary: false)
            #expect(view != nil && view === was.view, "message \(raw) got another view")
            // Where its row view is now, not where the table will put it at its next layout.
            let rowView = table.rowView(atRow: row, makeIfNecessary: false)
            let y = (rowView?.frame.minY ?? .nan) - clip.bounds.minY
            #expect(abs(y - was.y) <= 0.5, "message \(raw) moved from \(was.y) to \(y)")
        }
    }

    @Test
    func aPageLandingAboveKeepsTheVisibleRowsAndTheirViews() {
        show(page)
        scroll(toRowOf: 130)
        let before = visibleViews()
        #expect(!before.isEmpty)

        controller.show(MessageTableState(rows: messages(50, from: 51) + page))

        expectTheSameViewsInPlace(before)
    }

    @Test
    func aPageLandingAboveDuringAFlingKeepsTheVisibleRowsAndTheirViews() {
        show(page)
        scroll(toRowOf: 130)
        postLiveScroll(NSScrollView.willStartLiveScrollNotification)
        postLiveScroll(NSScrollView.didLiveScrollNotification)
        let before = visibleViews()

        controller.show(MessageTableState(rows: messages(50, from: 51) + page))

        expectTheSameViewsInPlace(before)
    }

    // A tall window and long messages, where the table's first answer for new rows is an
    // estimate until it has measured them.
    @Test
    func aPageLandingAboveATallFlingStaysExactlyInPlace() {
        let controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { nil })
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1000, height: 900),
            styleMask: [.titled, .resizable], backing: .buffered, defer: false)
        window.contentView = controller.scrollView
        window.layoutIfNeeded()
        let clip = controller.scrollView.contentView
        let table = controller.tableView
        let rows = { (range: ClosedRange<UInt64>) in
            range.map { raw in
                row(
                    raw, by: author(raw % 4 == 0 ? 2 : 1), at: self.noon + Double(raw) * 40,
                    content: String(
                        repeating: "some words that wrap ",
                        count: [1, 1, 2, 1, 6, 1, 3, 1, 14, 2][Int(raw % 10)] * 6))
            }
        }
        controller.show(MessageTableState(rows: rows(301...400)))
        window.layoutIfNeeded()
        NotificationCenter.default.post(
            name: NSScrollView.willStartLiveScrollNotification, object: controller.scrollView)
        for _ in 0..<30 {
            clip.scroll(to: NSPoint(x: 0, y: max(clip.bounds.minY - 90, 0)))
            controller.scrollView.reflectScrolledClipView(clip)
            NotificationCenter.default.post(
                name: NSScrollView.didLiveScrollNotification, object: controller.scrollView)
            window.layoutIfNeeded()
        }
        let place = { (raw: UInt64) -> (row: CGFloat, view: CGFloat) in
            let index = controller.timeline.items.firstIndex {
                $0.id == .message(MessageId(rawValue: raw))
            }
            guard let index else {
                return (.nan, .nan)
            }
            let view = table.rowView(atRow: index, makeIfNecessary: false)?.frame.minY ?? .nan
            return (table.rect(ofRow: index).minY - clip.bounds.minY, view - clip.bounds.minY)
        }
        let visible = table.rows(in: clip.bounds)
        let watched = (visible.location..<(visible.location + visible.length)).compactMap {
            row -> UInt64? in
            if case .message(let item, _) = controller.timeline.items[row] {
                return item.id.rawValue
            }
            return nil
        }
        let before = watched.map { place($0).row }

        controller.show(MessageTableState(rows: rows(251...300) + rows(301...400)))

        for (raw, was) in zip(watched, before) {
            let now = place(raw)
            #expect(abs(now.row - was) <= 0.5, "message \(raw) moved from \(was) to \(now.row)")
            #expect(abs(now.view - was) <= 0.5, "message \(raw)'s view is at \(now.view)")
        }
    }

    @Test
    func aVisibleRowThatFailsGrowsByItsFailedLine() {
        var rows = messages(20)
        let last = rows.count - 1
        show(rows)
        #expect(visibleRows.contains(index(of: 20)))
        let before = table.rect(ofRow: index(of: 20)).height

        rows[last] = row(20, by: author(1), at: noon + 20 * 120, delivery: .failed)
        show(rows)

        let after = table.rect(ofRow: index(of: 20)).height
        #expect(after > before)
        #expect(abs(after - fittingHeight(ofRow: index(of: 20))) <= 0.5)
        #expect(visibleRows.contains(index(of: 20)))
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

        let general = ChannelName(kind: .guildChannel(.guildText), name: "general")
        show(MessageTableState(rows: page, reachedOldest: true, start: general.start))
        #expect(try cell(at: 0) is ChannelStartCell)
        let shown = shownTexts(in: try cell(at: 0))
        #expect(shown == ["Welcome to #general!", "This is the start of the #general channel."])
        #expect(table.rect(ofRow: 0).height == fittingHeight(ofRow: 0))
    }

    // Its rows reach their places in this update, as a page's do; a SwiftUI update can lay
    // the table out a frame later.
    @Test
    func aRenameThatGrowsTheStartKeepsTheVisibleRowsInPlace() {
        let short = ChannelName(kind: .guildChannel(.guildText), name: "general")
        let long = ChannelName(
            kind: .guildChannel(.guildText),
            name: String(repeating: "a-very-long-channel-name-", count: 4))
        show(MessageTableState(rows: page, reachedOldest: true, start: short.start))
        scroll(toRowOf: 102)
        let before = visibleViews()
        #expect(!before.isEmpty)

        controller.show(MessageTableState(rows: page, reachedOldest: true, start: long.start))

        expectTheSameViewsInPlace(before)
    }

    // The official client's welcome sits left, above the first message.
    @Test
    func theStartHeaderSitsLeftAboveTheFirstMessage() throws {
        let general = ChannelName(kind: .guildChannel(.guildText), name: "general")
        show(MessageTableState(rows: [row(1, at: noon)], reachedOldest: true, start: general.start))
        let start = try #require(try cell(at: 0) as? ChannelStartCell)
        let texts = messageTexts(in: start).filter { !$0.isHiddenOrHasHiddenAncestor }
        let title = try #require(texts.first { $0.string.hasPrefix("Welcome") })
        let subtitle = try #require(texts.first { $0.string.hasPrefix("This is") })
        let attribute = { (text: NSTextView, key: NSAttributedString.Key) in
            text.textStorage?.attribute(key, at: 0, effectiveRange: nil)
        }

        #expect(start.icon.frame.size == NSSize(width: 68, height: 68))
        #expect(start.icon.frame.minX == 16)
        #expect(title.frame.minX == 16)
        #expect(subtitle.frame.minX == 16)
        #expect(attribute(title, .font) as? NSFont == TypeScale.startTitle.nsFont)
        #expect(attribute(subtitle, .font) as? NSFont == TypeScale.startSubtitle.nsFont)
        // Measured: the message text's color, not a muted one.
        #expect(attribute(subtitle, .foregroundColor) as? NSColor == Palette.textDefault)
        // Measured from the official client: the title's baseline 39 below the icon, the
        // subtitle's 33.5 below the title's.
        let fromTop = { (view: NSView, y: CGFloat) in
            (start.isFlipped ? view.frame.minY : start.bounds.height - view.frame.maxY) + y
        }
        let iconBottom = fromTop(start.icon, start.icon.frame.height)
        let titleBaseline = fromTop(title, title.firstBaselineOffsetFromTop)
        let subtitleBaseline = fromTop(subtitle, subtitle.firstBaselineOffsetFromTop)
        #expect(abs(titleBaseline - iconBottom - 39) <= 0.5)
        #expect(abs(subtitleBaseline - titleBaseline - 33.5) <= 0.5)
        #expect(controller.timeline.items[1] == .day(berlin.startOfDay(for: noon)))
    }

    @Test
    func aLongChannelNameWrapsTheTitleAndTheRowGrows() throws {
        let short = ChannelName(kind: .guildChannel(.guildText), name: "general")
        let long = ChannelName(
            kind: .guildChannel(.guildText),
            name: String(repeating: "a-very-long-channel-name-", count: 4))
        show(MessageTableState(rows: page, reachedOldest: true, start: short.start))
        let one = table.rect(ofRow: 0).height

        show(MessageTableState(rows: page, reachedOldest: true, start: long.start))

        #expect(table.rect(ofRow: 0).height > one)
        #expect(table.rect(ofRow: 0).height == fittingHeight(ofRow: 0))
    }

    @Test
    func aDirectMessageStartsWithTheAvatarAndTheUsername() throws {
        let mira = ChannelName(kind: .direct(author(7, "Mira")), name: "Mira")
        show(MessageTableState(rows: page, reachedOldest: true, start: mira.start))
        let shown = shownTexts(in: try cell(at: 0))

        #expect(
            shown == [
                "Mira", "mira", "This is the beginning of your direct message history with Mira.",
            ])
        #expect(table.rect(ofRow: 0).height == fittingHeight(ofRow: 0))
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
    func escapeInTheListJumpsToThePresent() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        let escape = try #require(escapeKey())
        table.currentEvent = { escape }

        table.cancelOperation(nil)

        #expect(actions.calls == ["escape"])
    }

    @Test
    func shiftEscapeInTheListMarksTheServerRead() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        let escape = try #require(escapeKey(.shift))
        table.currentEvent = { escape }

        table.cancelOperation(nil)

        #expect(actions.calls == ["server read"])
    }

    // In the list and from a message's text.
    @Test
    func cmdPeriodDoesntMarkRead() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        let period = try #require(periodKey(.command))
        table.currentEvent = { period }

        table.cancelOperation(nil)
        try #require(messageText(in: try cell(of: 200))).cancelOperation(nil)

        #expect(actions.calls.filter { !$0.hasPrefix("more") }.isEmpty)
    }

    @Test
    func cmdShiftPeriodDoesntMarkTheServerRead() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        let period = try #require(periodKey([.command, .shift]))
        table.currentEvent = { period }

        table.cancelOperation(nil)
        try #require(messageText(in: try cell(of: 200))).cancelOperation(nil)

        #expect(actions.calls.filter { !$0.hasPrefix("more") }.isEmpty)
    }

    @Test
    func theTableReportsWhetherItIsAtTheBottom() throws {
        let actions = RecordingActions()
        controller.actions = actions
        show(page)
        #expect(actions.bottoms.isEmpty, "a list opens at the bottom")

        scroll(toRowOf: 120)
        #expect(actions.bottoms == [false])

        let bottomClip = try #require(clip as? BottomClipView)
        clip.scroll(to: NSPoint(x: 0, y: bottomClip.originRange.upperBound))
        controller.scrollView.reflectScrolledClipView(clip)
        window.layoutIfNeeded()
        #expect(actions.bottoms == [false, true])

        scroll(toRowOf: 150)
        controller.jumpToPresent(load: true)
        #expect(actions.bottoms == [false, true, false, true])
    }

    // The bar's jump, a jump to the present and pinning; only the user's scrolls are reported.
    @Test
    func theTablesOwnScrollsReportNoUserScroll() {
        let actions = RecordingActions()
        controller.actions = actions
        let line = MessageId(rawValue: 150)
        show(MessageTableState(rows: page, newMessages: line))
        show(MessageTableState(rows: page, newMessages: line, unreadJumps: 1))
        controller.jumpToPresent(load: false)
        show(
            MessageTableState(rows: page + messages(1, from: 201), newMessages: line, unreadJumps: 1))
        #expect(actions.bottoms == [false, true])
        #expect(actions.scrolls.isEmpty)

        scroll(toRowOf: 120)

        #expect(actions.scrolls.count == 1)
        #expect(actions.scrolls.allSatisfy { $0 > 0 }, "toward older messages")
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
        #expect(
            edge.maxY
                <= clip.bounds.maxY - ComposerStatus.height - JumpBar.height - JumpBar.margin)

        show(page)
        #expect(controller.scrollView.contentInsets.bottom == restingInset)
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

    // Measured in the official client: 24 from the newest message's box to the composer.
    @Test
    func theNewestBoxEnds24AboveTheComposer() throws {
        show(messages(40))
        let cell = try #require(try cell(at: table.numberOfRows - 1) as? MessageCell)

        let boxBottom = cell.convert(cell.box, to: table).maxY
        #expect(abs(clip.bounds.maxY - boxBottom - 24) <= 0.5)
    }

    // Measured in the official client: from the divider's line to the first avatar 21.5.
    @Test
    func theDividersLineSits21AndAHalfAboveTheFirstAvatar() throws {
        show(MessageTableState(rows: [row(1, at: noon)], reachedOldest: true))
        let day = try #require(
            controller.timeline.items.firstIndex { if case .day = $0 { true } else { false } })
        let divider = try #require(try cell(at: day) as? DayDividerCell)
        let message = try cell(of: 1)
        let date = try #require(textFields(in: divider).first)
        let avatar = try #require(message.subviews.first { $0 is ImageLayerView })

        let line = date.convert(date.bounds, to: table).midY
        let avatarTop = avatar.convert(avatar.bounds, to: table).minY
        #expect(abs(avatarTop - line - 21.5) <= 0.5)
        // The date stays inside its own row.
        #expect(date.convert(date.bounds, to: table).maxY <= table.rect(ofRow: day).maxY)
    }

    // An older message of its day landing above it moves the divider away from it.
    @Test
    func aMessageLeavingItsDividerGetsTheGroupSpacingBack() throws {
        let first = row(2, by: author(2), at: noon + 600)
        show(MessageTableState(rows: [first], reachedOldest: true))
        let close = table.rect(ofRow: index(of: 2)).height

        show(MessageTableState(rows: [row(1, at: noon), first], reachedOldest: true))

        #expect(table.rect(ofRow: index(of: 2)).height == close + 10)
        #expect(table.rect(ofRow: index(of: 2)).height == fittingHeight(ofRow: index(of: 2)))
    }

    @Test
    func aShortFirstPageSitsAtTheBottom() {
        show(MessageTableState(rows: [], loading: .latest))

        show(MessageTableState(rows: messages(3), reachedOldest: true))

        let bottom = clip.bounds.maxY - table.rect(ofRow: table.numberOfRows - 1).maxY
        #expect(abs(bottom - restingInset) <= 1)
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
