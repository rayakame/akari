import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The browser in windows of its own over the session's window: a shield over the whole window
// that takes every mouse event outside the browser, the browser's panel over it, key while it
// shows. Nothing behind can react: the window server sends nothing to a window that's covered.
@MainActor
final class ThreadBrowserWindowsTests {
    // The session's window says it's key, so its composer takes the focus as the app's does.
    final class KeyWindow: NSWindow {
        var minimized = 0

        override var isKeyWindow: Bool { true }
        override var canBecomeKey: Bool { true }

        override func performMiniaturize(_ sender: Any?) {
            minimized += 1
        }
    }

    let store = FakeBrowserStore()
    let session: SessionModel
    let window: KeyWindow
    let host: NSHostingView<AnyView>
    var keyed: [NSWindow] = []

    init() {
        store.lines = true
        store.holdsLoads = true
        session = store.session()
        session.open(channel: FakeBrowserStore.parent)
        window = KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 813),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        // The window server reports an opening window's bounds scaled while it animates.
        window.animationBehavior = .none
        host = NSHostingView(rootView: AnyView(EmptyView()))
        host.rootView = AnyView(
            SessionScreen(session: session, splitWidth: SplitWidth())
                .environment(\.windowFocus, WindowFocus { [weak self] in self?.keyed.append($0) }))
        window.contentView = host
        window.orderFrontRegardless()
    }

    deinit {
        MainActor.assumeIsolated {
            for child in window.childWindows ?? [] {
                child.orderOut(nil)
            }
            window.orderOut(nil)
        }
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

    var shield: ShieldWindow? {
        window.childWindows?.compactMap { $0 as? ShieldWindow }.first
    }

    var panel: ContainerPanel? {
        shield?.childWindows?.compactMap { $0 as? ContainerPanel }.first
    }

    var composer: ComposerTextView? {
        views(ComposerTextView.self, in: host).first { $0.window != nil }
    }

    var searchField: NSTextField? {
        panel?.contentView.flatMap { views(NSTextField.self, in: $0).first { $0.isEditable } }
    }

    func ready() async throws -> ComposerTextView {
        try await waitUntil("the composer's focus") {
            composer != nil && window.firstResponder === composer
        }
        return try #require(composer)
    }

    func opened(_ presentation: ThreadBrowserModel.Presentation) async throws {
        _ = try await ready()
        session.openThreadBrowser(of: FakeBrowserStore.parent, as: presentation)
        try await waitUntil("the browser's panel") { panel?.isVisible == true && searchField != nil }
    }

    var closedCleanly: Bool {
        session.threadBrowser == nil && (window.childWindows ?? []).isEmpty
    }

    // Measured from the window's top left, as the screen tests measured the overlays.
    func fromTop(_ rect: NSRect) -> NSRect {
        NSRect(
            x: rect.minX - window.frame.minX, y: window.frame.maxY - rect.maxY,
            width: rect.width, height: rect.height)
    }

    func screenPoint(fromTop x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: window.frame.minX + x, y: window.frame.maxY - y)
    }

    // Of this test's windows, the one a click at `point` (screen coordinates) goes to: the
    // window server's front-to-back order, AppKit's frames (the window server's bounds can be
    // scaled, as Stage Manager shows an inactive app's windows). Other tests' windows may lie
    // anywhere.
    func topWindow(at point: NSPoint) -> NSWindow? {
        let ours = [window, shield, panel].compactMap { $0 }
        let order =
            (CGWindowListCopyWindowInfo([.optionOnScreenOnly], kCGNullWindowID)
                as? [[String: Any]] ?? [])
            .compactMap { $0[kCGWindowNumber as String] as? Int }
        return order.lazy.compactMap { number in ours.first { $0.windowNumber == number } }
            .first { $0.frame.contains(point) && !$0.ignoresMouseEvents }
    }

    func mouse(_ type: NSEvent.EventType, at point: NSPoint, in target: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: type, location: target.convertPoint(fromScreen: point), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: target.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
    }

    func key(_ code: UInt16, _ characters: String, in target: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: target.windowNumber, context: nil, characters: characters,
                charactersIgnoringModifiers: characters, isARepeat: false, keyCode: code))
    }

    // The popout hangs under the Threads button, its right edge on the button's (1084) and its
    // top 81 from the window's top, 480 wide; the shield covers the whole window.
    @Test
    func thePopoutIsAKeyPanelOverAShieldOverTheWindow() async throws {
        try await opened(.popout)
        let shield = try #require(shield)
        let panel = try #require(panel)

        #expect(shield.frame == window.frame)
        #expect(!shield.canBecomeKey && !shield.ignoresMouseEvents)
        #expect(panel.canBecomeKey && !panel.canBecomeMain)
        let frame = fromTop(panel.frame)
        #expect(abs(frame.maxX - 1084) <= 1 && abs(frame.minY - 81) <= 1, "\(frame)")
        #expect(frame.width == 480)
        #expect(keyed.last === panel)
    }

    // 680 wide and 701 tall in an 813 window: 56 above and below, the shield dimming the page.
    @Test
    func theModalIsCenteredOverItsDimmedShield() async throws {
        try await opened(.modal)
        let shield = try #require(shield)
        let frame = fromTop(try #require(panel).frame)

        #expect(abs(frame.minX - 210) <= 1 && abs(frame.minY - 56) <= 1, "\(frame)")
        #expect(frame.width == 680 && frame.height == 701, "\(frame)")
        let dim = try #require(shield.backgroundColor.usingColorSpace(.sRGB))
        #expect(dim.brightnessComponent == 0 && abs(dim.alphaComponent - 0.72) < 0.01)
    }

    // Over a channel row, a message, the composer and the header's button a click goes to the
    // shield; over the browser, to the browser.
    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func aClickAnywhereButTheBrowserGoesToTheShield(
        _ presentation: ThreadBrowserModel.Presentation
    ) async throws {
        try await opened(presentation)
        let shield = try #require(shield)
        let panel = try #require(panel)

        // The window server shows what AppKit ordered a moment later.
        try await Task.sleep(for: .milliseconds(300))
        for (name, point) in [
            ("channel row", screenPoint(fromTop: 150, 120)),
            ("server rail", screenPoint(fromTop: 36, 120)),
            ("Threads button", screenPoint(fromTop: 1068, 56)),
            ("composer", screenPoint(fromTop: 1050, 790)),
        ] {
            #expect(topWindow(at: point) === shield, "\(name)")
        }
        let inside = NSPoint(x: panel.frame.midX, y: panel.frame.midY)
        #expect(topWindow(at: inside) === panel)
    }

    // A press on the shield closes the browser and goes no further, whichever button.
    @Test(arguments: [NSEvent.EventType.leftMouseDown, .rightMouseDown, .otherMouseDown])
    func aPressOnTheShieldOnlyClosesTheBrowser(_ press: NSEvent.EventType) async throws {
        try await opened(.popout)
        let shield = try #require(shield)

        shield.sendEvent(try mouse(press, at: screenPoint(fromTop: 150, 120), in: shield))

        try await waitUntil("closed") { closedCleanly }
        #expect(session.messages?.channelId == FakeBrowserStore.parent)
    }

    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func escapeInTheBrowserClosesIt(_ presentation: ThreadBrowserModel.Presentation)
        async throws
    {
        try await opened(presentation)
        let panel = try #require(panel)

        panel.sendEvent(try key(53, "\u{1b}", in: panel))

        try await waitUntil("closed") { closedCleanly }
    }

    // While an input method composes, Escape is its own: it cancels what it composed.
    @Test
    func escapeWhileAnInputMethodComposesLeavesTheBrowserOpen() async throws {
        try await opened(.modal)
        let panel = try #require(panel)
        let field = try #require(searchField)
        panel.makeFirstResponder(field)
        let editor = try #require(panel.firstResponder as? NSTextView)
        editor.setMarkedText(
            "か", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        try #require(editor.hasMarkedText() && panel.firstResponder === editor)

        panel.sendEvent(try key(53, "\u{1b}", in: panel))
        try await Task.sleep(for: .milliseconds(100))

        #expect(session.threadBrowser != nil)
        #expect(panel.isVisible)
    }

    // The browser takes the key while it shows; closed, the window gets it back and the
    // channel's composer the focus, wherever it was before.
    @Test
    func closingGivesTheKeyBackAndTheComposerTheFocus() async throws {
        try await opened(.modal)
        let panel = try #require(panel)
        #expect(keyed.last === panel)
        window.makeFirstResponder(nil)

        session.closeThreadBrowser()

        try await waitUntil("closed") { closedCleanly }
        #expect(keyed.last === window)
        try await waitUntil("the composer's focus") { window.firstResponder === composer }
    }

    enum Closing: String, CaseIterable, Sendable {
        case escape, shieldPress, closeButton, row, otherChannel, home, windowClose
        case miniaturize, appDeactivates, fullScreen, commandW
    }

    func close(_ closing: Closing) async throws {
        let panel = try #require(panel)
        switch closing {
        case .escape:
            panel.sendEvent(try key(53, "\u{1b}", in: panel))
        case .shieldPress:
            let shield = try #require(shield)
            shield.sendEvent(try mouse(.leftMouseDown, at: screenPoint(fromTop: 20, 400), in: shield))
        case .closeButton:
            // The X: 16 in from the modal's right edge, centered in the header under its 8.
            let x = NSPoint(x: panel.frame.maxX - 1 - 16 - 16, y: panel.frame.maxY - 1 - 8 - 24)
            panel.sendEvent(try mouse(.leftMouseDown, at: x, in: panel))
            try await Task.sleep(for: .milliseconds(50))
            panel.sendEvent(try mouse(.leftMouseUp, at: x, in: panel))
        case .row:
            // The first card: 8 + 48 + 8 + 24 + 8 below the modal's top, 16 in.
            let card = NSPoint(x: panel.frame.minX + 100, y: panel.frame.maxY - 1 - 8 - 48 - 40 - 36)
            panel.sendEvent(try mouse(.leftMouseDown, at: card, in: panel))
            try await Task.sleep(for: .milliseconds(50))
            panel.sendEvent(try mouse(.leftMouseUp, at: card, in: panel))
        case .otherChannel:
            session.open(channel: ChannelId(rawValue: 1_001))
        case .home:
            session.open(.home)
        case .windowClose:
            window.close()
        case .miniaturize:
            NotificationCenter.default.post(name: NSWindow.willMiniaturizeNotification, object: window)
        case .appDeactivates:
            NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)
        case .fullScreen:
            NotificationCenter.default.post(name: NSWindow.willEnterFullScreenNotification, object: window)
        case .commandW:
            panel.performClose(nil)
        }
    }

    // However the browser closes, its windows go with it.
    @Test(arguments: Closing.allCases)
    func everyWayOfClosingLeavesNoWindowBehind(_ closing: Closing) async throws {
        try await opened(.modal)
        let shield = try #require(shield)
        let panel = try #require(panel)

        try await close(closing)

        try await waitUntil("closed") { closedCleanly }
        #expect(!shield.isVisible && !panel.isVisible, "\(closing)")
        #expect(shield.parent == nil && panel.parent == nil, "\(closing)")
    }

    // The menu's Close and Minimize go to the key window, the browser's panel: Close closes the
    // browser, Minimize the session's window.
    @Test
    func theWindowMenuActsOnTheBrowserAndItsWindow() async throws {
        try await opened(.popout)
        let panel = try #require(panel)
        let close = NSMenuItem(title: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")
        let minimize = NSMenuItem(
            title: "Minimize", action: #selector(NSWindow.performMiniaturize(_:)), keyEquivalent: "m")

        #expect(panel.validateUserInterfaceItem(close) && panel.validateUserInterfaceItem(minimize))
        panel.performMiniaturize(nil)
        #expect(window.minimized == 1)
    }

    // The window grows: the shield with it, the modal stays centered.
    @Test
    func theShieldAndTheModalFollowTheWindowsSize() async throws {
        try await opened(.modal)
        let shield = try #require(shield)

        window.setFrame(NSRect(x: 0, y: 0, width: 1300, height: 900), display: true)

        try await waitUntil("followed") {
            shield.frame == window.frame
                && abs(fromTop(panel?.frame ?? .zero).midX - 650) <= 1
        }
        #expect(shield.frame == window.frame)
    }

    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func voiceOverMeetsADialogNamedThreads(_ presentation: ThreadBrowserModel.Presentation)
        async throws
    {
        try await opened(presentation)
        let panel = try #require(panel)

        #expect(panel.accessibilitySubrole() == .dialog)
        #expect(panel.accessibilityLabel() == "Threads")
        #expect(try #require(shield).isAccessibilityElement() == false)
    }

    func line() async throws -> MessageTextView {
        var found: MessageTextView?
        try await waitUntil("the line") {
            found = views(MessageTextView.self, in: host).first {
                $0.string.contains("See all threads") && $0.window != nil
            }
            return found != nil
        }
        return try #require(found)
    }

    // The text view's own link handling: an inactive test app gives a first press through the
    // window to no AppKit view.
    func clickThreads() async throws {
        let text = try await line()
        let rect = try glyphs(of: "threads", in: text, within: text)
        let link = text.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
        let up = try #require(
            NSEvent.mouseEvent(
                with: .leftMouseUp, location: link, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime + 0.05,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 0))
        NSApp.postEvent(up, atStart: false)
        text.mouseDown(
            with: try #require(
                NSEvent.mouseEvent(
                    with: .leftMouseDown, location: link, modifierFlags: [],
                    timestamp: ProcessInfo.processInfo.systemUptime,
                    windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                    pressure: 1)))
    }

    var modalShown: Bool {
        session.threadBrowser?.presentation == .modal && panel?.isVisible == true
    }

    // Chris: after the modal closed once, "threads" didn't open it again until he switched
    // channels.
    @Test(arguments: [Closing.escape, .shieldPress, .closeButton])
    func threadsOpensTheModalAgainAfterItClosed(_ closing: Closing) async throws {
        _ = try await ready()
        try await clickThreads()
        try await waitUntil("opened") { modalShown }

        try await close(closing)
        try await waitUntil("closed") { closedCleanly }
        try await clickThreads()

        try await waitUntil("opened again") { modalShown }
        #expect(modalShown)
    }

    // A press inside a selection is the start of dragging it to NSTextView; a click on a link
    // follows it, selected or not. A double click on "threads" leaves it selected.
    @Test
    func aClickOnASelectedLinkFollowsIt() async throws {
        _ = try await ready()
        let text = try await line()
        text.setSelectedRange((text.string as NSString).range(of: "See all threads"))

        try await clickThreads()

        try await waitUntil("opened") { modalShown }
    }
}

