import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// A Control-click asks only the view under the pointer for its menu (NSWindow) and a right-click
// asks it first (NSView), so every part of a message's box answers with the message's menu;
// `menu(_:at:)` asks as they do.
@MainActor
struct MessageMenuAreaTests {
    let list = MessageTableTests()
    let actions = MessageTableTests.RecordingActions()

    enum Click: CaseIterable {
        case right, control
    }

    init() {
        list.controller.actions = actions
    }

    static func event(_ click: Click, at point: NSPoint, in window: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: click == .right ? .rightMouseDown : .leftMouseDown, location: point,
                modifierFlags: click == .control ? [.control] : [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
    }

    /// The menu AppKit gets for a click at `point`, in window coordinates.
    static func menu(_ click: Click, at point: NSPoint, in window: NSWindow) throws -> NSMenu? {
        let content = try #require(window.contentView)
        let hit = content.hitTest(content.superview?.convert(point, from: nil) ?? point)
        return try hit?.menu(for: event(click, at: point, in: window))
    }

    func menu(_ click: Click, at point: NSPoint) throws -> NSMenu? {
        try Self.menu(click, at: point, in: list.window)
    }

    static func views<View: NSView>(_ type: View.Type, in view: NSView) -> [View] {
        view.subviews.flatMap { ([$0 as? View].compactMap { $0 }) + views(type, in: $0) }
    }

    func messageCell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    /// Points 4 apart across `rect` (in `view`), edges included, in window coordinates.
    static func grid(_ rect: NSRect, in view: NSView) -> [NSPoint] {
        let inset = rect.insetBy(dx: 0.5, dy: 0.5)
        let xs = Array(stride(from: inset.minX, to: inset.maxX, by: 4)) + [inset.maxX]
        let ys = Array(stride(from: inset.minY, to: inset.maxY, by: 4)) + [inset.maxY]
        return xs.flatMap { x in ys.map { y in view.convert(NSPoint(x: x, y: y), to: nil) } }
    }

    // "Create Thread" names the message whose menu it is.
    func owner(of menu: NSMenu?) throws -> UInt64? {
        let menu = try #require(menu)
        let index = try #require(menu.items.firstIndex { $0.title == "Create Thread" })
        actions.calls = []
        menu.performActionForItem(at: index)
        let call = try #require(actions.calls.first { $0.hasPrefix("create thread from ") })
        return UInt64(call.dropFirst("create thread from ".count))
    }

    // Ray's group of three (the last one wraps), then Mira's.
    func showGroups() {
        actions.startsThreads = [1, 2, 3, 4]
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
            row(
                3, by: author(2, "Ray"), at: list.noon + 20,
                content: String(repeating: "words that wrap ", count: 12)),
            row(4, by: author(3), at: list.noon + 600, content: "short"),
        ])
    }

    @Test(arguments: Click.allCases)
    func everyPointOfAMessagesBoxOpensItsMenu(click: Click) throws {
        showGroups()
        // A follow-up's time shows while it's hovered.
        let hovered = try messageCell(2)
        hovered.isHovered = true
        for raw: UInt64 in [1, 2, 3, 4] {
            let cell = try messageCell(raw)
            let points = Self.grid(cell.box, in: cell)
            #expect(points.count > 100)
            for point in points {
                let owner = try owner(of: try menu(click, at: point))
                #expect(owner == raw, "message \(raw) at \(point)")
            }
        }
    }

    // As hover: the space before a group and the scrollbar's gutter belong to no message.
    @Test(arguments: Click.allCases)
    func outsideTheBoxNoMenuOpens(click: Click) throws {
        showGroups()
        for raw: UInt64 in [1, 4] {
            let cell = try messageCell(raw)
            let box = cell.box
            let above =
                cell.isFlipped
                ? NSRect(x: 0, y: 0, width: cell.bounds.width, height: box.minY)
                : NSRect(
                    x: 0, y: box.maxY, width: cell.bounds.width,
                    height: cell.bounds.height - box.maxY)
            #expect(above.height >= 6)
            for point in Self.grid(above.insetBy(dx: 0, dy: 0.5), in: cell) {
                #expect(try menu(click, at: point) == nil, "above message \(raw) at \(point)")
            }
            let gutter = NSRect(
                x: box.maxX, y: box.minY, width: cell.bounds.width - box.maxX, height: box.height)
            if gutter.width >= 2 {
                for point in Self.grid(gutter.insetBy(dx: 0.5, dy: 0), in: cell) {
                    #expect(try menu(click, at: point) == nil, "beside message \(raw) at \(point)")
                }
            }
        }
    }

    // Nothing to copy and no thread to start: no menu at all, not an empty one.
    @Test(arguments: Click.allCases)
    func aMessageWithNothingToOfferOpensNoMenu(click: Click) throws {
        list.show([row(1, at: list.noon, content: "", attachments: ["cat.png"])])
        let cell = try messageCell(1)
        for point in Self.grid(cell.box, in: cell) {
            #expect(try menu(click, at: point) == nil, "at \(point)")
        }
    }

    // A loaded starter draws its parent's message: its menu copies that message's text.
    @Test(arguments: Click.allCases)
    func aThreadsStarterMessageOpensTheOriginalsMenu(click: Click) throws {
        let original = row(5, by: author(2, "Ray"), at: list.noon, content: "the original").message
        let first = row(
            31, by: author(2, "Ray"), at: list.noon, kind: .threadStarterMessage, content: "")
        let starter = MessageListModel.Row(
            id: first.id, message: first.message, starter: .loaded(original))
        list.show([starter])
        let cell = try messageCell(31)
        for point in Self.grid(cell.box, in: cell) {
            #expect(try menu(click, at: point)?.items.map(\.title) == ["Copy Text"], "at \(point)")
        }
    }

    // Links have no menu of their own: a link's is its message's.
    @Test(arguments: Click.allCases)
    func aLinkOpensItsMessagesMenu(click: Click) throws {
        actions.startsThreads = [1]
        list.show([row(1, at: list.noon, content: "see https://example.com now")])
        let text = try #require(list.messageTexts(in: try messageCell(1)).first)
        let link = try glyphs(of: "example", in: text, within: text)
        let point = text.convert(NSPoint(x: link.midX, y: link.midY), to: nil)

        let menu = try menu(click, at: point)
        #expect(menu?.items.map(\.title) == ["Create Thread", "", "Copy Text"])
        #expect(try owner(of: menu) == 1)
    }

    // The box and Retry are buttons, yet part of their message.
    @Test(arguments: Click.allCases)
    func theThreadBoxAndAFailedMessagesButtonsOpenTheMessagesMenu(click: Click) throws {
        actions.startsThreads = [3, 4]
        list.show([
            row(
                3, by: author(2, "Ray"), at: list.noon, content: "test", hasThread: true,
                threadBox: ThreadBox(
                    threadId: ChannelId(rawValue: 20), name: "t1", link: "16 Messages ›",
                    bottom: .noRecentMessages, time: list.noon)),
            row(4, at: list.noon + 1800, delivery: .failed),
        ])
        let box = try #require(ThreadBoxViewTests.boxView(in: try messageCell(3)))
        let frame = box.boxFrame
        for point in [
            NSPoint(x: frame.minX + 2, y: frame.midY), NSPoint(x: frame.midX, y: frame.midY),
            NSPoint(x: frame.maxX - 2, y: frame.minY + 2),
        ] {
            #expect(try owner(of: try menu(click, at: box.convert(point, to: nil))) == 3)
        }
        let retry = try #require(
            Self.views(NSButton.self, in: try messageCell(4)).first { $0.title == "Retry" })
        let onRetry = retry.convert(NSPoint(x: retry.bounds.midX, y: retry.bounds.midY), to: nil)
        #expect(try owner(of: try menu(click, at: onRetry)) == 4)
    }

    // A system line's menu copies its text where it has some, as Discord's.
    @Test(arguments: Click.allCases)
    func aNoticeOpensItsMenuWhereItHasOne(click: Click) throws {
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, kind: .threadCreated, content: "t2"),
            row(
                2, by: author(2, "Ray"), at: list.noon + 600, kind: .channelPinnedMessage,
                content: ""),
        ])
        let started = try #require(try list.cell(of: 1) as? NoticeCell)
        let startedPoints = Self.grid(started.box, in: started)
        #expect(startedPoints.count > 100)
        for point in startedPoints {
            #expect(try menu(click, at: point)?.items.map(\.title) == ["Copy Text"], "at \(point)")
        }
        let above =
            started.isFlipped
            ? NSRect(x: 0, y: 0, width: started.bounds.width, height: started.box.minY)
            : NSRect(
                x: 0, y: started.box.maxY, width: started.bounds.width,
                height: started.bounds.height - started.box.maxY)
        #expect(above.height >= 6)
        for point in Self.grid(above.insetBy(dx: 0, dy: 0.5), in: started) {
            #expect(try menu(click, at: point) == nil, "above at \(point)")
        }
        let pinned = try #require(try list.cell(of: 2) as? NoticeCell)
        for point in Self.grid(pinned.box, in: pinned) {
            #expect(try menu(click, at: point) == nil, "pinned at \(point)")
        }
    }

    // The list's one cursor authority is unchanged: the arrow over the header and the avatar.
    @Test
    func theCursorStaysTheLists() throws {
        showGroups()
        let cell = try messageCell(1)
        let avatar = try #require(cell.avatarFrame)
        let onAvatar = NSPoint(x: avatar.midX, y: avatar.midY)
        let onAvatarInCell =
            cell.isFlipped ? onAvatar : NSPoint(x: onAvatar.x, y: cell.bounds.height - onAvatar.y)
        #expect(list.table.cursor(at: cell.convert(onAvatarInCell, to: nil)) == .arrow)
    }

    // AppKit's own path over each kind of part: a plain view (the avatar), a label (the name),
    // the text, the row itself (beside the text, between grouped lines), a notice's icon and
    // blank space, the thread box and a button. A Control-click can't take it here: a test's
    // app is never active, so its window offers a first press to the view's acceptsFirstMouse
    // and drops it. An active app's window asks the view under the pointer for its menu, as the
    // tests above do. AppKit pops the real menu onto a screen: `.onScreen`.
    @Test(.tags(.onScreen), .enabled(if: TestWindow.screenTests))
    func aRightClickInTheWindowShowsTheMenu() throws {
        actions.startsThreads = [1, 2, 4, 5]
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "second"),
            row(3, by: author(2, "Ray"), at: list.noon + 600, kind: .threadCreated, content: "t2"),
            row(
                4, by: author(2, "Ray"), at: list.noon + 1200, content: "test", hasThread: true,
                threadBox: ThreadBox(
                    threadId: ChannelId(rawValue: 20), name: "t1", link: "16 Messages ›",
                    bottom: .noRecentMessages, time: list.noon)),
            row(5, at: list.noon + 1800, delivery: .failed),
        ])
        let window = list.keyWindow()
        defer { window.close() }
        let cell = try messageCell(1)
        let avatar = try #require(cell.avatarFrame)
        let top = cell.isFlipped ? cell.box.minY : cell.box.maxY
        let down: CGFloat = cell.isFlipped ? 1 : -1
        let second = try messageCell(2)
        let notice = try #require(try list.cell(of: 3) as? NoticeCell)
        let icon = try #require(notice.iconFrame)
        let iconY = notice.isFlipped ? icon.midY : notice.bounds.height - icon.midY
        let box = try #require(ThreadBoxViewTests.boxView(in: try messageCell(4)))
        let retry = try #require(
            Self.views(NSButton.self, in: try messageCell(5)).first { $0.title == "Retry" })
        let message = ["Create Thread", "", "Copy Text"]
        let points: [(NSView, NSPoint, [String])] = [
            (cell, NSPoint(x: avatar.midX, y: top + down * (avatar.height / 2 + 4)), message),
            (cell, NSPoint(x: 80, y: top + down * 10), message),
            (
                cell, NSPoint(x: 74, y: cell.isFlipped ? cell.box.maxY - 10 : cell.box.minY + 10),
                message
            ),
            (cell, NSPoint(x: cell.box.maxX - 4, y: cell.box.midY), message),
            (second, NSPoint(x: 200, y: second.isFlipped ? 1 : second.bounds.height - 1), message),
            (notice, NSPoint(x: icon.midX, y: iconY), ["Copy Text"]),
            (notice, NSPoint(x: notice.box.maxX - 4, y: notice.box.midY), ["Copy Text"]),
            (box, NSPoint(x: box.boxFrame.midX, y: box.boxFrame.midY), message),
            (retry, NSPoint(x: retry.bounds.midX, y: retry.bounds.midY), message),
        ]
        var shown: [[String]] = []
        let observer = NotificationCenter.default.addObserver(
            forName: NSMenu.didBeginTrackingNotification, object: nil, queue: nil
        ) { notification in
            nonisolated(unsafe) let menu = notification.object as? NSMenu
            MainActor.assumeIsolated {
                shown.append(menu?.items.map(\.title) ?? [])
                RunLoop.main.perform(inModes: [.eventTracking, .default]) {
                    MainActor.assumeIsolated { menu?.cancelTracking() }
                }
            }
        }
        defer { NotificationCenter.default.removeObserver(observer) }

        for (view, point, items) in points {
            shown = []
            window.sendEvent(try Self.event(.right, at: view.convert(point, to: nil), in: window))
            #expect(shown == [items], "at \(point) in \(type(of: view))")
        }
    }
}

