import AppKit
import Testing

@testable import Akari

@MainActor
struct PowerEventsTests {
    @Test
    func sleepSuspendsAndWakeResumes() {
        let center = NotificationCenter()
        var calls: [String] = []
        var events: PowerEvents? = PowerEvents(
            center: center, sleep: { calls.append("sleep") }, wake: { calls.append("wake") })

        center.post(name: NSWorkspace.willSleepNotification, object: nil)
        center.post(name: NSWorkspace.didWakeNotification, object: nil)
        #expect(calls == ["sleep", "wake"])
        #expect(events != nil)

        events = nil
        center.post(name: NSWorkspace.willSleepNotification, object: nil)
        #expect(calls == ["sleep", "wake"])
    }
}
