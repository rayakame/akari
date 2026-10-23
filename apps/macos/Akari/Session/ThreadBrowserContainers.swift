import AkariKit
import AppKit
import SwiftUI

/// Where the header's Threads button is, for the popout under it.
struct ThreadsButtonBounds: PreferenceKey {
    static let defaultValue: Anchor<CGRect>? = nil

    static func reduce(value: inout Anchor<CGRect>?, nextValue: () -> Anchor<CGRect>?) {
        value = value ?? nextValue()
    }
}

/// The header's Threads button: first in the toolbar, selected while its popout is open.
struct ThreadsButton: View {
    static let label = "Threads"

    let selected: Bool
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            Image(systemName: ChannelSymbols.name(for: .publicThread))
                .font(.system(size: 17))
                .foregroundStyle(Color(nsColor: color))
                .frame(width: HeaderIconButton.size, height: HeaderIconButton.size)
                .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .anchorPreference(key: ThreadsButtonBounds.self, value: .bounds) { $0 }
        .help(Self.label)
        .accessibilityLabel(Self.label)
        .accessibilityAddTraits(selected ? .isSelected : [])
    }

    private var color: NSColor {
        selected ? Palette.interactiveTextActive : hovered ? Palette.interactiveText : Palette.textMuted
    }
}

/// The popout under the Threads button: its right edge on the button's, its top under the
/// header's line, 35% of the window wide (480–600), as tall as its list (400 to 80% of the
/// window). A click outside it, the button again or Escape closes it.
struct ThreadBrowserPopout: View {
    let browser: ThreadBrowserModel
    let session: SessionModel
    /// The button, in the space `area` is measured in.
    let button: CGRect
    let area: CGSize

    // 8 below the button's 32 box, and past the header's line.
    static let gap: CGFloat = 9
    static let radius: CGFloat = 8

    var body: some View {
        let width = ThreadBrowserLayout.popoutWidth(window: area.width)
        let height = ThreadBrowserLayout.popoutHeight(
            list: ThreadBrowserLayout.listHeight(browser), window: area.height)
        let frame = Self.frame(button: button, width: width, height: height)
        ZStack(alignment: .topLeading) {
            ThreadBrowserEvents(
                inside: [frame, button], onOutside: session.closeThreadBrowser,
                onEscape: session.closeThreadBrowser, onFocusFree: session.refocusChannel)
            ThreadBrowserView(browser: browser, session: session)
                .frame(width: width, height: height)
                .background(Color(nsColor: Palette.surfaceHigh))
                .clipShape(RoundedRectangle(cornerRadius: Self.radius))
                .overlay {
                    RoundedRectangle(cornerRadius: Self.radius + 1)
                        .stroke(Color(nsColor: Palette.shadowBorder), lineWidth: 1)
                        .padding(-0.5)
                }
                .shadow(color: .black.opacity(0.24), radius: 12, y: 12)
                .threadBrowserDialog()
                .offset(x: frame.minX, y: frame.minY)
        }
        .frame(width: area.width, height: area.height, alignment: .topLeading)
    }

    static func frame(button: CGRect, width: CGFloat, height: CGFloat) -> CGRect {
        CGRect(x: button.maxX - width, y: button.maxY + gap, width: width, height: height)
    }
}

/// "See all threads": the browser centered over the dimmed window, with its X. The X, Escape or
/// a click on the dimmed page closes it.
struct ThreadBrowserModal: View {
    let browser: ThreadBrowserModel
    let session: SessionModel
    let area: CGSize

    static let backdrop = 0.72
    // Discord's radius-md.
    static let radius: CGFloat = 12

