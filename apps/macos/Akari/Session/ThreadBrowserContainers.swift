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
        .background(CursorArea(.pointingHand))
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
/// window). A click outside it (on the button too) or Escape closes it.
struct ThreadBrowserPopout: View {
    let browser: ThreadBrowserModel
    let session: SessionModel
    /// The button, in the space `area` is measured in.
    let button: CGRect
    let area: CGSize
    @State private var fieldRequests = 0

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
                close: session.closeThreadBrowser, onTab: { fieldRequests += 1 },
                onFocusFree: session.refocusChannel)
            ThreadBrowserView(browser: browser, session: session, fieldRequests: fieldRequests)
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
    @State private var fieldRequests = 0

    static let backdrop = 0.72
    // Discord's radius-md.
    static let radius: CGFloat = 12

    var body: some View {
        let size = ThreadBrowserLayout.modalSize(
            list: ThreadBrowserLayout.listHeight(browser), window: area)
        ZStack {
            Color.black.opacity(Self.backdrop)
                .allowsHitTesting(false)
                .accessibilityHidden(true)
            ThreadBrowserEvents(
                close: session.closeThreadBrowser, modal: true, onTab: { fieldRequests += 1 },
                onFocusFree: session.refocusChannel)
            ThreadBrowserView(browser: browser, session: session, fieldRequests: fieldRequests)
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

/// The shield under a browser, with its keys: a click outside the popout closes it, a click on
/// the modal's dimmed page closes the modal, and neither reaches what's behind. Escape closes
/// either and goes no further; Tab stays inside, as in Discord's focus lock; a modal keeps typing
/// from what's behind it. Once it's closed, `onFocusFree` gets the focus nothing else took.
struct ThreadBrowserEvents: NSViewRepresentable {
    let close: () -> Void
    var modal = false
    /// Tab or Shift+Tab: the browser's field takes the focus, its only stop.
    var onTab: () -> Void = {}
    var onFocusFree: () -> Void = {}

    func makeNSView(context: Context) -> Watcher {
        Watcher()
    }

    func updateNSView(_ watcher: Watcher, context: Context) {
        watcher.close = close
        watcher.modal = modal
        watcher.onTab = onTab
        watcher.onFocusFree = onFocusFree
    }

    static func dismantleNSView(_ watcher: Watcher, coordinator: ()) {
        watcher.stop()
    }

    final class Watcher: PointerShield {
        var close: () -> Void = {}
        var modal = false
        var onTab: () -> Void = {}
        var onFocusFree: () -> Void = {}
        private var monitor: Any?
        private var pressed = false

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
            monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                self?.handle(event) ?? event
            }
        }

        func stop() {
            if let monitor {
                NSEvent.removeMonitor(monitor)
            }
            monitor = nil
        }

        // Discord's popouts close on the release of a press outside them, whichever button;
        // its modals on a click on the dimmed page.
        override func mouseDown(with event: NSEvent) {
            pressed = true
        }

        override func rightMouseDown(with event: NSEvent) {
            pressed = !modal
        }

        override func otherMouseDown(with event: NSEvent) {
            pressed = !modal
        }

        override func mouseUp(with event: NSEvent) {
            release(event)
        }

        override func rightMouseUp(with event: NSEvent) {
            release(event)
        }

        override func otherMouseUp(with event: NSEvent) {
            release(event)
        }

        private func release(_ event: NSEvent) {
            defer { pressed = false }
            if pressed, !modal || isShielded(at: event.locationInWindow) {
                close()
            }
        }

        // The Escape and Tab keys' codes on every layout.
        private static let escape: UInt16 = 53
        private static let tab: UInt16 = 48

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
            guard event.window === window, event.type == .keyDown else {
                return event
            }
            if event.keyCode == Self.escape {
                // The input method's, to cancel what it composes.
                if let text = window?.firstResponder as? NSTextView, text.hasMarkedText() {
                    return event
                }
                close()
                return nil
            }
            if event.keyCode == Self.tab, !event.modifierFlags.contains(.command) {
                onTab()
                return nil
            }
            if modal, !event.modifierFlags.contains(.command),
                !Self.isField(window?.firstResponder)
            {
                return nil
            }
            return event
        }
    }
}
