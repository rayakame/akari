import AppKit
import SwiftUI

extension NSWindow {
    /// This window or one attached above it, as the browser's panel is to the session's.
    func isFamily(of window: NSWindow) -> Bool {
        var current: NSWindow? = self
        while let next = current {
            if next === window {
                return true
            }
            current = next.parent
        }
        return false
    }

    /// The key window is this one or one attached above it.
    var holdsKey: Bool {
        NSApp.keyWindow?.isFamily(of: self) ?? isKeyWindow
    }
}

/// How a window takes the key; tests record it, since a test's app never becomes active.
struct WindowFocus {
    var makeKey: @MainActor (NSWindow) -> Void = { $0.makeKey() }
}

extension EnvironmentValues {
    @Entry var windowFocus = WindowFocus()
}

/// A borderless window attached to the session's, hosting SwiftUI: the Threads browser (key
/// while it shows, so typing, Tab and Escape stay in it) or "More Active Threads" (never key).
final class ContainerPanel: NSPanel {
    var onEscape: () -> Void = {}
    /// Sees each key press before the first responder; true when it took it (the viewer's arrows,
    /// which a video's player would otherwise seek with).
    var onKey: (NSEvent) -> Bool = { _ in false }
    private let keyable: Bool

    init(keyable: Bool) {
        self.keyable = keyable
        super.init(
            contentRect: .zero,
            styleMask: keyable ? [.borderless] : [.borderless, .nonactivatingPanel],
            backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = true
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        animationBehavior = .none
    }

    override var canBecomeKey: Bool { keyable }
    override var canBecomeMain: Bool { false }

    // The Window menu's commands go to the key window, this panel. Close closes it; the others
    // (Fill, Center, Move & Resize, Zoom, Minimize, full screen) belong to the session's window:
    // the panel says it can't, and names that window, so AppKit sends them there.
    override func performClose(_ sender: Any?) {
        onEscape()
    }

    override func validateUserInterfaceItem(_ item: any NSValidatedUserInterfaceItem) -> Bool {
        if item.action == #selector(performClose(_:)) {
            return keyable
        }
        return super.validateUserInterfaceItem(item)
    }

    private var root: NSWindow? {
        var root: NSWindow = self
        while let parent = root.parent {
            root = parent
        }
        return root === self ? nil : root
    }

    nonisolated private static func isWindowCommand(_ action: Selector) -> Bool {
        let name = NSStringFromSelector(action)
        return [
            #selector(NSWindow.performZoom(_:)), #selector(NSWindow.zoom(_:)),
            #selector(NSWindow.toggleFullScreen(_:)), #selector(NSWindow.performMiniaturize(_:)),
            #selector(NSWindow.miniaturize(_:)),
        ].contains(action) || name.hasPrefix("_zoom") || name.hasPrefix("_tile")
    }

    // AppKit asks on the main thread; anywhere else the panel answers as any window.
    nonisolated override func responds(to action: Selector!) -> Bool {
        if let action, Self.isWindowCommand(action), Thread.isMainThread,
            MainActor.assumeIsolated({ root != nil })
        {
            return false
        }
        return super.responds(to: action)
    }

    override func supplementalTarget(forAction action: Selector, sender: Any?) -> Any? {
        if Self.isWindowCommand(action), let root {
            return root
        }
        return super.supplementalTarget(forAction: action, sender: sender)
    }

    // NSPanel's would close it through performClose; sendEvent already took Escape.
    override func cancelOperation(_ sender: Any?) {}

    // Escape closes it, unless an input method composes: then it cancels what it composed.
    override func sendEvent(_ event: NSEvent) {
        if event.type == .keyDown, event.keyCode == 53,
            (firstResponder as? NSTextView)?.hasMarkedText() != true
        {
            onEscape()
            return
        }
        if event.type == .keyDown, onKey(event) {
            return
        }
        super.sendEvent(event)
    }
}

/// Over the whole session window under the browser: every mouse event outside the browser comes
/// here, the pointer is the arrow, and a press closes the browser and goes no further. The
/// modal's shield is its dimmed page.
final class ShieldWindow: NSPanel {
    static let dim = 0.72

    private let shield = ShieldView()

    var onPress: () -> Void {
        get { shield.onPress }
        set { shield.onPress = newValue }
    }

    init(dimmed: Bool) {
        super.init(
            contentRect: .zero, styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered, defer: false)
        isOpaque = false
        backgroundColor = dimmed ? NSColor.black.withAlphaComponent(Self.dim) : .clear
        hasShadow = false
        isReleasedWhenClosed = false
        hidesOnDeactivate = false
        animationBehavior = .none
        contentView = shield
        // A clear window would let clicks through its transparent pixels.
        ignoresMouseEvents = false
        setAccessibilityElement(false)
    }

    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }

    final class ShieldView: NSView {
        var onPress: () -> Void = {}
        private var area: NSTrackingArea?

        // Never key: its first click is the press.
        override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

        override func mouseDown(with event: NSEvent) { onPress() }
        override func rightMouseDown(with event: NSEvent) { onPress() }
        override func otherMouseDown(with event: NSEvent) { onPress() }
        override func scrollWheel(with event: NSEvent) {}

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if let area {
                removeTrackingArea(area)
            }
            let area = NSTrackingArea(
                rect: .zero, options: [.cursorUpdate, .mouseMoved, .activeInActiveApp, .inVisibleRect],
                owner: self)
            addTrackingArea(area)
            self.area = area
        }

        override func cursorUpdate(with event: NSEvent) { NSCursor.arrow.set() }
        override func mouseMoved(with event: NSEvent) { NSCursor.arrow.set() }
    }
}

// A panel's content takes the first click, as a window that never becomes key must.
final class PanelHostingView: NSHostingView<AnyView> {
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }
}

