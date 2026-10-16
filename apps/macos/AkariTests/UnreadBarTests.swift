import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct UnreadBarTests {
    final class Screen {
        var shows = true
    }

    let screen = Screen()
    let list: MessageTableTests
    let actions = MessageTableTests.RecordingActions()
    let line = MessageId(rawValue: 150)

    init() {
        list = MessageTableTests(onScreen: { [screen] _ in screen.shows })
        list.controller.actions = actions
    }

    var lineRow: Int? {
        list.controller.timeline.items.firstIndex {
            if case .newMessages = $0 { true } else { false }
        }
    }

    // How far the line's row starts below the top of what the list shows.
    func lineOffset() throws -> CGFloat {
        list.table.rect(ofRow: try #require(lineRow)).minY - list.clip.bounds.minY
    }

    func key(_ characters: String, keyCode: UInt16, modifiers: NSEvent.ModifierFlags) throws
        -> NSEvent
    {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                windowNumber: list.window.windowNumber, context: nil, characters: characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: keyCode))
    }

    @Test
    func theTableSaysWhetherTheLineIsOnScreen() {
        list.show(MessageTableState(rows: list.page, newMessages: line))
        #expect(actions.linesOnScreen.last == false, "above the view at the bottom")

        screen.shows = false
        let reported = actions.linesOnScreen.count
        list.scroll(toRowOf: 145)
        #expect(actions.linesOnScreen.count == reported, "nothing from a hidden window")

        screen.shows = true
        NotificationCenter.default.post(
            name: NSWindow.didChangeOcclusionStateNotification, object: list.window)
        #expect(actions.linesOnScreen.last == true)

        list.scroll(toRowOf: 151)
        #expect(actions.linesOnScreen.last == false, "just above the view")
    }

    @Test
    func theCachedLineRowFollowsTimelineChanges() {
        list.show(MessageTableState(rows: list.page, newMessages: line))
        #expect(list.controller.lineRow != nil)
        #expect(list.controller.lineRow == lineRow)

        // An older page above the line, then rows above it gone.
        list.show(
            MessageTableState(rows: list.messages(50, from: 51) + list.page, newMessages: line))
        #expect(list.controller.lineRow == lineRow)
        list.show(MessageTableState(rows: Array(list.page.dropFirst(30)), newMessages: line))
        #expect(list.controller.lineRow == lineRow)

        list.show(MessageTableState(rows: list.page, newMessages: MessageId(rawValue: 190)))
        #expect(list.controller.lineRow == lineRow)
        #expect(
            list.controller.lineRow.map { list.controller.timeline.items[$0 + 1].id }
                == .message(MessageId(rawValue: 190)))

        list.show(MessageTableState(rows: list.page))
        #expect(list.controller.lineRow == nil)
    }

    @Test
    func aJumpScrollsTheLineToTheTop() throws {
        list.show(MessageTableState(rows: list.page, newMessages: line))
        #expect(list.controller.isPinnedToBottom)

        list.show(MessageTableState(rows: list.page, newMessages: line, unreadJumps: 1))

        #expect(abs(try lineOffset()) <= 0.5)
        #expect(actions.bottoms.last == false)
        #expect(actions.linesOnScreen.last == true)

        // Where the reader is now, a message arriving below moves nothing.
        let arrived = list.page + [row(201, by: author(2), at: list.noon + 201 * 120)]
        list.show(MessageTableState(rows: arrived, newMessages: line, unreadJumps: 1))
        #expect(abs(try lineOffset()) <= 0.5)
    }

    // The first unread message wasn't loaded: the page around the read position comes with the
    // jump.
    @Test
    func aJumpWithThePageAroundTheReadPositionScrollsToItsLine() throws {
        list.show(MessageTableState(rows: list.page, newMessages: MessageId(rawValue: 101)))

        let around = list.messages(50, from: 21)
        list.show(
            MessageTableState(
                rows: around, atPresent: false, newMessages: MessageId(rawValue: 46),
                unreadJumps: 1))

        #expect(list.controller.timeline.items[try #require(lineRow) + 1].id
                == .message(MessageId(rawValue: 46)))
        #expect(abs(try lineOffset()) <= 0.5)
    }

    @Test
    func shiftPageUpJumpsFromTheListAndFromAMessagesText() throws {
        list.show(MessageTableState(rows: list.page, newMessages: line))
        let pageUp = "\u{F72C}"
        list.window.makeFirstResponder(list.table)

        list.table.keyDown(with: try key(pageUp, keyCode: 116, modifiers: [.function]))
        #expect(!actions.calls.contains("unread"), "Page Up alone doesn't jump")

        list.table.keyDown(with: try key(pageUp, keyCode: 116, modifiers: [.shift, .function]))
        #expect(actions.calls.filter { !$0.hasPrefix("more") } == ["unread"])

        let text = try #require(list.messageText(in: try list.cell(of: 200)))
        list.window.makeFirstResponder(text)
        text.setSelectedRange(NSRange(location: 0, length: 3))
        text.keyDown(with: try key(pageUp, keyCode: 116, modifiers: [.shift, .function]))
        #expect(actions.calls.filter { !$0.hasPrefix("more") } == ["unread", "unread"])
        #expect(text.selectedRange() == NSRange(location: 0, length: 3))
    }
}

