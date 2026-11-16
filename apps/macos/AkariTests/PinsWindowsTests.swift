import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The pins popout in windows of its own over the session's, as the Threads popout: a clear
// shield and the popout's key panel; the unpin question over both.
@MainActor
final class PinsWindowsTests {
    let fake = FakeBrowserStore()
    let session: SessionModel
    let window: ThreadBrowserWindowsTests.KeyWindow
    let host: NSHostingView<AnyView>
    var keyed: [NSWindow] = []
    let windows = ShownWindows()

    init() {
        fake.lines = true
        fake.pins = 3
        session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        window = TestWindow.hide(
            ThreadBrowserWindowsTests.KeyWindow(
                contentRect: NSRect(origin: .zero, size: NSSize(width: 1100, height: 813)),
                styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false))
        window.isReleasedWhenClosed = false
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
                    for grandchild in child.childWindows ?? [] {
                        grandchild.orderOut(nil)
                    }
                    child.orderOut(nil)
                }
                window.orderOut(nil)
            }
        }
    }

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

    var shields: [ShieldWindow] {
        window.childWindows?.compactMap { $0 as? ShieldWindow } ?? []
    }

    func panel(_ label: String) -> ContainerPanel? {
        shields.lazy.compactMap { $0.childWindows?.compactMap { $0 as? ContainerPanel }.first }
            .first { $0.accessibilityLabel() == label }
    }

    var pinsPanel: ContainerPanel? { panel(PinsModel.title) }

    // Each test closes what it opened, as the app does: a popout or a question left in a window keeps
    // the main thread busy beside every suite after it.
    func close() {
        if session.pinQuestion != nil {
            session.answerPinQuestion(false)
        }
        session.closePins()
        session.close()
        host.rootView = AnyView(EmptyView())
        host.layoutSubtreeIfNeeded()
        for child in window.childWindows ?? [] {
            child.orderOut(nil)
        }
        window.orderOut(nil)
        windows.expectNoneLeft()
    }

    func fromTop(_ rect: NSRect) -> NSRect {
        NSRect(
            x: rect.minX - window.frame.minX, y: window.frame.maxY - rect.maxY,
            width: rect.width, height: rect.height)
    }

    func screenPoint(fromTop x: CGFloat, _ y: CGFloat) -> NSPoint {
        NSPoint(x: window.frame.minX + x, y: window.frame.maxY - y)
    }

    func mouse(_ type: NSEvent.EventType, at point: NSPoint, in target: NSWindow) throws -> NSEvent {
        try settleEvents(in: target)
        return try #require(
            NSEvent.mouseEvent(
                with: type, location: target.convertPoint(fromScreen: point), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: target.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
    }

    func opened() async throws -> ContainerPanel {
        try await waitUntil("the header") { window.firstResponder is ComposerTextView }
        session.togglePins()
        try await waitUntil("the popout's panel") { pinsPanel?.isVisible == true }
        return try #require(pinsPanel)
    }

    // Its right edge on the pins button's (1084, 16 from the window's), its top 81 from the
    // window's top, 420 wide; the shield is clear.
    @Test
    func thePopoutIsAKeyPanelUnderThePinsButton() async throws {
        let pins = try await opened()
        let cover = try #require(shields.first)

        let frame = fromTop(pins.frame)
        #expect(abs(frame.maxX - 1084) <= 1 && abs(frame.minY - 81) <= 1, "\(frame)")
        #expect(frame.width == 420)
        #expect(cover.frame == window.frame)
        #expect(cover.backgroundColor.alphaComponent == 0)
        #expect(keyed.last === pins)
        close()
    }

    // The button: the popout opens; a second press lands on the shield and closes it.
    @Test
    func thePinsButtonOpensThePopoutAndAPressOutsideClosesIt() async throws {
        try await waitUntil("the header") { window.firstResponder is ComposerTextView }
        let button = screenPoint(fromTop: 1068, 56)
        for type in [NSEvent.EventType.leftMouseDown, .leftMouseUp] {
            window.sendEvent(try mouse(type, at: button, in: window))
            try await Task.sleep(for: .milliseconds(30))
        }
        try await waitUntil("opened") { session.pins != nil && pinsPanel?.isVisible == true }
        let cover = try #require(shields.first)

        cover.sendEvent(try mouse(.leftMouseDown, at: button, in: cover))

        try await waitUntil("closed") { session.pins == nil && (window.childWindows ?? []).isEmpty }
        #expect(keyed.last === window)
        close()
    }

    @Test
    func escapeClosesItAndTheComposerHasTheFocusAgain() async throws {
        let pins = try await opened()
        window.makeFirstResponder(nil)

        pins.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: pins.windowNumber, context: nil, characters: "\u{1b}",
                    charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53)))

        try await waitUntil("closed") { session.pins == nil && (window.childWindows ?? []).isEmpty }
        #expect(keyed.last === window)
        try await waitUntil("the composer's focus") { window.firstResponder is ComposerTextView }
        close()
    }

    // The question takes the key over the popout and gives it back to the popout, not the window.
    @Test
    func theUnpinQuestionStandsOverThePopoutAndGivesItTheKeyBack() async throws {
        let pins = try await opened()
        let model = try #require(session.pins)

        model.unpin(try #require(model.rows.first), shift: false)
        try await waitUntil("the question") { panel("Unpin Message")?.isVisible == true }
        let question = try #require(panel("Unpin Message"))
        #expect(keyed.last === question)
        let order = (NSWindow.windowNumbers(options: []) ?? []).map(\.intValue)
        let questionIndex = try #require(order.firstIndex(of: question.windowNumber))
        let pinsIndex = try #require(order.firstIndex(of: pins.windowNumber))
        #expect(questionIndex < pinsIndex, "front to back")

        session.answerPinQuestion(false)

        try await waitUntil("the question's windows go") { panel("Unpin Message") == nil }
        #expect(keyed.last === pins)
        #expect(session.pins != nil && pins.isVisible)
        close()
    }

    @Test
    func closingThePopoutUnderTheQuestionLeavesTheQuestionTheKey() async throws {
        _ = try await opened()
        let model = try #require(session.pins)
        model.unpin(try #require(model.rows.first), shift: false)
        try await waitUntil("the question") { panel("Unpin Message")?.isVisible == true }
        let question = try #require(panel("Unpin Message"))

        session.closePins()

        try await waitUntil("the popout's windows go") { pinsPanel == nil }
        #expect(keyed.last === question)
        #expect(session.pinQuestion != nil)
        session.answerPinQuestion(false)
        close()
    }
}

// Pin's and Unpin's window as the session shows it: over the dimmed session, its yes pins, and
// Cancel, Escape and every way of closing it answer no; it stays while another app is active.
@MainActor
struct PinQuestionWindowTests {
    let fake = FakeBrowserStore()
    let windows = ShownWindows()
    let session: SessionModel
    let view: ShownView

    init() {
        fake.lines = true
        fake.pinsLines = true
        session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        view = ShownView(SessionScreen(session: session, splitWidth: SplitWidth()), size: NSSize(width: 1100, height: 700))
    }

    var shield: ShieldWindow? {
        view.window.childWindows?.compactMap { $0 as? ShieldWindow }.first
    }

    var panel: ContainerPanel? {
        shield?.childWindows?.compactMap { $0 as? ContainerPanel }.first
    }

    var pinCalls: [String] {
        (fake.account?.pinCalls.withLock { $0 } ?? []).filter { $0.hasPrefix("pin ") }
    }

    func rows() throws -> (MessageListModel.Row, MessageListModel.Row) {
        let rows = try #require(session.messages?.rows)
        return (try #require(rows.first), try #require(rows.last))
    }

    func ask(_ row: MessageListModel.Row) async throws -> ContainerPanel {
        let list = try #require(session.messages)
        session.messageActions(for: list) { _ in }.perform(.pin, on: row, shift: false, from: .menu)
        try await view.waitUntil("the question") { panel?.isVisible == true }
        return try #require(panel)
    }

    func question(in panel: ContainerPanel) throws -> MessageQuestionView {
        let content = try #require(panel.contentView)
        return try #require(MessageMenuAreaTests.views(MessageQuestionView.self, in: content).first)
    }

    func close() {
        if session.pinQuestion != nil {
            session.answerPinQuestion(false)
        }
        session.close()
        view.close()
        windows.expectNoneLeft()
    }

    // Whatever an earlier answer started reaches the account before this pin does.
    func pinAfterwards(_ row: MessageListModel.Row) async throws {
        try question(in: try await ask(row)).confirm.performClick(nil)
        try await view.waitUntil("the pin") { pinCalls.contains("pin \(row.message.id.rawValue)") }
    }

    @Test
    func theQuestionShowsOverTheDimmedSessionAndItsYesPins() async throws {
        let (row, _) = try rows()

        let panel = try await ask(row)
        #expect((shield?.backgroundColor.alphaComponent ?? 0) > 0.5)
        try question(in: panel).confirm.performClick(nil)

        try await view.waitUntil("the pin") { !pinCalls.isEmpty }
        #expect(pinCalls == ["pin \(row.message.id.rawValue)"])
        #expect(session.pinQuestion == nil)
        close()
    }

    enum Closing: CaseIterable {
        case cancel, escape, commandW, pressOutside, miniaturize
    }

    @Test(arguments: Closing.allCases)
    func everyOtherWayOutPinsNothing(_ closing: Closing) async throws {
        let (row, other) = try rows()
        let panel = try await ask(row)

        switch closing {
        case .cancel:
            try question(in: panel).cancel.performClick(nil)
        case .escape:
            panel.sendEvent(
                try #require(
                    NSEvent.keyEvent(
                        with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                        windowNumber: panel.windowNumber, context: nil, characters: "\u{1b}",
                        charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53)))
        case .commandW:
            panel.performClose(nil)
        case .pressOutside:
            try #require(shield).onPress()
        case .miniaturize:
            NotificationCenter.default.post(name: NSWindow.willMiniaturizeNotification, object: view.window)
        }

        try await view.waitUntil("closed") { session.pinQuestion == nil && self.panel == nil }
        try await pinAfterwards(other)
        #expect(pinCalls == ["pin \(other.message.id.rawValue)"])
        close()
    }

    @Test
    func theQuestionStaysWhileAnotherAppIsActive() async throws {
        let (row, _) = try rows()
        let panel = try await ask(row)

        NotificationCenter.default.post(name: NSApplication.didResignActiveNotification, object: NSApp)

        #expect(session.pinQuestion != nil)
        #expect(self.panel === panel && panel.isVisible)
        session.answerPinQuestion(false)
        close()
    }

    @Test
    func aNewQuestionHasAWindowOfItsOwn() async throws {
        let (row, other) = try rows()
        let first = try await ask(row)
        let list = try #require(session.messages)

        session.messageActions(for: list) { _ in }.perform(.pin, on: other, shift: false, from: .menu)

        try await view.waitUntil("the new window") { panel !== first && panel?.isVisible == true }
        #expect(!first.isVisible)
        session.answerPinQuestion(false)
        close()
    }
}

// "pinned messages" in a pin's line, through the app's wiring: the bridge, the message area and
// the session screen.
@MainActor
struct PinLineWiringTests {
    let windows = ShownWindows()

    @Test
    func theBridgeHandsTheLinesChannelOn() throws {
        let fake = FakeBrowserStore()
        fake.lines = true
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        let messages = try #require(session.messages)
        var toggled: [ChannelId] = []
        let bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in },
            togglePins: { toggled.append($0) }, markServerRead: {})

        bridge.togglePins(of: FakeBrowserStore.parent)

        #expect(toggled == [FakeBrowserStore.parent])
    }

    @Test
    func pinnedMessagesInTheListOpensThePopout() async throws {
        let fake = FakeBrowserStore()
        fake.lines = true
        fake.pins = 1
        fake.pinLine = true
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        let view = ShownView(SessionScreen(session: session, splitWidth: SplitWidth()), size: NSSize(width: 1100, height: 700))
        var line: MessageTextView?
        try await view.waitUntil("the pin's line") {
            line = MessageMenuAreaTests.views(MessageTextView.self, in: view.host)
                .first { $0.string.contains("pinned messages") }
            return line != nil
        }
        let text = try #require(line)
        let link = try #require(try attribute(.link, of: "pinned messages", in: text))

        text.clicked(onLink: link, at: try index(of: "pinned messages", in: text))

        try await view.waitUntil("the popout") { session.pins?.channelId == FakeBrowserStore.parent }
        session.closePins()
        session.close()
        view.close()
        windows.expectNoneLeft()
    }
}
