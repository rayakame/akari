import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The pins popout in windows of its own over the session's, as the Threads popout: a clear
// shield and the popout's key panel; the unpin question over both.
@MainActor
@Suite(.releasesWhatItShows)
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

    // Every panel a test looks at must be released once it closes.
    func panel(_ label: String) -> ContainerPanel? {
        let found = shields.lazy.compactMap { $0.childWindows?.compactMap { $0 as? ContainerPanel }.first }
            .first { $0.accessibilityLabel() == label }
        Releases.watch(found)
        return found
    }

    var pinsPanel: ContainerPanel? { panel(PinsModel.title) }

    // Each test closes what it opened, as the app does: a popout or a question left in a window keeps
    // the main thread busy beside every suite after it.
    func close() {
        Releases.watch(window)
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

    func escape(in panel: NSWindow) throws {
        panel.sendEvent(
            try #require(
                NSEvent.keyEvent(
                    with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                    windowNumber: panel.windowNumber, context: nil, characters: "\u{1b}",
                    charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53)))
    }

    func attached<W: NSWindow>(_ panel: W, to parent: NSWindow) -> W {
        panel.setFrame(NSRect(x: 0, y: 0, width: 100, height: 100), display: false)
        let hidden = TestWindow.hide(panel)
        parent.addChildWindow(hidden, ordered: .above)
        return hidden
    }

    // The key goes back past a panel of another kind, one that never takes the key and another
    // window's: to the session's window.
    @Test
    func theKeyGoesBackOnlyToTheSessionsOwnKeyPanels() async throws {
        let pins = try await opened()
        let other = TestWindow.make(size: NSSize(width: 100, height: 100))
        other.orderFrontRegardless()
        let decoys = [
            attached(TestWindow.panel(size: NSSize(width: 100, height: 100), style: [.titled]), to: window),
            attached(ContainerPanel(keyable: false), to: window),
            attached(ContainerPanel(keyable: true), to: other),
        ]

        try escape(in: pins)

        try await waitUntil("closed") { session.pins == nil }
        #expect(keyed.last === window)
        for decoy in decoys {
            decoy.parent?.removeChildWindow(decoy)
            decoy.orderOut(nil)
        }
        other.orderOut(nil)
        close()
    }

    @Test
    func theKeyGoesBackToTheFrontmostOfTwoPanels() async throws {
        _ = try await opened()
        let front = attached(ContainerPanel(keyable: true), to: window)
        let model = try #require(session.pins)
        model.unpin(try #require(model.rows.first), shift: false)
        try await waitUntil("the question") { panel("Unpin Message")?.isVisible == true }

        session.answerPinQuestion(false)

        try await waitUntil("the question's windows go") { panel("Unpin Message") == nil }
        #expect(keyed.last === front)
        window.removeChildWindow(front)
        front.orderOut(nil)
        close()
    }

    @Test
    func theCardsDrawWithTheSessionsImages() async throws {
        let pins = try await opened()
        let content = try #require(pins.contentView)

        let previews = MessageMenuAreaTests.views(MessagePreviewView.self, in: content)

        #expect(!previews.isEmpty)
        #expect(previews.allSatisfy { $0.cell.images === session.images })
        close()
    }

    // The line of a thread's pin opens the thread's own popout in a window of its own.
    @Test
    func anotherChannelsPopoutHasItsOwnWindow() async throws {
        let first = try await opened()
        let thread = ChannelId(rawValue: 1_000)

        session.togglePins(of: thread)

        try await waitUntil("the thread's popout") {
            session.pins?.channelId == thread && pinsPanel !== first && pinsPanel?.isVisible == true
        }
        #expect(!first.isVisible)
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
@Suite(.releasesWhatItShows)
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

    // Every question's panel must be released once it closes.
    var panel: ContainerPanel? {
        let found = shield?.childWindows?.compactMap { $0 as? ContainerPanel }.first
        Releases.watch(found)
        return found
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
        Releases.watch(childrenOf: view.window)
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
@Suite(.releasesWhatItShows)
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
        Releases.watch(childrenOf: view.window)
        session.closePins()
        session.close()
        view.close()
        windows.expectNoneLeft()
    }
}

// The header's pins button as the session shows it: its dot, selected while the popout is open,
// and without the hand behind the age gate.
@MainActor
@Suite(.releasesWhatItShows)
struct PinsButtonInTheHeaderTests {
    let windows = ShownWindows()

    func screen(_ configure: (FakeBrowserStore) -> Void = { _ in }) -> (SessionModel, ShownView) {
        let fake = FakeBrowserStore()
        fake.lines = true
        fake.pins = 1
        configure(fake)
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        let view = ShownView(SessionScreen(session: session, splitWidth: SplitWidth()), size: NSSize(width: 1100, height: 700))
        view.window.appearance = NSAppearance(named: .darkAqua)
        return (session, view)
    }

    // The rightmost 32 box with the hand in the header.
    func button(in view: ShownView) async throws -> NSRect {
        var found: NSRect?
        try await view.waitUntil("the header's buttons") {
            found = view.hands().filter { $0.width == 32 && $0.minY < 100 }.max { $0.minX < $1.minX }
            return found != nil
        }
        return try #require(found)
    }

    func drawn(_ view: ShownView) throws -> (NSPoint) -> NSColor? {
        view.host.layoutSubtreeIfNeeded()
        let rep = try #require(view.host.bitmapImageRepForCachingDisplay(in: view.host.bounds))
        view.host.cacheDisplay(in: view.host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / view.host.bounds.width
        return { rep.colorAt(x: Int($0.x * scale), y: Int($0.y * scale))?.usingColorSpace(.sRGB) }
    }

    // The brightest of the pin's 20 box, in the 32 box's middle.
    func ink(_ color: (NSPoint) -> NSColor?, in box: NSRect) -> CGFloat {
        var most: CGFloat = 0
        for x in stride(from: box.minX + 6, to: box.minX + 26, by: 0.5) {
            for y in stride(from: box.minY + 6, to: box.minY + 26, by: 0.5) {
                most = max(most, color(NSPoint(x: x, y: y))?.brightnessComponent ?? 0)
            }
        }
        return most
    }

    func close(_ session: SessionModel, _ view: ShownView) {
        Releases.watch(childrenOf: view.window)
        session.closePins()
        session.close()
        view.close()
        windows.expectNoneLeft()
    }

    @Test
    func theDotShowsWhileTheChannelHasPinsTheUserHasntSeen() async throws {
        let (session, view) = screen { $0.pinsDot = true }
        let box = try await button(in: view)

        let dot = try drawn(view)(NSPoint(x: box.minX + 23, y: box.minY + 23))

        let red = dot?.redComponent ?? 0
        #expect(red > 0.75 && red - (dot?.greenComponent ?? 1) > 0.35 && red - (dot?.blueComponent ?? 1) > 0.35, "\(String(describing: dot))")
        close(session, view)
    }

    @Test
    func itIsSelectedWhileThePopoutIsOpen() async throws {
        let (session, view) = screen()
        let box = try await button(in: view)
        let rest = ink(try drawn(view), in: box)

        session.togglePins()

        var open = rest
        try await view.waitUntil("selected") {
            open = ink(try drawn(view), in: box)
            return open > rest + 0.1
        }
        #expect(open > rest + 0.1, "\(open) against \(rest)")
        close(session, view)
    }

    @Test
    func behindTheAgeGateItHasNoHand() async throws {
        let (session, view) = screen()
        let box = try await button(in: view)
        close(session, view)

        let (gated, gate) = screen { $0.ageGated = true }
        try await gate.waitUntil("the header") { !gate.hands().isEmpty }

        #expect(gated.pinsDisabled)
        #expect(!gate.hands().contains { $0.intersects(box) })
        close(gated, gate)
    }
}

// Outside the main actor: Swift 6.3's @TaskLocal gives `$current` the enclosing type's isolation.
nonisolated enum ReleasesScope {
    @TaskLocal static var current: Releases?
}

// What a test's windows held, by weak reference, must be gone once it is over: one kept alive
// keeps laying out and taking frame notifications beside every suite after it.
@MainActor
final class Releases {

    private final class Held {
        weak var object: AnyObject?
        let name: String

        init(_ object: AnyObject) {
            self.object = object
            name = String(describing: type(of: object)).components(separatedBy: "<").first ?? ""
        }
    }

    private var held: [ObjectIdentifier: Held] = [:]

    private static func kept(_ view: NSView) -> Bool {
        view is NSScrollView || view is NSTextView || view is MessagePreviewView || view is MessagePreviewBox
            || view is MessageQuestionView || view is NSProgressIndicator
            || String(describing: type(of: view)).contains("HostingView")
    }

    /// Watches the window, its children and their views as they are now.
    static func watch(_ window: NSWindow?) {
        ReleasesScope.current?.watch(window)
    }

    static func watch(childrenOf window: NSWindow) {
        for child in window.childWindows ?? [] {
            ReleasesScope.current?.watch(child)
        }
    }

    private func watch(_ window: NSWindow?) {
        guard let window else {
            return
        }
        held[ObjectIdentifier(window)] = Held(window)
        var views = window.contentView.map { [$0] } ?? []
        while let view = views.popLast() {
            if Self.kept(view) {
                held[ObjectIdentifier(view)] = Held(view)
            }
            views += view.subviews
        }
        for child in window.childWindows ?? [] {
            watch(child)
        }
    }

    func expectReleased() async throws {
        // A window still alive now shows what it keeps.
        for window in held.values.compactMap({ $0.object as? NSWindow }) {
            watch(window)
        }
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline, held.values.contains(where: { $0.object != nil }) {
            try await Task.sleep(for: .milliseconds(20))
        }
        var alive: [String: Int] = [:]
        for kept in held.values where kept.object != nil {
            alive[kept.name, default: 0] += 1
        }
        let named = alive.sorted { $0.key < $1.key }.map { "\($0.key) × \($0.value)" }
        #expect(alive.isEmpty, "still alive 5 s after the test: \(named.joined(separator: ", "))")
    }
}

// Checks each test once it is over, its suite and locals gone, for what it watched.
nonisolated struct ReleasesWhatItShows: TestTrait, SuiteTrait, TestScoping {
    var isRecursive: Bool { true }

    func scopeProvider(for test: Test, testCase: Test.Case?) -> Self? {
        testCase == nil ? nil : self
    }

    @concurrent func provideScope(
        for test: Test, testCase: Test.Case?, performing function: @concurrent @Sendable () async throws -> Void
    ) async throws {
        let releases = await Releases()
        try await ReleasesScope.$current.withValue(releases) {
            try await function()
        }
        try await releases.expectReleased()
    }
}

extension Trait where Self == ReleasesWhatItShows {
    static var releasesWhatItShows: Self { Self() }
}
