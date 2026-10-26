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
