import AppKit

// The gateway's timers don't notice a sleep, so the session disconnects before it and
// reconnects after it.
final class PowerEvents: NSObject {
    private let sleep: () -> Void
    private let wake: () -> Void

    init(
        center: NotificationCenter = NSWorkspace.shared.notificationCenter,
        sleep: @escaping () -> Void, wake: @escaping () -> Void
    ) {
        self.sleep = sleep
        self.wake = wake
        super.init()
        // Selector observers go away with the object.
        center.addObserver(
            self, selector: #selector(willSleep), name: NSWorkspace.willSleepNotification,
            object: nil)
        center.addObserver(
            self, selector: #selector(didWake), name: NSWorkspace.didWakeNotification, object: nil
        )
    }

    @objc private func willSleep(_ notification: Notification) {
        sleep()
    }

    @objc private func didWake(_ notification: Notification) {
        wake()
    }
}
