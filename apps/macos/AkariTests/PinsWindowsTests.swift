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

    // The panel over a shield, by its VoiceOver name.
    func panel(_ label: String) -> ContainerPanel? {
        shields.lazy.compactMap { $0.childWindows?.compactMap { $0 as? ContainerPanel }.first }
            .first { $0.accessibilityLabel() == label }
    }

    var pinsPanel: ContainerPanel? { panel(PinsModel.title) }

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
    }
}
