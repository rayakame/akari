import AkariKit
import AppKit
import Testing

@testable import Akari

// A message list with a pointer and modifier keys of the test's own, its actions recorded.
@MainActor
final class BarList {
    let pointer = MessageTableTests.Pointer()
    let actions = MessageTableTests.RecordingActions()
    let controller: MessageTableController
    let window: NSWindow
    let noon = berlinTime(2026, 10, 10, 12, 0)
    var shift = false
    var popped: [(titles: [String], view: NSView, point: NSPoint, moreSelected: Bool)] = []
    private(set) var rows: [MessageListModel.Row] = []

    static let plain: [MessageAction] = [.createThread, .more]
    static let shifted: [MessageAction] = [.copyLink, .markUnread, .createThread, .more]

    init(scale: CGFloat? = nil, height: CGFloat = 400) {
        controller = MessageTableController(
            calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) },
            pointer: { [pointer] in pointer.location })
        window =
            scale.map { ScaledWindow.make(scale: $0) }
            ?? TestWindow.make(size: NSSize(width: 600, height: height), style: [.titled, .resizable])
        window.setContentSize(NSSize(width: 600, height: height))
        window.contentView = controller.scrollView
        controller.actions = actions
        controller.modifiers = { [unowned self] in shift ? .shift : [] }
        controller.popUp = { [unowned self] menu, point, view in
            NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
            popped.append(
                (menu.items.map(\.title), view, point, controller.hoverBar.button(.more)?.isSelected == true))
            NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)
        }
        window.layoutIfNeeded()
    }

    var table: MessageTableView { controller.tableView }
    var bar: MessageHoverBar { controller.hoverBar }
    var clip: NSClipView { controller.scrollView.contentView }

    // Every message gets the same bar unless the test says otherwise.
    func show(_ rows: [MessageListModel.Row], bars: Bool = true) {
        self.rows = rows
        if bars {
            for row in rows where actions.bars[row.id.rawValue] == nil {
                actions.bars[row.id.rawValue] = (Self.plain, Self.shifted)
            }
        }
        controller.show(MessageTableState(rows: rows, reachedOldest: true))
        window.layoutIfNeeded()
    }

    // Ray's group of three, Mira's reply, a join line, a failed send.
    func showGroups() {
        actions.bars[6] = ([.retry, .delete], [.retry, .delete])
        show([
            row(1, by: author(2, "Ray"), at: noon, content: "first"),
            row(2, by: author(2, "Ray"), at: noon + 10, content: "second"),
            row(3, by: author(2, "Ray"), at: noon + 20, content: "third"),
            row(4, by: author(3), at: noon + 600, kind: .reply, content: "a reply"),
            row(5, by: author(4, "Kim"), at: noon + 700, kind: .userJoin),
            row(6, at: noon + 800, delivery: .failed),
        ])
    }

    func many(_ count: Int, longText: Bool = false) -> [MessageListModel.Row] {
        (1...UInt64(count)).map { raw in
            row(
                raw, by: author(raw % 3 == 0 ? 2 : 1), at: noon + Double(raw) * 120,
                content: longText ? String(repeating: "words that wrap ", count: 12) : nil)
        }
    }

    func index(of raw: UInt64) -> Int {
        controller.timeline.items.firstIndex { $0.id == .message(MessageId(rawValue: raw)) } ?? -1
    }

    func cell(of raw: UInt64) throws -> NSView & MessageMenuRow {
        let cell = try #require(
            table.view(atColumn: 0, row: index(of: raw), makeIfNecessary: true) as? (NSView & MessageMenuRow))
        cell.layoutSubtreeIfNeeded()
        return cell
    }

    /// The message's box in the table's coordinates.
    func box(of raw: UInt64) throws -> NSRect {
        let cell = try cell(of: raw)
        return table.convert(cell.box, from: cell)
    }

    /// The pointer goes to a point of the table.
    func move(to point: NSPoint) {
        pointer.location = table.convert(point, to: nil)
        controller.pointerMoved()
    }

    func hover(_ raw: UInt64) throws {
        let box = try box(of: raw)
        move(to: NSPoint(x: box.minX + 100, y: box.midY))
    }

    /// The message whose row is tinted, and the one the bar's click would act on.
    var hovered: [UInt64] {
        let rows = table.rows(in: table.visibleRect)
        return (rows.location..<NSMaxRange(rows)).compactMap { row -> UInt64? in
            guard (table.view(atColumn: 0, row: row, makeIfNecessary: false) as? MessageCell)?.isHovered == true,
                case .message(let item, _) = controller.timeline.items[row]
            else {
                return nil
            }
            return item.id.rawValue
        }
    }

    /// The message the bar acts on: its first button's click names it.
    func barOwner() throws -> UInt64? {
        guard !bar.isHidden, let button = bar.buttons.first else {
            return nil
        }
        actions.calls = []
        button.performClick(nil)
        let call = try #require(actions.calls.last)
        return call.split(separator: " ").compactMap { UInt64($0) }.first
    }

    func menuEvent(on raw: UInt64) throws -> NSEvent {
        let cell = try cell(of: raw)
        return try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: cell.convert(NSPoint(x: cell.box.midX, y: cell.box.midY), to: nil),
                modifierFlags: [], timestamp: 0, windowNumber: window.windowNumber, context: nil,
                eventNumber: 0, clickCount: 1, pressure: 1))
    }

    func click(_ flags: NSEvent.ModifierFlags, at point: NSPoint) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: .leftMouseDown, location: table.convert(point, to: nil), modifierFlags: flags,
                timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1))
    }
}

