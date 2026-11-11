import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
final class ReadingSignalsTests {
    let session: SessionModel
    let window = TestWindow.make(size: NSSize(width: 400, height: 300))
    let center = NotificationCenter()
    let workspace = NotificationCenter()
    var active = true
    var key = true
    var visible = true
    var reducesMotion = false

    init() {
        session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(CrowdedStore.guildIds[0]))
    }

    func signals(monitors: Bool = false) -> ReadingSignals {
        ReadingSignals(
            session: session, window: window, center: center, workspace: workspace,
            isActive: { [unowned self] in active }, isKey: { [unowned self] _ in key },
            isVisible: { [unowned self] _ in visible },
            reducesMotion: { [unowned self] in reducesMotion }, monitors: monitors)
    }

    // The system's Reduce Motion reaches the session's motion policy, live.
    @Test
    func reduceMotionReachesTheSession() {
        reducesMotion = true
        let signals = signals()
        _ = signals
        #expect(session.motion.reduceMotion)

        reducesMotion = false
        workspace.post(
            name: NSWorkspace.accessibilityDisplayOptionsDidChangeNotification, object: nil)
        #expect(!session.motion.reduceMotion)
    }

    func click(in window: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: .leftMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
    }

    func keyDown(in window: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, characters: "a",
                charactersIgnoringModifiers: "a", isARepeat: false, keyCode: 0))
    }

    // A right click: a left one would make the window key and bring it on screen.
    func rightClick(in window: NSWindow) throws -> NSEvent {
        try settleEvents(in: window)
        return try #require(
            NSEvent.mouseEvent(
                with: .rightMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1))
    }

    // The window goes behind another app's and comes back.
    func leaveAndComeBack() {
        (active, key) = (false, false)
        center.post(name: NSApplication.didResignActiveNotification, object: nil)
        center.post(name: NSWindow.didResignKeyNotification, object: window)
        (active, key) = (true, true)
        center.post(name: NSApplication.didBecomeActiveNotification, object: nil)
        center.post(name: NSWindow.didBecomeKeyNotification, object: window)
    }

    @Test
    func aScrollWheelEventDoesntEngageWhileAMouseDownDoes() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)
        #expect(session.reading == [channel])
        leaveAndComeBack()
        #expect(session.reading.isEmpty)

        let wheel = try #require(
            CGEvent(
                scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: -40,
                wheel2: 0, wheel3: 0
            ).flatMap(NSEvent.init(cgEvent:)))
        signals.handle(wheel)
        #expect(session.reading.isEmpty)
        #expect(!ReadingSignals.engages(wheel.type))

        signals.handle(try click(in: window))
        #expect(session.reading == [channel])
    }

    @Test
    func aKeyPressEngagesButNotOneInAnotherWindow() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)
        leaveAndComeBack()
        let other = TestWindow.make(size: NSSize(width: 200, height: 100))

        signals.handle(try click(in: other))
        #expect(session.reading.isEmpty)

        let key = try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, characters: "a",
                charactersIgnoringModifiers: "a", isARepeat: false, keyCode: 0))
        signals.handle(key)
        #expect(session.reading == [channel])
    }

    @Test
    func aMinimizedOrCoveredWindowStopsReading() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)

        visible = false
        center.post(name: NSWindow.didMiniaturizeNotification, object: window)
        #expect(session.reading.isEmpty)
        visible = true
        center.post(name: NSWindow.didDeminiaturizeNotification, object: window)
        #expect(session.reading.isEmpty)
        signals.handle(try click(in: window))
        #expect(session.reading == [channel])

        visible = false
        center.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
        #expect(session.reading.isEmpty)
    }

    @Test
    func anotherWindowsNotificationsChangeNothing() throws {
        _ = signals()
        let channel = try #require(session.messages?.channelId)
        key = false

        center.post(name: NSWindow.didResignKeyNotification, object: NSWindow())

        #expect(session.reading == [channel])
    }

    // Another user's session takes the screen, and the user comes back to theirs.
    @Test
    func fastUserSwitchingStopsReading() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)

        workspace.post(name: NSWorkspace.sessionDidResignActiveNotification, object: nil)
        #expect(session.reading.isEmpty)
        workspace.post(name: NSWorkspace.sessionDidBecomeActiveNotification, object: nil)
        #expect(session.reading.isEmpty, "not before a click or a key")

        signals.handle(try click(in: window))
        #expect(session.reading == [channel])
    }

    // Events the app dispatches reach the monitor before any window.
    @Test
    func theMonitorSeesClicksAndKeysInTheSessionsWindowOnly() throws {
        let signals = signals(monitors: true)
        #expect(signals.isMonitoring)
        let channel = try #require(session.messages?.channelId)
        let other = TestWindow.make(size: NSSize(width: 200, height: 100))

        leaveAndComeBack()
        NSApp.sendEvent(try rightClick(in: other))
        NSApp.sendEvent(try keyDown(in: other))
        #expect(session.reading.isEmpty)
        NSApp.sendEvent(try rightClick(in: window))
        #expect(session.reading == [channel])

        leaveAndComeBack()
        NSApp.sendEvent(try keyDown(in: window))
        #expect(session.reading == [channel])
    }

    @Test
    func theMonitorGoesWithTheWindow() throws {
        let signals = signals(monitors: true)
        leaveAndComeBack()

        center.post(name: NSWindow.willCloseNotification, object: window)
        #expect(!signals.isMonitoring)
        NSApp.sendEvent(try rightClick(in: window))
        #expect(session.reading.isEmpty)
    }

    // SwiftUI can keep the session's view while its window is closed and opened again.
    @Test
    func theMonitorComesBackWithTheWindow() throws {
        let signals = signals(monitors: true)
        let channel = try #require(session.messages?.channelId)

        (key, visible) = (false, false)
        center.post(name: NSWindow.willCloseNotification, object: window)
        center.post(name: NSWindow.didResignKeyNotification, object: window)
        #expect(!signals.isMonitoring)
        (key, visible) = (true, true)
        center.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
        center.post(name: NSWindow.didBecomeKeyNotification, object: window)
        #expect(signals.isMonitoring)

        #expect(session.reading.isEmpty)
        NSApp.sendEvent(try rightClick(in: window))
        #expect(session.reading == [channel])
    }

    // The test host is never active, so the session's inputs are set by hand.
    @Test
    func theMonitorGoesWhenTheViewLeavesItsWindow() throws {
        let channel = try #require(session.messages?.channelId)
        let attach = ReadingSignals.AttachView(session: session)
        let disengage = { [session] in
            session.setAppActive(true)
            session.setWindowVisible(true)
            session.setWindowKey(true)
            session.setWindowKey(false)
            session.setWindowKey(true)
        }
        window.contentView?.addSubview(attach)
        disengage()
        NSApp.sendEvent(try rightClick(in: window))
        #expect(session.reading == [channel], "the monitor is in")

        attach.removeFromSuperview()
        disengage()
        NSApp.sendEvent(try rightClick(in: window))
        #expect(session.reading.isEmpty)
    }
}