    var body: some View {
        let size = ThreadBrowserLayout.modalSize(
            list: ThreadBrowserLayout.listHeight(browser), window: area)
        ZStack {
            ThreadBrowserEvents(
                inside: [], onOutside: nil, onEscape: session.closeThreadBrowser, modal: true,
                onFocusFree: session.refocusChannel)
            Color.black.opacity(Self.backdrop)
                .contentShape(Rectangle())
                .onTapGesture { session.closeThreadBrowser() }
                .accessibilityHidden(true)
            ThreadBrowserView(browser: browser, session: session)
                .padding(.top, 8)
                .padding(.bottom, 16)
                .frame(width: size.width - 2, height: size.height - 2)
                .background(Color(nsColor: Palette.surfaceHigh))
                .clipShape(RoundedRectangle(cornerRadius: Self.radius))
                .padding(1)
                .overlay {
                    RoundedRectangle(cornerRadius: Self.radius + 1)
                        .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
                }
                .shadow(color: .black.opacity(0.24), radius: 12, y: 12)
                .threadBrowserDialog()
        }
        .frame(width: area.width, height: area.height)
    }
}

extension View {
    /// As Discord's: a modal dialog named "Threads", which VoiceOver stays inside.
    func threadBrowserDialog() -> some View {
        accessibilityElement(children: .contain)
            .accessibilityLabel(ThreadsButton.label)
            .accessibilityAddTraits(.isModal)
    }
}

/// Watches the window's clicks and keys while a browser shows: a click outside `inside` (in this
/// view's space) calls `onOutside` and still reaches what it hit, as a click outside Discord's
/// popouts does; Escape calls `onEscape` and goes no further; a modal keeps typing from what's
/// behind it. Once it's closed, `onFocusFree` gets the focus nothing else took.
struct ThreadBrowserEvents: NSViewRepresentable {
    let inside: [CGRect]
    let onOutside: (() -> Void)?
    let onEscape: () -> Void
    var modal = false
    var onFocusFree: () -> Void = {}

    func makeNSView(context: Context) -> Watcher {
        Watcher()
    }

    func updateNSView(_ watcher: Watcher, context: Context) {
        watcher.inside = inside
        watcher.onOutside = onOutside
        watcher.onEscape = onEscape
        watcher.modal = modal
        watcher.onFocusFree = onFocusFree
    }

    static func dismantleNSView(_ watcher: Watcher, coordinator: ()) {
        watcher.stop()
    }

    final class Watcher: NSView {
        var inside: [CGRect] = []
        var onOutside: (() -> Void)?
        var onEscape: () -> Void = {}
        var modal = false
        var onFocusFree: () -> Void = {}
        private var monitor: Any?

        override var isFlipped: Bool { true }

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewWillMove(toWindow newWindow: NSWindow?) {
            if newWindow == nil, let window {
                handFocusOn(in: window)
            }
            super.viewWillMove(toWindow: newWindow)
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stop()
            guard window != nil else {
                return
            }
            monitor = NSEvent.addLocalMonitorForEvents(
                matching: [.leftMouseDown, .rightMouseDown, .otherMouseDown, .keyDown]
            ) { [weak self] event in
                self?.handle(event) ?? event
            }
        }

        func stop() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
        }

        // The Escape key's code on every layout.
        private static let escape: UInt16 = 53

        private static func isField(_ responder: NSResponder?) -> Bool {
            (responder as? NSTextView)?.isFieldEditor == true
        }

        // The field's focus falls back to the window unless the closing click took it. A closing
        // window may be deallocating, so it's looked up by number, never referenced.
        private func handFocusOn(in window: NSWindow) {
            let free = onFocusFree
            let number = window.windowNumber
            Task { @MainActor in
                guard let window = NSApp.window(withWindowNumber: number) else {
                    return
                }
                let current = window.firstResponder
                if current === window || (current as? NSView)?.window !== window {
                    free()
                }
            }
        }

        func handle(_ event: NSEvent) -> NSEvent? {
            guard event.window === window else {
                return event
            }
            if event.type == .keyDown {
                if event.keyCode == Self.escape {
                    // The input method's, to cancel what it composes.
                    if let text = window?.firstResponder as? NSTextView, text.hasMarkedText() {
                        return event
                    }
                    onEscape()
                    return nil
                }
                if modal, !event.modifierFlags.contains(.command),
                    !Self.isField(window?.firstResponder)
                {
                    return nil
                }
                return event
            }
            let point = convert(event.locationInWindow, from: nil)
            if let onOutside, !inside.contains(where: { $0.contains(point) }) {
                onOutside()
            }
            return event
        }
    }
}
