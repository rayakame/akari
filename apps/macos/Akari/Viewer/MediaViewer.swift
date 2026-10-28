import AkariKit
import AppKit

/// The media viewer (D146): a key panel over the whole session window with Discord's scrim, the
/// media and its chrome. It goes on Escape, ⌘W, Close or a click on the scrim, and when its
/// window closes, minimizes or changes full screen; everything it asked goes with it.
@MainActor
final class MediaViewer {
    private(set) var panel: ContainerPanel?
    private(set) var view: ViewerView?
    private weak var parent: NSWindow?
    private var observers: [NSObjectProtocol] = []
    var focus = WindowFocus()
    var pasteboard = NSPasteboard.general
    var openURL: (URL) -> Void = { NSWorkspace.shared.open($0) }
    /// Where Save writes; the save panel on the viewer when unset.
    var chooseSaveURL: ((String) async -> URL?)?
    var reducesMotion: () -> Bool = { NSWorkspace.shared.accessibilityDisplayShouldReduceMotion }

    static let label = "Media Viewer"
    // F-10: Discord's 200 ms fade, the media growing from 0.9.
    static let fade: TimeInterval = 0.2
    static let openScale: CGFloat = 0.9

    func open(
        _ model: ViewerModel, in parent: NSWindow, images: ImageCache?,
        placeholders: PlaceholderCache? = nil
    ) {
        dismiss(animated: false, refocus: false)
        self.parent = parent
        let panel = ContainerPanel(keyable: true)
        panel.hasShadow = false
        panel.appearance = NSAppearance(named: .darkAqua)
        panel.setAccessibilitySubrole(.dialog)
        panel.setAccessibilityLabel(Self.label)
        let actions = ViewerActions(
            model: model, pasteboard: pasteboard, openURL: { [weak self] in self?.openURL($0) },
            chooseSaveURL: { [weak self] name in await self?.saveURL(for: name) },
            onFailure: { [weak self] message in self?.fail(message) })
        let view = ViewerView(model: model, images: images, placeholders: placeholders, actions: actions)
        view.reducesMotion = reducesMotion
        view.onClose = { [weak self] in self?.close() }
        panel.contentView = view
        panel.onEscape = { [weak self] in self?.close() }
        panel.onKey = { [weak view] event in view?.handleKey(event) ?? false }
        panel.setFrame(parent.frame, display: false)
        parent.addChildWindow(panel, ordered: .above)
        self.panel = panel
        self.view = view
        focus.makeKey(panel)
        watch(parent, panel: panel)
        updateMotion()
        view.layoutSubtreeIfNeeded()
        if !reducesMotion() {
            appear(panel, view)
        }
    }

    /// Closes it; the session's window takes the key back.
    func close() {
        dismiss(animated: !reducesMotion(), refocus: true)
    }

    private func appear(_ panel: NSPanel, _ view: ViewerView) {
        panel.alphaValue = 0
        NSAnimationContext.runAnimationGroup { context in
            context.duration = Self.fade
            context.timingFunction = CAMediaTimingFunction(name: .easeOut)
            panel.animator().alphaValue = 1
        }
        guard let layer = view.stage.tile.layer else {
            return
        }
        // AppKit's layers turn about their origin: the scale is about the middle.
        let size = layer.bounds.size
        var start = CATransform3DMakeTranslation(size.width / 2, size.height / 2, 0)
        start = CATransform3DScale(start, Self.openScale, Self.openScale, 1)
        start = CATransform3DTranslate(start, -size.width / 2, -size.height / 2, 0)
        let grow = CASpringAnimation(keyPath: "transform")
        grow.fromValue = start
        grow.toValue = CATransform3DIdentity
        grow.stiffness = 300
        grow.damping = 30
        grow.duration = grow.settlingDuration
        layer.add(grow, forKey: "open")
    }

    private func dismiss(animated: Bool, refocus: Bool) {
        for observer in observers {
            NotificationCenter.default.removeObserver(observer)
        }
        observers = []
        guard let panel, let view else {
            return
        }
        self.panel = nil
        self.view = nil
        let finish = {
            view.stop()
            panel.parent?.removeChildWindow(panel)
            panel.orderOut(nil)
        }
        if animated, panel.isVisible {
            NSAnimationContext.runAnimationGroup { context in
                context.duration = Self.fade
                panel.animator().alphaValue = 0
            } completionHandler: {
                MainActor.assumeIsolated(finish)
            }
        } else {
            finish()
        }
        if refocus, let parent, parent.isVisible, !parent.isMiniaturized {
            focus.makeKey(parent)
        }
    }

    // Follows its window's size; goes with it when it closes, minimizes or changes full screen.
    private func watch(_ parent: NSWindow, panel: NSPanel) {
        let center = NotificationCenter.default
        let gone: @Sendable (Notification) -> Void = { [weak self] _ in
            MainActor.assumeIsolated {
                self?.dismiss(animated: false, refocus: false)
            }
        }
        for name in [
            NSWindow.willCloseNotification, NSWindow.willMiniaturizeNotification,
            NSWindow.willEnterFullScreenNotification, NSWindow.willExitFullScreenNotification,
        ] {
            observers.append(center.addObserver(forName: name, object: parent, queue: nil, using: gone))
        }
        observers.append(
            center.addObserver(forName: NSWindow.didResizeNotification, object: parent, queue: nil) {
                [weak self, weak parent] _ in
                MainActor.assumeIsolated {
                    if let parent, let panel = self?.panel, panel.frame != parent.frame {
                        panel.setFrame(parent.frame, display: true)
                    }
                }
            })
        let motion: @Sendable (Notification) -> Void = { [weak self] _ in
            MainActor.assumeIsolated {
                self?.updateMotion()
            }
        }
        for name in [NSWindow.didBecomeKeyNotification, NSWindow.didResignKeyNotification] {
            observers.append(center.addObserver(forName: name, object: panel, queue: nil, using: motion))
        }
        for name in [NSApplication.didBecomeActiveNotification, NSApplication.didResignActiveNotification] {
            observers.append(center.addObserver(forName: name, object: nil, queue: nil, using: motion))
        }
    }

    // D136's rule for media under the pointer: it plays while the viewer is key and the app
    // active.
    private func updateMotion() {
        view?.animates = (panel?.isKeyWindow ?? false) && NSApp.isActive
    }

    private func saveURL(for name: String) async -> URL? {
        if let chooseSaveURL {
            return await chooseSaveURL(name)
        }
        guard let panel else {
            return nil
        }
        let save = NSSavePanel()
        save.nameFieldStringValue = name
        save.canCreateDirectories = true
        let response = await save.beginSheetModal(for: panel)
        return response == .OK ? save.url : nil
    }

    private func fail(_ message: String) {
        let alert = NSAlert()
        alert.messageText = message
        alert.alertStyle = .warning
        if let panel {
            alert.beginSheetModal(for: panel)
        } else {
            alert.runModal()
        }
    }
}