/// A panel, and for the browser the shield under it, attached to the session's window: they move
/// with it, follow its size (each layout places them), and go when it closes, minimizes, enters
/// or leaves full screen, or the app goes to the background.
@MainActor
final class ChildWindows {
    private(set) var panel: ContainerPanel?
    private(set) var shield: ShieldWindow?
    private weak var parent: NSWindow?
    private var host: PanelHostingView?
    private var observers: [NSObjectProtocol] = []
    private let keyable: Bool
    /// VoiceOver's name for the panel, a dialog.
    var label: String?
    /// The panel is as wide as its content, from the frame's left edge.
    var fitsWidth = false
    /// The panel or shield asked to close (Escape, a press on the shield) or its window went.
    var onClose: () -> Void = {}
    /// After the browser's panel went and its window has the key back: the composer's focus.
    var refocus: () -> Void = {}
    var focus = WindowFocus()

    init(keyable: Bool) {
        self.keyable = keyable
    }

    /// Shows `content` at `frame` (screen coordinates), with the shield when `keyable`.
    func show(_ content: AnyView, at frame: NSRect, in parent: NSWindow, dimmed: Bool) {
        if self.parent !== parent {
            tearDown(refocus: false)
        }
        if let host {
            host.rootView = content
        } else {
            open(content, in: parent, dimmed: dimmed)
        }
        panel?.appearance = parent.effectiveAppearance
        shield?.appearance = parent.effectiveAppearance
        var frame = frame
        if fitsWidth, let host {
            frame.size.width = host.fittingSize.width
        }
        if shield?.frame != parent.frame {
            shield?.setFrame(parent.frame, display: true)
        }
        if panel?.frame != frame {
            panel?.setFrame(frame, display: true)
            panel?.invalidateShadow()
        }
    }

    private func open(_ content: AnyView, in parent: NSWindow, dimmed: Bool) {
        self.parent = parent
        let panel = ContainerPanel(keyable: keyable)
        let host = PanelHostingView(rootView: content)
        panel.contentView = host
        panel.onEscape = { [weak self] in self?.onClose() }
        if let label {
            panel.setAccessibilitySubrole(.dialog)
            panel.setAccessibilityLabel(label)
        }
        var below: NSWindow = parent
        if keyable {
            let shield = ShieldWindow(dimmed: dimmed)
            shield.onPress = { [weak self] in self?.onClose() }
            shield.setFrame(parent.frame, display: false)
            parent.addChildWindow(shield, ordered: .above)
            self.shield = shield
            below = shield
        }
        below.addChildWindow(panel, ordered: .above)
        self.panel = panel
        self.host = host
        if keyable {
            focus.makeKey(panel)
        }
        watch(parent)
    }

    private func watch(_ parent: NSWindow) {
        let center = NotificationCenter.default
        let closing = { [weak self] (_: Notification) in
            MainActor.assumeIsolated {
                guard let self, self.panel != nil else {
                    return
                }
                self.tearDown(refocus: false)
                self.onClose()
            }
        }
        for name in [
            NSWindow.willCloseNotification, NSWindow.willMiniaturizeNotification,
            NSWindow.willEnterFullScreenNotification, NSWindow.willExitFullScreenNotification,
        ] {
            observers.append(
                center.addObserver(forName: name, object: parent, queue: nil, using: closing))
        }
        observers.append(
            center.addObserver(
                forName: NSApplication.didResignActiveNotification, object: nil, queue: nil,
                using: closing))
    }

    /// Takes the windows away; `refocus` gives the session's window the key back.
    func tearDown(refocus: Bool) {
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
        observers = []
        if let panel {
            panel.parent?.removeChildWindow(panel)
            panel.orderOut(nil)
        }
        if let shield {
            shield.parent?.removeChildWindow(shield)
            shield.orderOut(nil)
        }
        let hadKey = panel != nil && keyable
        panel = nil
        shield = nil
        host = nil
        if refocus, hadKey, let parent, parent.isVisible, !parent.isMiniaturized {
            focus.makeKey(parent)
            self.refocus()
        }
    }
}

/// Places its windows from a SwiftUI layout: `frame` is in this view's space, on the screen its
/// window is on. The view fills the space it measures and takes no clicks; the windows go with
/// it.
struct ChildWindowsHost: NSViewRepresentable {
    let content: AnyView
    let frame: CGRect
    /// The browser: key, over a shield. More Active Threads: neither.
    var browser = false
    var dimmed = false
    var label: String?
    var fitsWidth = false
    var onClose: () -> Void = {}
    var refocus: () -> Void = {}

    final class Anchor: NSView {
        var place: () -> Void = {}

        override var isFlipped: Bool { true }
        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            place()
        }
    }

    func makeCoordinator() -> ChildWindows {
        ChildWindows(keyable: browser)
    }

    func makeNSView(context: Context) -> Anchor {
        Anchor()
    }

    func updateNSView(_ anchor: Anchor, context: Context) {
        let windows = context.coordinator
        windows.focus = context.environment.windowFocus
        windows.onClose = onClose
        windows.refocus = refocus
        windows.label = label
        windows.fitsWidth = fitsWidth
        let content = content
        let frame = frame
        let dimmed = dimmed
        anchor.place = { [weak anchor, weak windows] in
            guard let anchor, let windows, let window = anchor.window else {
                return
            }
            let screen = window.convertToScreen(anchor.convert(frame, to: nil))
            windows.show(content, at: screen, in: window, dimmed: dimmed)
        }
        anchor.place()
    }

    static func dismantleNSView(_ anchor: Anchor, coordinator: ChildWindows) {
        anchor.place = {}
        coordinator.tearDown(refocus: true)
    }
}