// The Threads browser is a panel attached above the session's window, key while it shows: the
// channel stays read behind it, as behind Discord's popout, and what the user does in it counts.
@MainActor
struct ReadingBesideTheBrowserTests {
    final class KeyPanel: OutOfSightPanel {
        override var isKeyWindow: Bool { true }
    }

    func windows() -> (NSWindow, NSWindow, NSPanel) {
        let window = TestWindow.make(size: NSSize(width: 400, height: 300))
        let shield = TestWindow.panel(size: window.frame.size, style: [.borderless])
        let panel = TestWindow.hide(
            KeyPanel(
                contentRect: NSRect(origin: NSPoint(x: 40, y: 40), size: NSSize(width: 100, height: 100)), styleMask: [.borderless],
                backing: .buffered, defer: false))
        window.addChildWindow(shield, ordered: .above)
        shield.addChildWindow(panel, ordered: .above)
        return (window, shield, panel)
    }

    @Test
    func aPanelAttachedAboveTheWindowIsOfItsFamily() {
        let (window, shield, panel) = windows()
        let other = TestWindow.make(size: NSSize(width: 50, height: 50))

        #expect(panel.isFamily(of: window) && shield.isFamily(of: window) && window.isFamily(of: window))
        #expect(!other.isFamily(of: window) && !window.isFamily(of: panel))
    }

    // The window goes behind another app's; the user comes back to the browser's panel.
    @Test
    func thePanelsKeyAndClicksAreTheWindows() throws {
        let tests = ReadingSignalsTests()
        let (window, _, panel) = windows()
        let center = NotificationCenter()
        var (active, key) = (true, true)
        let signals = ReadingSignals(
            session: tests.session, window: window, center: center, workspace: NotificationCenter(),
            isActive: { active }, isKey: { _ in key }, isVisible: { _ in true })
        let channel = try #require(tests.session.messages?.channelId)

        (active, key) = (false, false)
        center.post(name: NSApplication.didResignActiveNotification, object: nil)
        center.post(name: NSWindow.didResignKeyNotification, object: window)
        #expect(tests.session.reading.isEmpty)
        (active, key) = (true, true)
        center.post(name: NSApplication.didBecomeActiveNotification, object: nil)
        center.post(name: NSWindow.didBecomeKeyNotification, object: panel)
        #expect(tests.session.reading.isEmpty, "not before a click or a key")

        signals.handle(try tests.rightClick(in: panel))

        #expect(tests.session.reading == [channel])
    }
}