// The hover bar: one view of the list's controller, over the hovered message's top right corner
// as the official client's (34 high, 28 a button, 14 from the row's right edge, its top 25 above
// the row's, 16 over a group's first message).
@MainActor
struct HoverBarTests {
    @Test(arguments: [CGFloat(1), 2])
    func theBarSitsOverItsRowsTopRightCorner(scale: CGFloat) throws {
        let list = BarList(scale: scale)
        list.showGroups()

        // 25 above a follow-up, a reply at a group's start and a system line; 16 above a group's
        // first message.
        for (raw, overhang): (UInt64, CGFloat) in [(2, 25), (3, 25), (1, 16), (4, 25), (5, 25)] {
            try list.hover(raw)
            let box = try list.box(of: raw)
            let frame = list.bar.frame

            #expect(!list.bar.isHidden, "message \(raw)")
            #expect(frame.height == 34)
            #expect(frame.width == 62)
            #expect(abs(frame.maxX - (box.maxX - 14)) < 1 / scale, "message \(raw)")
            #expect(abs(frame.minY - (box.minY - overhang)) < 1 / scale, "message \(raw): \(frame) over \(box)")
            #expect((frame.minY * scale).rounded() == frame.minY * scale)
            #expect((frame.minX * scale).rounded() == frame.minX * scale)
        }
        #expect(
            list.bar.buttons.map(\.frame)
                == [NSRect(x: 3, y: 3, width: 28, height: 28), NSRect(x: 31, y: 3, width: 28, height: 28)])
    }

    @Test
    func theBarGrowsWithItsButtons() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        let right = list.bar.frame.maxX

        list.shift = true
        list.controller.pointerMoved()

        #expect(list.bar.frame.width == 118)
        #expect(list.bar.frame.maxX == right)
    }

    // The part of the bar that lies over the row above is its own row's.
    @Test
    func thePointerOverTheOverhangKeepsTheBarsRowHovered() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(3)
        let bar = list.bar.frame
        let above = try list.box(of: 2)
        #expect(bar.minY < above.maxY - 20, "the bar overhangs message 2")

        var y = try list.box(of: 3).midY
        while y > bar.minY {
            list.move(to: NSPoint(x: bar.midX, y: y))
            #expect(list.hovered == [3], "at \(y)")
            #expect(list.bar.frame == bar, "at \(y)")
            #expect(try list.barOwner() == 3)
            y -= 1
        }

        // Beside the bar, at the same height, the row above is hovered and the bar is its own.
        list.move(to: NSPoint(x: bar.minX - 40, y: bar.minY + 4))
        #expect(list.hovered == [2])
        #expect(try list.barOwner() == 2)
        #expect(list.bar.frame.minY < bar.minY)
    }

    @Test
    func aClickOnTheOverhangReachesTheBarNotTheRowUnderIt() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(3)
        let content = try #require(list.window.contentView)
        let button = try #require(list.bar.buttons.first)
        let point = button.convert(NSPoint(x: 14, y: 6), to: nil)
        #expect(try list.box(of: 2).contains(list.table.convert(point, from: nil)))

        #expect(content.hitTest(content.superview?.convert(point, from: nil) ?? point) === button)
        #expect(list.controller.cursor(at: point) == .pointingHand)
        let beside = list.bar.convert(NSPoint(x: 1, y: 1), to: nil)
        #expect(content.hitTest(content.superview?.convert(beside, from: nil) ?? beside) === list.bar)
        #expect(list.bar.menu(for: try list.menuEvent(on: 2)) == nil)
    }

    // The table adds row views as it scrolls, over whatever it holds already.
    @Test
    func aRowViewAddedOverTheBarDoesNotTakeItsClicks() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(3)
        let content = try #require(list.window.contentView)
        let button = try #require(list.bar.buttons.first)
        let point = button.convert(NSPoint(x: 14, y: 14), to: nil)

        list.table.addSubview(NSView(frame: list.bar.frame.insetBy(dx: -20, dy: -20)))

        #expect(content.hitTest(content.superview?.convert(point, from: nil) ?? point) === button)
        list.controller.pointerMoved()
        #expect(list.table.subviews.last === list.bar)
    }

    // The official client cuts the bar off at the list's top; Akari keeps it inside.
    @Test
    func aRowAtTheListsTopKeepsItsBarInSight() throws {
        let list = BarList()
        list.show(list.many(80))
        let row = list.table.rect(ofRow: list.index(of: 40))
        list.clip.scroll(to: NSPoint(x: 0, y: row.minY + 2))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()

        list.move(to: NSPoint(x: 200, y: row.maxY - 4))

        #expect(list.hovered == [40])
        #expect(list.bar.frame.minY == list.table.visibleRect.minY + 4)
        #expect(list.table.visibleRect.contains(list.bar.frame))
    }

    @Test
    func theControllerLightsTheButtonUnderThePointer() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        let (first, more) = (list.bar.buttons[0], list.bar.buttons[1])
        #expect(!first.isHovered && !more.isHovered && !list.bar.isPointerInside)

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: more))
        #expect(more.isHovered && !first.isHovered && list.bar.isPointerInside)
        #expect(more.contentTintColor == Palette.interactiveTextActive)
        #expect(first.contentTintColor == Palette.interactiveText)

        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: first))
        #expect(first.isHovered && !more.isHovered)

        try list.hover(2)
        #expect(!first.isHovered && !more.isHovered && !list.bar.isPointerInside)
        let own = (first.trackingAreas + list.bar.trackingAreas).filter { $0.owner === first || $0.owner === list.bar }
        #expect(own.isEmpty, "\((first.trackingAreas + list.bar.trackingAreas).map { String(describing: $0.owner) })")
    }

    @Test
    func aRowWithoutButtonsHasNoBar() throws {
        let list = BarList()
        list.show(
            [row(1, at: list.noon), row(2, at: list.noon + 10, delivery: .pending)], bars: false)
        list.actions.bars[1] = (BarList.plain, BarList.shifted)

        try list.hover(2)
        #expect(list.bar.isHidden)
        try list.hover(1)
        #expect(!list.bar.isHidden)
        try list.hover(2)
        #expect(list.bar.isHidden)
    }

    @Test
    func noPointerNoBar() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        #expect(!list.bar.isHidden)

        // The window isn't key, or another window lies over the list.
        list.pointer.location = nil
        list.controller.pointerMoved()

        #expect(list.bar.isHidden)
        #expect(list.hovered.isEmpty)
    }

    @Test
    func theSpaceBeforeAGroupAndADayDividerHaveNoBar() throws {
        let list = BarList()
        list.showGroups()
        let reply = try list.box(of: 4)

        list.move(to: NSPoint(x: 200, y: reply.minY - 8))
        #expect(list.bar.isHidden)
        list.move(to: NSPoint(x: 200, y: list.table.rect(ofRow: 0).midY))
        #expect(list.bar.isHidden)
    }

    @Test
    func aFailedSendsBarRetriesAndDeletes() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(6)

        #expect(list.bar.items.map(\.action) == [.retry, .delete])
        list.actions.calls = []
        list.bar.buttons[0].performClick(nil)
        list.bar.buttons[1].performClick(nil)
        #expect(list.actions.calls == ["retry 6", "delete 6"])

        // The line under the message keeps its own links.
        func buttons(_ view: NSView) -> [NSButton] {
            ((view as? NSButton).map { [$0] } ?? []) + view.subviews.flatMap(buttons)
        }
        let line = buttons(try list.cell(of: 6))
        list.actions.calls = []
        try #require(line.first { $0.title == "Retry" }).performClick(nil)
        try #require(line.first { $0.title == "Delete" }).performClick(nil)
        #expect(list.actions.calls == ["retry 6", "delete 6"])
    }

    @Test
    func eachButtonReachesTheActionsWithShiftAsItIsAtTheClick() throws {
        let list = BarList()
        list.showGroups()
        list.shift = true
        try list.hover(2)
        #expect(list.bar.items.map(\.action) == BarList.shifted)

        list.actions.calls = []
        for button in list.bar.buttons.dropLast() {
            button.performClick(nil)
        }
        #expect(list.actions.calls == ["copyLink 2 shift", "markUnread 2 shift", "create thread from 2"])

        // The bar was read with Shift; the click reads the key again.
        list.shift = false
        list.actions.calls = []
        list.bar.buttons[0].performClick(nil)
        #expect(list.actions.calls == ["copyLink 2"])
    }

    @Test
    func shiftSwitchesTheBarWhileThePointerRests() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        #expect(list.bar.buttons.map(\.toolTip) == ["createThread", "more"])
        let flags = try #require(
            NSEvent.keyEvent(
                with: .flagsChanged, location: .zero, modifierFlags: .shift, timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, characters: "",
                charactersIgnoringModifiers: "", isARepeat: false, keyCode: 56))

        list.shift = true
        #expect(!list.controller.monitored(flags))
        #expect(list.bar.buttons.map(\.toolTip) == ["copyLink", "markUnread", "createThread", "more"])
        #expect(list.bar.buttons.map { $0.accessibilityLabel() } == ["copyLink", "markUnread", "createThread", "more"])

        list.shift = false
        #expect(!list.controller.monitored(flags))
        #expect(list.bar.items.map(\.action) == BarList.plain)
        #expect(list.hovered == [2])
    }

    @Test
    func theBarIsAGroupOfLabelledButtons() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)

        #expect(list.bar.accessibilityRole() == .group)
        #expect(list.bar.accessibilityLabel() == "Message Actions")
        #expect(list.bar.buttons.allSatisfy { $0.accessibilityLabel() == $0.item.title && $0.toolTip == $0.item.title })
        #expect(list.bar.buttons.allSatisfy { $0.image != nil && $0.refusesFirstResponder })
    }

    @Test
    func theBarFollowsItsRowOnEveryScroll() throws {
        let list = BarList()
        list.show(list.many(80, longText: true))
        list.pointer.location = NSPoint(x: 200, y: 200)
        list.controller.pointerMoved()
        var owners: Set<UInt64> = []

        for step in 0..<100 {
            list.clip.scroll(to: NSPoint(x: 0, y: list.clip.bounds.minY - 7))
            list.controller.scrollView.reflectScrolledClipView(list.clip)
            list.window.layoutIfNeeded()
            let hovered = list.hovered
            guard let raw = hovered.first else {
                #expect(list.bar.isHidden, "step \(step)")
                continue
            }
            let box = try list.box(of: raw)
            #expect(!list.bar.isHidden, "step \(step)")
            #expect(list.bar.frame.maxX == box.maxX - 14, "step \(step)")
            let tops = [CGFloat(16), 25].map { max(box.minY - $0, list.table.visibleRect.minY + 4) }
            #expect(tops.contains { abs(list.bar.frame.minY - $0) <= 0.5 }, "step \(step)")
            #expect(try list.barOwner() == raw, "step \(step)")
            owners.insert(raw)
        }
        #expect(owners.count > 3, "the pointer saw several rows")
    }

    // A reused cell never brings another row's bar: the bar isn't the cell's.
    @Test
    func aRowScrolledAwayTakesItsBarWithIt() throws {
        let list = BarList()
        list.show(list.many(120))
        try list.hover(118)
        #expect(try list.barOwner() == 118)

        list.clip.scroll(to: NSPoint(x: 0, y: 0))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()

        let owner = try list.barOwner()
        #expect(owner != 118)
        #expect(owner.map { [$0] } ?? [] == list.hovered)
        if let owner {
            #expect(list.table.visibleRect.intersects(try list.box(of: owner)))
            #expect(list.table.visibleRect.contains(list.bar.frame))
        }
    }

    @Test
    func aReloadOfTheHoveredRowReadsItsBarAgain() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        list.actions.calls = []
        list.controller.pointerMoved()
        #expect(list.actions.calls.isEmpty, "a move inside the row reads nothing")

        var rows = list.rows
        rows[1] = row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second, edited")
        list.actions.bars[2] = ([.viewThread, .more], BarList.shifted)
        list.show(rows)

        #expect(list.actions.calls.contains("bar of 2"))
        #expect(list.bar.items.map(\.action) == [.viewThread, .more])
        #expect(list.hovered == [2])
        let box = try list.box(of: 2)
        #expect(list.bar.frame.minY == box.minY - 25 && list.bar.frame.maxX == box.maxX - 14)
        #expect(list.table.subviews.last === list.bar)
    }

    @Test
    func aRowGrowingAboveTheReaderMovesTheBarWithItsRow() throws {
        let list = BarList()
        var rows = list.many(100)
        list.show(rows)
        list.clip.scroll(to: NSPoint(x: 0, y: list.table.rect(ofRow: list.index(of: 50)).minY))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        list.window.layoutIfNeeded()
        try list.hover(56)
        let offset = list.bar.frame.minY - list.clip.bounds.minY
        let height = list.table.rect(ofRow: list.index(of: 40)).height

        rows[39] = row(
            40, by: rows[39].message.author, at: rows[39].message.timestamp,
            content: String(repeating: "words that wrap ", count: 40))
        list.show(rows)

        #expect(list.table.rect(ofRow: list.index(of: 40)).height > height)
        #expect(list.hovered == [56])
        #expect(try list.barOwner() == 56)
        let box = try list.box(of: 56)
        #expect(abs(list.bar.frame.minY - (box.minY - 25)) <= 0.5)
        #expect(abs(list.bar.frame.minY - list.clip.bounds.minY - offset) <= 0.5, "the bar stays on screen where it was")
    }

    @Test
    func theHoveredRowGrowingKeepsItsBarAtItsTop() throws {
        let list = BarList()
        var rows = list.many(100)
        list.show(rows)
        try list.hover(99)

        rows[98] = row(
            99, by: rows[98].message.author, at: rows[98].message.timestamp,
            content: String(repeating: "words that wrap ", count: 40))
        list.show(rows)
        list.controller.pointerMoved()

        if let raw = list.hovered.first {
            let box = try list.box(of: raw)
            #expect(list.bar.frame.maxX == box.maxX - 14)
            #expect(try list.barOwner() == raw)
            #expect(list.bar.frame.minY <= box.minY)
        } else {
            #expect(list.bar.isHidden)
        }
    }

    @Test
    func aHoveredRowThatGoesLeavesNoBarBehind() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(3)
        var rows = list.rows

        rows.remove(at: 2)
        list.show(rows)

        let owner = try list.barOwner()
        #expect(owner != 3)
        #expect(list.hovered == (owner.map { [$0] } ?? []))

        list.show([])
        #expect(list.bar.isHidden)
        list.controller.pointerMoved()
        #expect(list.bar.isHidden)
    }

    @Test
    func theRowStaysLitWhileItsMenuIsOpen() throws {
        let list = BarList()
        list.showGroups()
        try list.hover(2)
        let frame = list.bar.frame
        let menu = try #require(try list.cell(of: 2).menu(for: try list.menuEvent(on: 2)))

        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
        try list.hover(4)
        list.shift = true
        list.controller.pointerMoved()

        #expect(list.hovered == [2])
        #expect(list.bar.frame == frame)
        #expect(list.bar.items.map(\.action) == BarList.plain, "Shift doesn't expand the bar under a menu")
        #expect(try list.barOwner() == 2)

        list.shift = false
        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)

        #expect(list.hovered == [4])
        #expect(try list.barOwner() == 4)
    }

    @Test
    func aMenuOpenedWithoutThePointerLightsItsRow() throws {
        let list = BarList()
        list.showGroups()
        let menu = try #require(try list.cell(of: 2).menu(for: try list.menuEvent(on: 2)))

        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
        #expect(list.hovered == [2])
        #expect(!list.bar.isHidden)

        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)
        #expect(list.hovered.isEmpty)
        #expect(list.bar.isHidden)
    }

    @Test
    func anotherListsMenuOrAnyOtherMenuLocksNothing() throws {
        let list = BarList()
        let other = BarList()
        list.showGroups()
        other.showGroups()
        try list.hover(2)
        let menu = try #require(try other.cell(of: 3).menu(for: try other.menuEvent(on: 3)))

        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: NSMenu())
        try list.hover(4)

        #expect(list.hovered == [4])
        #expect(other.hovered == [3])
        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)
        #expect(other.hovered.isEmpty)
    }

    @Test
    func aRowRemovedUnderItsOpenMenuLetsGo() throws {
        let list = BarList()
        list.actions.startsThreads = [3]
        list.showGroups()
        try list.hover(3)
        let menu = try #require(try list.cell(of: 3).menu(for: try list.menuEvent(on: 3)))
        NotificationCenter.default.post(name: NSMenu.didBeginTrackingNotification, object: menu)
        var rows = list.rows

        rows.remove(at: 2)
        list.show(rows)

        #expect(try list.barOwner() != 3)
        list.actions.calls = []
        menu.performActionForItem(at: 0)
        #expect(list.actions.calls == ["create thread from 3"], "the item still names its message")
        NotificationCenter.default.post(name: NSMenu.didEndTrackingNotification, object: menu)
    }

    // The split view: two lists in one window under one pointer, each with its own bar.
    @Test
    func eachPaneHasItsOwnBar() throws {
        let pointer = MessageTableTests.Pointer()
        let noon = berlinTime(2026, 10, 10, 12, 0)
        let window = TestWindow.make(size: NSSize(width: 900, height: 400), style: [.titled, .resizable])
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 900, height: 400))
        window.contentView = container
        let panes = (0..<2).map { index -> (MessageTableController, MessageTableTests.RecordingActions) in
            let controller = MessageTableController(
                calendar: { berlin }, now: { berlinTime(2026, 10, 10, 15, 0) }, pointer: { pointer.location })
            let actions = MessageTableTests.RecordingActions()
            for raw in 1...3 as ClosedRange<UInt64> {
                actions.bars[raw] = (BarList.plain, BarList.shifted)
            }
            controller.actions = actions
            controller.modifiers = { [] }
            controller.scrollView.frame = NSRect(x: CGFloat(index) * 450, y: 0, width: 450, height: 400)
            container.addSubview(controller.scrollView)
            controller.show(
                MessageTableState(
                    rows: (1...3).map { row(UInt64($0), by: author(2, "Ray"), at: noon + Double($0) * 10) },
                    reachedOldest: true))
            return (controller, actions)
        }
        window.layoutIfNeeded()
        let (channel, thread) = (panes[0].0, panes[1].0)

        for (over, other) in [(thread, channel), (channel, thread)] {
            let row = over.tableView.rect(ofRow: over.tableView.numberOfRows - 1)
            pointer.location = over.tableView.convert(NSPoint(x: 100, y: row.midY), to: nil)
            channel.pointerMoved()
            thread.pointerMoved()

            #expect(!over.hoverBar.isHidden)
            #expect(other.hoverBar.isHidden)
            #expect(over.hoverBar.superview === over.tableView)
            #expect(over.tableView.visibleRect.contains(over.hoverBar.frame))
        }
        panes[0].1.calls = []
        channel.hoverBar.buttons[0].performClick(nil)
        #expect(panes[0].1.calls == ["create thread from 3"])
        #expect(!panes[1].1.calls.contains { $0.hasPrefix("create") })
    }

    @Test
    func moreOpensTheRowsMenuUnderTheButton() throws {
        let list = BarList()
        list.actions.startsThreads = [2]
        list.actions.marksUnread = [2]
        list.showGroups()
        try list.hover(2)
        let more = try #require(list.bar.button(.more))
        let context = try #require(try list.cell(of: 2).menu(for: try list.menuEvent(on: 2)))

        more.performClick(nil)

        let popped = try #require(list.popped.first)
        #expect(popped.titles == ["Create Thread", "", "Copy Text", "Mark Unread"])
        #expect(popped.titles == context.items.map(\.title))
        #expect(popped.view === more)
        #expect(popped.point == NSPoint(x: 0, y: more.bounds.maxY + 4))
        #expect(popped.moreSelected, "More is lit while its menu is open")
        #expect(!more.isSelected)
        #expect(list.hovered == [2])
        #expect(!list.actions.calls.contains { $0.hasPrefix("more") })
    }

    @Test
    func theMenusItemsRunTheirActions() throws {
        let list = BarList()
        list.actions.startsThreads = [2]
        list.actions.marksUnread = [2]
        list.showGroups()
        let menu = try #require(try list.cell(of: 2).menu(for: try list.menuEvent(on: 2)))

        list.actions.calls = []
        for index in [0, 2, 3] {
            menu.performActionForItem(at: index)
        }

        #expect(list.actions.calls == ["create thread from 2", "copyText 2", "markUnread 2"])
    }

    // A system line has the same menu and bar as a message.
    @Test
    func aNoticeHasItsMenuAndBar() throws {
        let list = BarList()
        list.actions.marksUnread = [5]
        list.showGroups()

        let menu = try #require(try list.cell(of: 5).menu(for: try list.menuEvent(on: 5)))
        #expect(menu.items.map(\.title).contains("Mark Unread"))
        try list.hover(5)
        #expect(try list.barOwner() == 5)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theBarTakesTheThemesColors(_ name: NSAppearance.Name) throws {
        let list = BarList()
        list.window.appearance = NSAppearance(named: name)
        list.showGroups()
        try list.hover(2)
        let layer = try #require(list.bar.layer)
        func resolved(_ color: NSColor) -> [CGFloat] {
            var cg = CGColor.clear
            list.bar.effectiveAppearance.performAsCurrentDrawingAppearance { cg = color.cgColor }
            return Self.parts(cg)
        }

        #expect(Self.parts(layer.backgroundColor) == resolved(Palette.surfaceHigh))
        #expect(Self.parts(layer.borderColor) == resolved(Palette.borderMuted))
        #expect(layer.borderWidth == 1 && layer.cornerRadius == 8)
        #expect(layer.shadowOpacity == (name == .darkAqua ? 0.14 : 0.08))
        #expect(layer.shadowRadius == 2 && layer.shadowOffset == CGSize(width: 0, height: -1))

        let more = try #require(list.bar.button(.more))
        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: more))
        #expect(Self.parts(layer.borderColor) == resolved(Palette.borderSubtle))
        #expect(layer.shadowOpacity == (name == .darkAqua ? 0.16 : 0.08))
        #expect(layer.shadowRadius == 4 && layer.shadowOffset == CGSize(width: 0, height: -4))
        #expect(Self.parts(more.layer?.backgroundColor) == resolved(Palette.hoverBackground))
        #expect(more.layer?.cornerRadius == 6)
        more.isSelected = true
        #expect(Self.parts(more.layer?.backgroundColor) == resolved(Palette.selectedBackground))
    }

    static func parts(_ color: CGColor?) -> [CGFloat] {
        let srgb = color?.converted(to: CGColorSpace(name: CGColorSpace.sRGB)!, intent: .defaultIntent, options: nil)
        return (srgb?.components ?? []).map { ($0 * 255).rounded() / 255 }
    }

    @Test
    func everySymbolFitsTheIconBox() throws {
        let symbols = [
            "face.smiling", "arrowshape.turn.up.left", "pencil", "text.bubble", "doc.on.doc", "pin",
            "pin.slash", "arrow.clockwise", "message.badge", "link", "xmark.circle", "trash", "number",
            "ellipsis",
        ]
        for symbol in symbols {
            let image = try #require(HoverBarButton.icon(symbol), "\(symbol)")
            let side = 28
            let rep = try #require(
                NSBitmapImageRep(
                    bitmapDataPlanes: nil, pixelsWide: side * 2, pixelsHigh: side * 2, bitsPerSample: 8,
                    samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB,
                    bytesPerRow: 0, bitsPerPixel: 0))
            rep.size = NSSize(width: side, height: side)
            NSGraphicsContext.saveGraphicsState()
            NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
            image.draw(
                in: NSRect(
                    x: (CGFloat(side) - image.size.width) / 2, y: (CGFloat(side) - image.size.height) / 2,
                    width: image.size.width, height: image.size.height))
            NSGraphicsContext.restoreGraphicsState()
            var xs: [Int] = []
            var ys: [Int] = []
            for y in 0..<rep.pixelsHigh {
                for x in 0..<rep.pixelsWide where (rep.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.5 {
                    xs.append(x)
                    ys.append(y)
                }
            }
            let (left, right) = (try #require(xs.min()), try #require(xs.max()))
            let (top, bottom) = (try #require(ys.min()), try #require(ys.max()))
            let width = CGFloat(right - left + 1) / 2
            let height = CGFloat(bottom - top + 1) / 2
            #expect(width <= 20 && height <= 20, "\(symbol): \(width) x \(height)")
            #expect(max(width, height) >= 13, "\(symbol): \(width) x \(height)")
            let middle = (CGFloat(right + left + 1) / 4, CGFloat(bottom + top + 1) / 4)
            #expect(abs(middle.0 - 14) <= 1.5 && abs(middle.1 - 14) <= 2, "\(symbol): centred at \(middle)")
        }
    }
}

// Option-click on a message marks the channel unread from it, as Alt+click in the official
// client: anywhere in the message's box but on its buttons, and nothing else happens.
@MainActor
struct OptionClickTests {
    let list = BarList()

    init() {
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "see https://example.com now"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
            row(3, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin),
            row(4, at: list.noon + 800, delivery: .failed),
        ])
        list.pointer.location = NSPoint(x: 10, y: 10)
    }

    func middle(of raw: UInt64) throws -> NSPoint {
        let box = try list.box(of: raw)
        return NSPoint(x: box.midX, y: box.midY)
    }

    @Test
    func anOptionClickInAMessagesBoxMarksItUnread() throws {
        #expect(list.controller.optionClick(try list.click(.option, at: try middle(of: 2))))
        #expect(list.controller.optionClick(try list.click(.option, at: try middle(of: 3))))

        #expect(list.actions.calls.filter { $0.hasPrefix("mark unread") } == ["mark unread 2", "mark unread 3"])
    }

    @Test
    func overALinkItMarksUnreadAndFollowsNothing() throws {
        let cell = try list.cell(of: 1)
        let text = try #require(MessageListFixture.messageText(in: cell))
        let glyphs = try glyphs(of: "example", in: text, within: text)
        let point = list.table.convert(NSPoint(x: glyphs.midX, y: glyphs.midY), from: text)

        #expect(list.controller.monitored(try list.click(.option, at: point)))

        #expect(list.actions.calls.filter { !$0.hasPrefix("bar") && !$0.hasPrefix("more") } == ["mark unread 1"])
        #expect(text.selectedRange().length == 0)
    }

    @Test
    func otherClicksAreLeftAlone() throws {
        let point = try middle(of: 2)

        #expect(!list.controller.optionClick(try list.click([], at: point)))
        #expect(!list.controller.optionClick(try list.click([.option, .shift], at: point)))
        #expect(!list.controller.optionClick(try list.click([.option, .command], at: point)))
        #expect(!list.controller.monitored(try list.click([], at: point)))
        let gap = NSPoint(x: 200, y: try list.box(of: 3).minY - 6)
        #expect(!list.controller.optionClick(try list.click(.option, at: gap)))
        #expect(!list.controller.optionClick(try list.click(.option, at: NSPoint(x: 200, y: list.table.rect(ofRow: 0).midY))))

        #expect(!list.actions.calls.contains { $0.hasPrefix("mark unread") })
    }

    @Test
    func aButtonKeepsItsClick() throws {
        func buttons(_ view: NSView) -> [NSButton] {
            ((view as? NSButton).map { [$0] } ?? []) + view.subviews.flatMap(buttons)
        }
        let retry = try #require(buttons(try list.cell(of: 4)).first { $0.title == "Retry" })
        let onRetry = list.table.convert(NSPoint(x: retry.bounds.midX, y: retry.bounds.midY), from: retry)
        #expect(!list.controller.optionClick(try list.click(.option, at: onRetry)))

        try list.hover(2)
        let more = try #require(list.bar.button(.more))
        let onMore = list.table.convert(NSPoint(x: 14, y: 14), from: more)
        #expect(!list.controller.optionClick(try list.click(.option, at: onMore)))
        let onBar = list.table.convert(NSPoint(x: 1, y: 1), from: list.bar)
        #expect(!list.controller.optionClick(try list.click(.option, at: onBar)))

        #expect(!list.actions.calls.contains { $0.hasPrefix("mark unread") })
    }

    // Behind another window, or while the window isn't key, the list takes no click.
    @Test
    func withoutThePointerNothingIsMarked() throws {
        list.pointer.location = nil

        #expect(!list.controller.optionClick(try list.click(.option, at: try middle(of: 2))))
    }
}