// The split's and the full view's lists are the channel's: the same rows, the same menus.
extension SessionScreenTests {
    @Test(arguments: MessageMenuAreaTests.Click.allCases)
    func everyPaneOpensAMessagesMenuOverItsHeaderAndAvatar(click: MessageMenuAreaTests.Click)
        async throws
    {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        try await waitUntil(host) {
            tables(in: host).count == 2
                && tables(in: host).allSatisfy { table in
                    groupStarts(in: table).contains { !$0.box.isEmpty }
                }
        }
        try expectHeaderMenus(in: tables(in: host), window: window, click: click)

        session.closeSplit()
        session.go(to: FormattedStore.threadId)
        try await waitUntil(host) {
            tables(in: host).count == 1
                && (tables(in: host).first?.delegate as? MessageTableController)?.state.channelId
                    == FormattedStore.threadId
                && tables(in: host).allSatisfy { !groupStarts(in: $0).isEmpty }
        }
        try expectHeaderMenus(in: tables(in: host), window: window, click: click)
    }

    func groupStarts(in table: NSTableView) -> [MessageCell] {
        let rows = table.rows(in: table.visibleRect)
        return (rows.location..<(rows.location + rows.length)).compactMap {
            table.view(atColumn: 0, row: $0, makeIfNecessary: false) as? MessageCell
        }
        .filter(\.showsHeader)
    }

    func expectHeaderMenus(
        in tables: [MessageTableView], window: NSWindow, click: MessageMenuAreaTests.Click
    ) throws {
        for table in tables {
            let cell = try #require(groupStarts(in: table).first)
            let avatar = try #require(cell.avatarFrame)
            let top = cell.isFlipped ? cell.box.minY : cell.box.maxY
            let down: CGFloat = cell.isFlipped ? 1 : -1
            let avatarCenter = NSPoint(x: avatar.midX, y: top + down * (avatar.height / 2 + 4))
            let header = NSPoint(x: 80, y: top + down * 10)
            let blank = NSPoint(x: cell.box.maxX - 4, y: cell.box.midY)
            for point in [avatarCenter, header, blank] {
                let menu = try MessageMenuAreaTests.menu(
                    click, at: cell.convert(point, to: nil), in: window)
                #expect(menu?.items.map(\.title).contains("Copy Text") == true, "at \(point)")
            }
        }
    }
}
