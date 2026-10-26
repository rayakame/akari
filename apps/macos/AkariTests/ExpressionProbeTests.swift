import AkariKit
import AppKit
import Testing

@testable import Akari

// PROBE (temporary): what macOS does with synthetic mouse events on CI's runner. Logs only.
enum Probe {
    static func event(_ type: NSEvent.EventType, _ window: NSWindow, at timestamp: TimeInterval, x: CGFloat) -> NSEvent {
        NSEvent.mouseEvent(
            with: type, location: NSPoint(x: x, y: 10), modifierFlags: [], timestamp: timestamp,
            windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1, pressure: 1)!
    }

    static func log(_ line: String) {
        NSLog("PROBE %@", line)
    }

    // Read only: the geometry the press's hit test uses, for a point in window coordinates.
    static func state(_ view: MessageTextView, at windowPoint: NSPoint) -> String {
        let point = view.convert(windowPoint, from: nil)
        let hit = view.expression(at: point).map { "\($0.rect)" } ?? "nil"
        let boxes = view.emojiOverlay.emojiViews.map { "\($0.frame)" }
        let shapes = ((view.layoutManager as? MessageLayoutManager)?.blocks() ?? []).compactMap { shape -> String? in
            if case .emoji(let rect, _) = shape { return "\(rect)" }
            return nil
        }
        return "frame \(view.frame) origin \(view.textContainerOrigin) container \(view.textContainer?.size ?? .zero) point \(point) hit \(hit) overlay \(view.emojiOverlay.frame) boxes \(boxes) shapes now \(shapes) scale \(view.window?.backingScaleFactor ?? 0) screen \(view.window?.screen?.frame ?? .zero)"
    }

    static func describe(_ event: NSEvent?) -> String {
        guard let event else {
            return "none"
        }
        return "type \(event.type.rawValue) ts \(event.timestamp) w \(event.windowNumber) x \(event.locationInWindow.x)"
    }
}

// Runs before ExpressionTests and takes nothing from the queue.
@MainActor @Suite(.serialized)
struct ExpressionProbeTests {
    @Test func theQueueBeforeTheExpressionTests() {
        Probe.log("os \(ProcessInfo.processInfo.operatingSystemVersionString) active \(NSApp.isActive) uptime \(ProcessInfo.processInfo.systemUptime) media \(CACurrentMediaTime())")
        for (name, mask) in [("down", NSEvent.EventTypeMask.leftMouseDown), ("dragged", .leftMouseDragged), ("up", .leftMouseUp), ("any", .any)] {
            let first = NSApp.nextEvent(matching: mask, until: .distantPast, inMode: .default, dequeue: false)
            Probe.log("first queued \(name): \(Probe.describe(first))")
        }
    }
}

// Runs after ExpressionTests.
@MainActor @Suite(.serialized)
struct ExpressionTestsProbe {
    // Reads as the emoji press does, but gives up after a second instead of waiting forever.
    func pressRead() -> NSEvent? {
        NSApp.nextEvent(
            matching: [.leftMouseUp, .leftMouseDragged], until: Date(timeIntervalSinceNow: 1),
            inMode: .eventTracking, dequeue: true)
    }

    // The stale-event test's sequence, read back event by event.
    @Test(arguments: [0, -1] as [TimeInterval])
    func theStaleSequence(staleOffset: TimeInterval) {
        let window = ScaledWindow(scale: 1)
        let now = ProcessInfo.processInfo.systemUptime
        let staleStamp = staleOffset == 0 ? 0 : now + staleOffset
        for type in [NSEvent.EventType.leftMouseDragged, .leftMouseUp] {
            NSApp.postEvent(Probe.event(type, window, at: staleStamp, x: 70), atStart: false)
        }
        let start = ProcessInfo.processInfo.systemUptime
        NSApp.postEvent(Probe.event(.leftMouseUp, window, at: start + 0.02, x: 10), atStart: false)
        var read: [String] = []
        while read.count < 6, let next = pressRead() {
            read.append(Probe.describe(next))
            if next.locationInWindow.x == 10 {
                break
            }
        }
        Probe.log("window \(window.windowNumber), stale stamped \(staleStamp), press \(start): \(read)")
    }

    // The stale test's view, measured as built and again after the run loop turns.
    @Test(arguments: [1, 2] as [CGFloat])
    func theGeometryBeforeAndAfterASpin(_ scale: CGFloat) throws {
        let window = ScaledWindow(scale: scale)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 200))
        window.contentView = container
        let view = MessageTextView()
        view.detectsExpressions = true
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.widthAnchor.constraint(equalToConstant: 280),
        ])
        let times = TimestampText(
            locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
            calendar: Calendar(identifier: .gregorian))
        let noon = Date(timeIntervalSince1970: 1_800_000_000)
        view.show(MessageText.build(row(1, at: noon, content: "hi <:wave:5> there").message, revealed: [], now: noon, times: times).text)
        container.layoutSubtreeIfNeeded()
        let layout = try #require(view.layoutManager)
        let glyphs = layout.glyphRange(forCharacterRange: NSRange(location: 3, length: 1), actualCharacterRange: nil)
        let rect = layout.boundingRect(forGlyphRange: glyphs, in: try #require(view.textContainer))
            .offsetBy(dx: view.textContainerOrigin.x, dy: view.textContainerOrigin.y)
        let windowPoint = view.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
        Probe.log("\(scale)x as built: \(Probe.state(view, at: windowPoint))")
        RunLoop.current.run(until: Date(timeIntervalSinceNow: 0.05))
        Probe.log("\(scale)x after 50 ms: \(Probe.state(view, at: windowPoint))")
        withExtendedLifetime(window) {}
    }

    // When the stamp changes, if it does: the first read before the press, then again.
    @Test func aZeroStampReadTwice() {
        let window = ScaledWindow(scale: 1)
        NSApp.postEvent(Probe.event(.leftMouseUp, window, at: 0, x: 70), atStart: false)
        let first = NSApp.nextEvent(matching: [.leftMouseUp], until: .distantPast, inMode: .default, dequeue: true)
        let after = ProcessInfo.processInfo.systemUptime
        if let first {
            NSApp.postEvent(first, atStart: false)
        }
        let second = NSApp.nextEvent(matching: [.leftMouseUp], until: .distantPast, inMode: .eventTracking, dequeue: true)
        Probe.log("zero read once: \(Probe.describe(first)), uptime after \(after), read again: \(Probe.describe(second))")
    }
}