@MainActor
struct UnreadBarScreenTests {
    let screens = SessionScreenTests()
    let drawing = MessageTextDrawingTests()
    // The first channel's messages 1...40, read up to 5: the line stands before 6.
    let account = FormattedAccount(contents: ["test"], readUpTo: 5)
    let channel = FormattedStore.channelIds[0]

    // A point this far right of the list's left edge and down from its top, in the window.
    func point(_ x: CGFloat, _ y: CGFloat, in table: MessageTableView) throws -> NSPoint {
        let scroll = try #require(table.enclosingScrollView)
        let frame = scroll.convert(scroll.bounds, to: nil)
        return NSPoint(x: frame.minX + x, y: frame.maxY - y)
    }

    // The host fills the window's content, so its pixels count rows from the window's top.
    func pixel(_ host: NSView, at point: NSPoint) throws -> NSColor {
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let y = host.bounds.height - point.y
        return try #require(
            rep.colorAt(x: Int(point.x * scale), y: Int(y * scale))?.usingColorSpace(.sRGB))
    }

    // Opens the channel, at its bottom, and waits for the bar over the line above.
    func shownWithItsBar(_ session: SessionModel, in host: NSView) async throws
        -> MessageTableView
    {
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        // A locked screen hides every window, and the table then says nothing.
        if let window = host.window, !MessageTableController.isOnScreen(window) {
            session.messages?.setNewMessagesLineOnScreen(false)
        }
        let inside = try point(16 + 4, 4 + 16, in: table)
        try await screens.waitUntil(host) {
            try drawing.matches(try pixel(host, at: inside), Palette.brand)
        }
        return table
    }

    func lineOffset(in table: MessageTableView) throws -> CGFloat {
        let controller = try #require(table.delegate as? MessageTableController)
        let row = try #require(
            controller.timeline.items.firstIndex {
                if case .newMessages = $0 { true } else { false }
            })
        let clip = try #require(table.enclosingScrollView).contentView
        return table.rect(ofRow: row).minY - clip.bounds.minY
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theBarDrawsOverTheListsTopInThePalettesBlue(appearance: NSAppearance.Name) async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        window.appearance = NSAppearance(named: appearance)
        defer { window.orderOut(nil) }
        // Inside the bar's left padding, at its middle; below it the messages show.
        let table = try await shownWithItsBar(session, in: host)

        #expect(session.messages?.unreadBar?.count == 35)
        let below = try point(16 + 4, 4 + 32 + 2, in: table)
        #expect(try !drawing.matches(try pixel(host, at: below), Palette.brand))
    }

    @Test
    func clickingTheBarScrollsTheLineToTheTop() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await shownWithItsBar(session, in: host)
        #expect(try lineOffset(in: table) < 0, "the line is above the view")
        let click = try point(120, 4 + 16, in: table)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type, location: click, modifierFlags: [], timestamp: 0,
                        windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                        clickCount: 1, pressure: 1)))
        }

        try await screens.waitUntil(host) { abs(try lineOffset(in: table)) <= 0.5 }
        #expect(session.messages?.unreadBar == nil)
        #expect(session.messages?.unreadJumps == 1)
    }
}
