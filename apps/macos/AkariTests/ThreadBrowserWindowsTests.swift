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
    final class KeyWindow: OutOfSightWindow {
        var minimized = 0
        var fullScreens = 0

        override var isKeyWindow: Bool { true }
        override var canBecomeKey: Bool { true }

        override func performMiniaturize(_ sender: Any?) {
            minimized += 1
        }

        override func toggleFullScreen(_ sender: Any?) {
            fullScreens += 1
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
        window = TestWindow.hide(
            KeyWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: 1100, height: 813)), styleMask: [.titled, .fullSizeContentView],
                backing: .buffered, defer: false))
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

    // A test's instance can go away off the main thread.
    deinit {
        nonisolated(unsafe) let window = window
        DispatchQueue.main.async {
            MainActor.assumeIsolated {
                for child in window.childWindows ?? [] {
                    child.orderOut(nil)
                }
                window.orderOut(nil)
            }
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
        let cover = try #require(self.shield)
        let browser = try #require(self.panel)

        #expect(cover.frame == window.frame)
        #expect(!cover.canBecomeKey && !cover.ignoresMouseEvents)
        #expect(browser.canBecomeKey && !browser.canBecomeMain)
        let frame = fromTop(browser.frame)
        #expect(abs(frame.maxX - 1084) <= 1 && abs(frame.minY - 81) <= 1, "\(frame)")
        #expect(frame.width == 480)
        #expect(keyed.last === browser)
    }

    // 680 wide and 701 tall in an 813 window: 56 above and below, the shield dimming the page.
    @Test
    func theModalIsCenteredOverItsDimmedShield() async throws {
        try await opened(.modal)
        let cover = try #require(self.shield)
        let frame = fromTop(try #require(panel).frame)

        #expect(abs(frame.minX - 210) <= 1 && abs(frame.minY - 56) <= 1, "\(frame)")
        #expect(frame.width == 680 && frame.height == 701, "\(frame)")
        let dim = try #require(cover.backgroundColor.usingColorSpace(.sRGB))
        #expect(dim.brightnessComponent == 0 && abs(dim.alphaComponent - 0.72) < 0.01)
    }

    // Over a channel row, a message, the composer and the header's button a click goes to the
    // shield; over the browser, to the browser.
    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func aClickAnywhereButTheBrowserGoesToTheShield(
        _ presentation: ThreadBrowserModel.Presentation
    ) async throws {
        try await opened(presentation)
        let cover = try #require(self.shield)
        let browser = try #require(self.panel)

        // The window server shows what AppKit ordered a moment later.
        try await Task.sleep(for: .milliseconds(300))
        for (name, point) in [
            ("channel row", screenPoint(fromTop: 150, 120)),
            ("server rail", screenPoint(fromTop: 36, 120)),
            ("Threads button", screenPoint(fromTop: 1068, 56)),
            ("composer", screenPoint(fromTop: 1050, 790)),
        ] {
            #expect(topWindow(at: point) === cover, "\(name)")
        }
        let inside = NSPoint(x: browser.frame.midX, y: browser.frame.midY)
        #expect(topWindow(at: inside) === browser)
    }

    // A press on the shield closes the browser and goes no further, whichever button.
    @Test(arguments: [NSEvent.EventType.leftMouseDown, .rightMouseDown, .otherMouseDown])
    func aPressOnTheShieldOnlyClosesTheBrowser(_ press: NSEvent.EventType) async throws {
        try await opened(.popout)
        let cover = try #require(self.shield)

        cover.sendEvent(try mouse(press, at: screenPoint(fromTop: 150, 120), in: cover))

        try await waitUntil("closed") { closedCleanly }
        #expect(session.messages?.channelId == FakeBrowserStore.parent)
    }

    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func escapeInTheBrowserClosesIt(_ presentation: ThreadBrowserModel.Presentation)
        async throws
    {
        try await opened(presentation)
        let browser = try #require(self.panel)

        browser.sendEvent(try key(53, "\u{1b}", in: browser))

        try await waitUntil("closed") { closedCleanly }
    }

    // While an input method composes, Escape is its own: it cancels what it composed.
    @Test
    func escapeWhileAnInputMethodComposesLeavesTheBrowserOpen() async throws {
        try await opened(.modal)
        let browser = try #require(self.panel)
        let field = try #require(searchField)
        browser.makeFirstResponder(field)
        let editor = try #require(browser.firstResponder as? NSTextView)
        editor.setMarkedText(
            "か", selectedRange: NSRange(location: 1, length: 0),
            replacementRange: NSRange(location: NSNotFound, length: 0))
        try #require(editor.hasMarkedText() && browser.firstResponder === editor)

        browser.sendEvent(try key(53, "\u{1b}", in: browser))
        try await Task.sleep(for: .milliseconds(100))

        #expect(session.threadBrowser != nil)
        #expect(browser.isVisible)
    }

    // The browser takes the key while it shows; closed, the window gets it back and the
    // channel's composer the focus, wherever it was before.
    @Test
    func closingGivesTheKeyBackAndTheComposerTheFocus() async throws {
        try await opened(.modal)
        let browser = try #require(self.panel)
        #expect(keyed.last === browser)
        window.makeFirstResponder(nil)

        session.closeThreadBrowser()

        try await waitUntil("closed") { closedCleanly }
        #expect(keyed.last === window)
        try await waitUntil("the composer's focus") { window.firstResponder === composer }
    }

    enum Closing: String, CaseIterable, Sendable {
        case escape, shieldPress, closeButton, row, otherChannel, home, windowClose
        case miniaturize, appDeactivates, fullScreen, leavingFullScreen, commandW
    }

    func close(_ closing: Closing) async throws {
        let browser = try #require(self.panel)
        switch closing {
        case .escape:
            browser.sendEvent(try key(53, "\u{1b}", in: browser))
        case .shieldPress:
            let cover = try #require(self.shield)
            cover.sendEvent(try mouse(.leftMouseDown, at: screenPoint(fromTop: 20, 400), in: cover))
        case .closeButton:
            // The X: 16 in from the modal's right edge, centered in the header under its 8.
            let x = NSPoint(x: browser.frame.maxX - 1 - 16 - 16, y: browser.frame.maxY - 1 - 8 - 24)
            browser.sendEvent(try mouse(.leftMouseDown, at: x, in: browser))
            try await Task.sleep(for: .milliseconds(50))
            browser.sendEvent(try mouse(.leftMouseUp, at: x, in: browser))
        case .row:
            // The first card: 8 + 48 + 8 + 24 + 8 below the modal's top, 16 in.
            let card = NSPoint(x: browser.frame.minX + 100, y: browser.frame.maxY - 1 - 8 - 48 - 40 - 36)
            browser.sendEvent(try mouse(.leftMouseDown, at: card, in: browser))
            try await Task.sleep(for: .milliseconds(50))
            browser.sendEvent(try mouse(.leftMouseUp, at: card, in: browser))
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
        case .leavingFullScreen:
            NotificationCenter.default.post(name: NSWindow.willExitFullScreenNotification, object: window)
        case .commandW:
            browser.performClose(nil)
        }
    }

    // However the browser closes, its windows go with it.
    @Test(arguments: Closing.allCases)
    func everyWayOfClosingLeavesNoWindowBehind(_ closing: Closing) async throws {
        try await opened(.modal)
        let cover = try #require(self.shield)
        let browser = try #require(self.panel)

        try await close(closing)

        try await waitUntil("closed") { closedCleanly }
        #expect(!cover.isVisible && !browser.isVisible, "\(closing)")
        #expect(cover.parent == nil && browser.parent == nil, "\(closing)")
    }

    // The menu's Close goes to the key window, the browser's panel: it closes the browser.
    @Test
    func theWindowMenusCloseClosesTheBrowser() async throws {
        try await opened(.popout)
        let browser = try #require(self.panel)
        let close = NSMenuItem(title: "Close", action: #selector(NSWindow.performClose(_:)), keyEquivalent: "w")

        #expect(browser.validateUserInterfaceItem(close))
        browser.performClose(nil)

        try await waitUntil("closed") { closedCleanly }
    }

    // With the browser's panel key, the Window menu's commands find it first: Fill, Center,
    // Move & Resize, Zoom, Minimize and full screen go on to the session's window, with the
    // browser open (it follows the window, and full screen closes it).
    @Test
    func theWindowsCommandsActOnTheSessionWindow() async throws {
        try await opened(.popout)
        let browser = try #require(self.panel)
        let commands =
            [
                #selector(NSWindow.performZoom(_:)), #selector(NSWindow.zoom(_:)),
                #selector(NSWindow.toggleFullScreen(_:)), #selector(NSWindow.performMiniaturize(_:)),
            ]
            + ["_zoomFill:", "_zoomCenter:", "_zoomLeft:", "_zoomTopRight:", "_zoomUntile:", "_tileLeft:"]
            .map(NSSelectorFromString)

        for command in commands {
            #expect(!browser.responds(to: command), "\(command)")
            #expect(
                browser.supplementalTarget(forAction: command, sender: nil) as? NSWindow === window,
                "\(command)")
        }
        #expect(browser.responds(to: #selector(NSWindow.performClose(_:))))
        #expect(browser.supplementalTarget(forAction: #selector(NSText.copy(_:)), sender: nil) == nil)
    }

    // ⌃⌘F, as on any Mac app, from the View menu; the browser's panel hands it to its window.
    @Test
    func theViewMenuEntersFullScreenWithControlCommandF() async throws {
        let items = (NSApp.mainMenu?.items ?? []).compactMap(\.submenu).flatMap(\.items)
        let item = try #require(items.first { $0.title == "Enter Full Screen" })
        #expect(item.keyEquivalent == "f")
        #expect(item.keyEquivalentModifierMask == [.control, .command])

        try await opened(.popout)
        FullScreen.toggle(from: try #require(self.panel))
        #expect(window.fullScreens == 1)
        FullScreen.toggle(from: window)
        #expect(window.fullScreens == 2)
    }

    // Its own center: AppKit's observers would take the test's window for one in full screen.
    @Test
    func theMenusTitleSaysWhatItWillDo() async {
        let center = NotificationCenter()
        let state = FullScreen.State(center: center)
        #expect(state.title == "Enter Full Screen")

        center.post(name: NSWindow.didEnterFullScreenNotification, object: window)
        #expect(state.title == "Exit Full Screen")
        center.post(name: NSWindow.didExitFullScreenNotification, object: window)
        #expect(state.title == "Enter Full Screen")
    }

    // The window grows: the shield with it, the modal stays centered.
    @Test
    func theShieldAndTheModalFollowTheWindowsSize() async throws {
        try await opened(.modal)
        let cover = try #require(self.shield)

        window.setFrame(NSRect(origin: window.frame.origin, size: NSSize(width: 1300, height: 900)), display: true)

        try await waitUntil("followed") {
            cover.frame == window.frame
                && abs(fromTop(panel?.frame ?? .zero).midX - 650) <= 1
        }
        #expect(cover.frame == window.frame)
    }

    @Test(arguments: [ThreadBrowserModel.Presentation.popout, .modal])
    func voiceOverMeetsADialogNamedThreads(_ presentation: ThreadBrowserModel.Presentation)
        async throws
    {
        try await opened(presentation)
        let browser = try #require(self.panel)

        #expect(browser.accessibilitySubrole() == .dialog)
        #expect(browser.accessibilityLabel() == "Threads")
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

