import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The browser over the whole screen, in a window that takes clicks and keys as the app's does:
// nothing behind Discord's popouts and modals reacts to the pointer or gets the keys.
@MainActor
struct ThreadBrowserInFrontTests {
    final class KeyWindow: NSWindow {
        override var isKeyWindow: Bool { true }
        override var canBecomeKey: Bool { true }
    }

    let store = FakeBrowserStore()
    let session: SessionModel
    let window: KeyWindow
    let host: NSHostingView<AnyView>

    init() {
        store.lines = true
        store.holdsLoads = true
        session = store.session()
        session.open(channel: FakeBrowserStore.parent)
        window = KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 813),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        host = NSHostingView(
            rootView: AnyView(
                SessionScreen(session: session, splitWidth: SplitWidth()).takingFirstClicks()))
        window.contentView = host
        window.orderFrontRegardless()
    }

    // SwiftUI applies a change over a few run loop turns; at most 5 s.
    func waitUntil(
        _ what: String = "", _ condition: () throws -> Bool,
        sourceLocation: SourceLocation = #_sourceLocation
    ) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            if try condition() {
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        Issue.record("not within 5 s: \(what)", sourceLocation: sourceLocation)
    }

    func views<View: NSView>(_ type: View.Type, in view: NSView) -> [View] {
        (view as? View).map { [$0] } ?? [] + view.subviews.flatMap { views(type, in: $0) }
    }

    func text(containing part: String) async throws -> MessageTextView {
        var found: MessageTextView?
        try await waitUntil {
            found = views(MessageTextView.self, in: host).first {
                $0.string.contains(part) && $0.window != nil
            }
            return found != nil
        }
        return try #require(found)
    }

    func line() async throws -> MessageTextView {
        try await text(containing: "See all threads")
    }

    // In the window's coordinates.
    func middle(of part: String, in text: NSTextView) throws -> NSPoint {
        let rect = try glyphs(of: part, in: text, within: text)
        return text.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    }

    func center(of view: NSView) -> NSPoint {
        view.convert(NSPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
    }

    // `point` measured from the window's top left.
    func fromTop(_ x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: x, y: host.frame.height - y)
    }

    func hit(_ point: NSPoint) -> NSView? {
        host.superview.flatMap { host.hitTest($0.convert(point, from: nil)) }
    }

    func mouse(_ type: NSEvent.EventType, at point: NSPoint) throws -> NSEvent {
        // The release comes a moment after the press.
        let later = type == .leftMouseUp || type == .rightMouseUp ? 0.05 : 0
        return try #require(
            NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime + later,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: type == .leftMouseUp ? 0 : 1))
    }

    // To the view the window hits, as the window would: a test's inactive app gives a first left
    // click to no AppKit view.
    func deliver(_ type: NSEvent.EventType, at point: NSPoint) throws {
        let target = try #require(hit(point))
        let event = try mouse(type, at: point)
        switch type {
        case .leftMouseDown: target.mouseDown(with: event)
        case .leftMouseUp: target.mouseUp(with: event)
        case .rightMouseDown: target.rightMouseDown(with: event)
        case .rightMouseUp: target.rightMouseUp(with: event)
        default: Issue.record("not a click: \(type)")
        }
    }

    // Through the window's hit testing, as SwiftUI takes a click.
    func click(at point: NSPoint) async throws {
        window.sendEvent(try mouse(.leftMouseDown, at: point))
        try await Task.sleep(for: .milliseconds(50))
        window.sendEvent(try mouse(.leftMouseUp, at: point))
    }

    func key(_ code: UInt16, _ characters: String, _ modifiers: NSEvent.ModifierFlags = [])
        throws -> NSEvent
    {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: modifiers, timestamp: 0,
                windowNumber: window.windowNumber, context: nil, characters: characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
    }

    var watcher: ThreadBrowserEvents.Watcher? {
        views(ThreadBrowserEvents.Watcher.self, in: host).first { $0.window != nil }
    }

    // What the app's monitor passes on reaches the window; the test's app has no key window.
    func type(_ code: UInt16, _ characters: String, _ modifiers: NSEvent.ModifierFlags = [])
        throws
    {
        let event = try key(code, characters, modifiers)
        if let passed = watcher.map({ $0.handle(event) }) ?? event {
            window.sendEvent(passed)
        }
    }

    // The search field: the screen's only editable text field.
    var searchField: NSTextField? {
        views(NSTextField.self, in: host).first { $0.isEditable && $0.window != nil }
    }

    var composer: ComposerTextView? {
        views(ComposerTextView.self, in: host).first { $0.window != nil }
    }

    var fieldHasTheFocus: Bool {
        guard let editor = window.firstResponder as? NSTextView, editor.isFieldEditor else {
            return false
        }
        return (editor.delegate as? NSTextField) === searchField
    }

    // The channel's lines and its composer, with the focus.
    func ready() async throws -> ComposerTextView {
        _ = try await line()
        try await waitUntil("the composer's focus") {
            composer != nil && window.firstResponder === composer
        }
        return try #require(composer)
    }

    func shown(_ presentation: ThreadBrowserModel.Presentation) async throws {
        _ = try await ready()
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: presentation)
        try await waitUntil("the field's focus") {
            searchField != nil && fieldHasTheFocus
        }
        try await Task.sleep(for: .milliseconds(200))
    }

    var modalShown: Bool { session.threadBrowser?.presentation == .modal && searchField != nil }

    // The text view's own link handling: a press through the window doesn't reach AppKit views
    // inside SwiftUI in a test's inactive app.
    func press(_ point: NSPoint, in text: MessageTextView) throws {
        NSApp.postEvent(try mouse(.leftMouseUp, at: point), atStart: false)
        text.mouseDown(with: try mouse(.leftMouseDown, at: point))
    }

    func clickThreads() async throws {
        let text = try await line()
        let link = try middle(of: "threads", in: text)
        #expect(hit(link) === text)
        try press(link, in: text)
    }

    // Behind the browser and beside it: a channel row, the server rail, the header's Threads
    // button, a message's row (left of the popout, right of the modal) and the composer.
    func behind(_ presentation: ThreadBrowserModel.Presentation = .popout) async throws
        -> [(String, NSPoint)]
    {
        let composer = try await ready()
        let hello = try await text(containing: "hello")
        let row = try middle(of: "hello", in: hello)
        let input = composer.convert(composer.bounds, to: nil)
        return [
            ("channel row", fromTop(150, 120)), ("server rail", fromTop(36, 120)),
            ("Threads button", fromTop(1068, 56)),
            ("message", NSPoint(x: presentation == .popout ? 560 : 1000, y: row.y)),
            ("composer", NSPoint(x: input.maxX - 20, y: input.midY)),
        ]
    }

    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func theShieldTakesThePointerEverywhereBehindTheBrowser(
        _ presentation: ThreadBrowserModel.Presentation
    ) async throws {
        let points = try await behind(presentation)
        try await shown(presentation)
        let shield = try #require(watcher)

        for (name, point) in points {
            #expect(hit(point) === shield, "\(name): \(String(describing: hit(point)))")
        }
        let field = try #require(searchField)
        let inFront = hit(center(of: field))
        #expect(inFront !== shield && inFront != nil, "\(String(describing: inFront))")
    }

    // Chris: a click outside the popout also opened what it hit. Discord's click trap takes it,
    // and the popout closes on its release, whichever button.
    @Test(arguments: [
        (NSEvent.EventType.leftMouseDown, NSEvent.EventType.leftMouseUp),
        (.rightMouseDown, .rightMouseUp),
    ])
    func aClickOutsideThePopoutOnlyClosesIt(
        _ press: NSEvent.EventType, _ release: NSEvent.EventType
    ) async throws {
        let points = try await behind()
        try await shown(.popout)
        let shield = try #require(watcher)
        let row = points[0].1
        #expect(hit(row) === shield)

        try deliver(press, at: row)
        try await Task.sleep(for: .milliseconds(50))
        #expect(session.threadBrowser != nil, "not on the press")
        try deliver(release, at: row)

        try await waitUntil { session.threadBrowser == nil }
        #expect(session.messages?.channelId == FakeBrowserStore.parent)
    }

    @Test
    func aClickOnTheDimmedPageOnlyClosesTheModal() async throws {
        let points = try await behind()
        try await shown(.modal)
        let shield = try #require(watcher)
        let row = points[0].1
        #expect(hit(row) === shield)

        try deliver(.leftMouseDown, at: row)
        try await Task.sleep(for: .milliseconds(50))
        #expect(session.threadBrowser != nil, "a modal closes on the click, not the press")
        try deliver(.leftMouseUp, at: row)

        try await waitUntil { session.threadBrowser == nil }
    }

    // Chris: Tab from the modal's field went to the composer behind it, and the letters after
    // it too. Discord's focus lock keeps Tab inside; the field is the only stop.
    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func tabStaysInsideTheBrowser(_ presentation: ThreadBrowserModel.Presentation) async throws {
        let composer = try await ready()
        try await shown(presentation)

        try type(48, "\t")
        try type(48, "\t", .shift)
        for letter in ["a", "b", "c"] {
            try type(0, letter)
        }

        #expect(fieldHasTheFocus)
        #expect(searchField?.stringValue == "abc")
        #expect(composer.string.isEmpty)
    }

    // However the focus would get there, nothing behind the browser takes it while it's open.
    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func theComposerTakesNoFocusWhileTheBrowserIsOpen(
        _ presentation: ThreadBrowserModel.Presentation
    ) async throws {
        let composer = try await ready()
        try await shown(presentation)

        composer.takeFocus()
        #expect(fieldHasTheFocus)
        window.makeFirstResponder(composer)
        #expect(window.firstResponder !== composer)
        #expect(!composer.acceptsFirstResponder)

        session.closeThreadBrowser()
        try await waitUntil { watcher == nil }
        #expect(composer.acceptsFirstResponder)
    }

    // Chris: the modal's field showed no I-beam. Its own area does, in front of the shield.
    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func theSearchFieldShowsTheIBeam(_ presentation: ThreadBrowserModel.Presentation)
        async throws
    {
        try await shown(presentation)
        let field = try #require(searchField)
        let middle = center(of: field)

        let areas = views(CursorArea.Area.self, in: host).filter {
            $0.window != nil && $0.cursor == .iBeam
        }
        let area = try #require(areas.first { $0.convert($0.bounds, to: nil).contains(middle) })
        #expect(area.shownRect == area.bounds)
    }

    enum Closing: CaseIterable, Sendable {
        case closeButton
        case escape
        case dimmedPage
    }

    enum Before: CaseIterable, Sendable {
        case nothing
        case clickIntoTheField
        case tabAndType
        case search
    }

    func close(_ closing: Closing) async throws {
        switch closing {
        case .closeButton:
            let modal = NSRect(x: 211, y: 56, width: 680, height: 701)
            // The X: 16 from the modal's right edge, in the header under its 8.
            try await click(at: fromTop(modal.maxX - 1 - 16 - 12, modal.minY + 1 + 8 + 24))
        case .escape:
            try type(53, "\u{1b}")
        case .dimmedPage:
            try deliver(.leftMouseDown, at: fromTop(100, 400))
            try deliver(.leftMouseUp, at: fromTop(100, 400))
        }
    }

    // Chris: after the modal closed once, "threads" didn't open it again until he switched
    // channels. Every way of closing it, after what TE2 does in it, with its page still loading.
    @Test(arguments: Closing.allCases, Before.allCases)
    func threadsOpensTheModalAgainAfterItClosed(_ closing: Closing, _ before: Before)
        async throws
    {
        try await clickThreads()
        try await waitUntil { modalShown }
        try await Task.sleep(for: .milliseconds(300))
        let field = try #require(searchField)
        switch before {
        case .nothing:
            break
        case .clickIntoTheField:
            try await click(at: center(of: field))
        case .tabAndType:
            try await click(at: center(of: field))
            try type(48, "\t")
            for letter in ["a", "b", "c"] {
                try type(0, letter)
            }
        case .search:
            for letter in ["t", "1"] {
                try type(17, letter)
            }
            try type(36, "\r")
        }
        try await Task.sleep(for: .milliseconds(200))

        try await close(closing)
        try await waitUntil { session.threadBrowser == nil && searchField == nil }
        try await Task.sleep(for: .milliseconds(200))
        try await clickThreads()

        try await waitUntil { modalShown }
        #expect(modalShown, "\(closing) \(before)")
    }

    // A press inside a selection is the start of dragging it to NSTextView; in Discord's client a
    // click on a link follows it, selected or not. A double click on "threads" leaves it selected.
    @Test
    func aClickOnASelectedLinkFollowsIt() async throws {
        let text = try await line()
        text.setSelectedRange((text.string as NSString).range(of: "See all threads"))

        try press(try middle(of: "threads", in: text), in: text)

        try await waitUntil { modalShown }
        #expect(modalShown)
    }
}
