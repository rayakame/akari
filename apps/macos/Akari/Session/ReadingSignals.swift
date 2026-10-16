import AkariKit
import AppKit
import SwiftUI

// What the session's reading needs from AppKit: the app active, the window key and on screen,
// and the user's clicks and key presses in that window.
final class ReadingSignals: NSObject {
    // Scrolling isn't an interaction: the official client doesn't ack on it (D88).
    static let mask: NSEvent.EventTypeMask = [
        .leftMouseDown, .rightMouseDown, .otherMouseDown, .keyDown,
    ]

    private let session: SessionModel
    private weak var window: NSWindow?
    private let isActive: @MainActor () -> Bool
    private let isKey: @MainActor (NSWindow) -> Bool
    private let isVisible: @MainActor (NSWindow) -> Bool
    // False while fast user switching shows another user's session.
    private var sessionActive = true
    nonisolated(unsafe) private var monitor: Any?

    init(
        session: SessionModel, window: NSWindow, center: NotificationCenter = .default,
        workspace: NotificationCenter = NSWorkspace.shared.notificationCenter,
        isActive: @escaping @MainActor () -> Bool = { NSApp.isActive },
        isKey: @escaping @MainActor (NSWindow) -> Bool = { $0.isKeyWindow },
        isVisible: @escaping @MainActor (NSWindow) -> Bool = MessageTableController.isOnScreen,
        monitors: Bool = false
    ) {
        self.session = session
        self.window = window
        self.isActive = isActive
        self.isKey = isKey
        self.isVisible = isVisible
        super.init()
        // Selector observers go away with the object.
        for name in [
            NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification,
        ] {
            center.addObserver(self, selector: #selector(changed), name: name, object: nil)
        }
        for name in [
            NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification,
            NSWindow.didMiniaturizeNotification, NSWindow.didDeminiaturizeNotification,
            NSWindow.didChangeOcclusionStateNotification,
        ] {
            center.addObserver(self, selector: #selector(changed), name: name, object: window)
        }
        workspace.addObserver(
            self, selector: #selector(sessionResigned),
            name: NSWorkspace.sessionDidResignActiveNotification, object: nil)
        workspace.addObserver(
            self, selector: #selector(sessionBecameActive),
            name: NSWorkspace.sessionDidBecomeActiveNotification, object: nil)
        if monitors {
            monitor = NSEvent.addLocalMonitorForEvents(matching: Self.mask) { [weak self] event in
                MainActor.assumeIsolated {
                    self?.handle(event)
                }
                return event
            }
        }
        changed()
    }

    deinit {
        if let monitor {
            NSEvent.removeMonitor(monitor)
        }
    }

    static func engages(_ type: NSEvent.EventType) -> Bool {
        mask.contains(NSEvent.EventTypeMask(type: type))
    }

    func handle(_ event: NSEvent) {
        guard Self.engages(event.type), let window, event.windowNumber == window.windowNumber
        else {
            return
        }
        session.userInteracted()
    }

    @objc private func changed() {
        guard let window else {
            return
        }
        session.setAppActive(isActive())
        session.setWindowKey(isKey(window))
        session.setWindowVisible(sessionActive && isVisible(window))
    }

    @objc private func sessionResigned() {
        sessionActive = false
        changed()
    }

    @objc private func sessionBecameActive() {
        sessionActive = true
        changed()
    }

    // Feeds the session while this view is in a window.
    struct Attach: NSViewRepresentable {
        let session: SessionModel

        func makeNSView(context: Context) -> NSView {
            AttachView(session: session)
        }

        func updateNSView(_ nsView: NSView, context: Context) {}
    }

    private final class AttachView: NSView {
        private let session: SessionModel
        private var signals: ReadingSignals?

        init(session: SessionModel) {
            self.session = session
            super.init(frame: .zero)
        }

        required init?(coder: NSCoder) {
            nil
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            signals = window.map { ReadingSignals(session: session, window: $0, monitors: true) }
        }

        // Behind the whole session: clicks belong to what lies in front.
        override func hitTest(_ point: NSPoint) -> NSView? {
            nil
        }
    }
}
