import AkariKit
import AppKit
import Testing

@testable import Akari

@MainActor
final class ReadingSignalsTests {
    let session: SessionModel
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 400, height: 300), styleMask: [.titled],
        backing: .buffered, defer: false)
    let center = NotificationCenter()
    var active = true
    var key = true
    var visible = true

    init() {
        session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(CrowdedStore.guildIds[0]))
    }

    func signals() -> ReadingSignals {
        ReadingSignals(
            session: session, window: window, center: center,
            isActive: { [unowned self] in active }, isKey: { [unowned self] _ in key },
            isVisible: { [unowned self] _ in visible })
    }

    func click(in window: NSWindow) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: .leftMouseDown, location: .zero, modifierFlags: [], timestamp: 0,
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
        #expect(session.reading == channel)
        leaveAndComeBack()
        #expect(session.reading == nil)

        let wheel = try #require(
            CGEvent(
                scrollWheelEvent2Source: nil, units: .pixel, wheelCount: 1, wheel1: -40,
                wheel2: 0, wheel3: 0
            ).flatMap(NSEvent.init(cgEvent:)))
        signals.handle(wheel)
        #expect(session.reading == nil)
        #expect(!ReadingSignals.engages(wheel.type))

        signals.handle(try click(in: window))
        #expect(session.reading == channel)
    }

    @Test
    func aKeyPressEngagesButNotOneInAnotherWindow() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)
        leaveAndComeBack()
        let other = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 200, height: 100), styleMask: [.titled],
            backing: .buffered, defer: false)

        signals.handle(try click(in: other))
        #expect(session.reading == nil)

        let key = try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: window.windowNumber, context: nil, characters: "a",
                charactersIgnoringModifiers: "a", isARepeat: false, keyCode: 0))
        signals.handle(key)
        #expect(session.reading == channel)
    }

    @Test
    func aMinimizedOrCoveredWindowStopsReading() throws {
        let signals = signals()
        let channel = try #require(session.messages?.channelId)

        visible = false
        center.post(name: NSWindow.didMiniaturizeNotification, object: window)
        #expect(session.reading == nil)
        visible = true
        center.post(name: NSWindow.didDeminiaturizeNotification, object: window)
        #expect(session.reading == nil)
        signals.handle(try click(in: window))
        #expect(session.reading == channel)

        visible = false
        center.post(name: NSWindow.didChangeOcclusionStateNotification, object: window)
        #expect(session.reading == nil)
    }

    @Test
    func anotherWindowsNotificationsChangeNothing() throws {
        _ = signals()
        let channel = try #require(session.messages?.channelId)
        key = false

        center.post(name: NSWindow.didResignKeyNotification, object: NSWindow())

        #expect(session.reading == channel)
    }
}
