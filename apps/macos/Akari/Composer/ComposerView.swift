import AkariKit
import AppKit
import SwiftUI

struct ComposerView: View {
    let composer: ComposerModel
    let placeholder: String
    // The composer grows to half of it.
    let areaHeight: CGFloat
    var onSend: () -> Void = {}
    var onEscape: () -> Void = {}
    var onAttach: (ComposerTextView) -> Void = { _ in }
    @State private var height: CGFloat = 56

    var body: some View {
        ZStack(alignment: .topLeading) {
            ComposerField(
                text: Bindable(composer).draft, enabled: !denied,
                maxHeight: areaHeight / 2, height: $height,
                onSubmit: {
                    Task { await composer.submit(willSend: onSend) }
                },
                onEscape: onEscape, onAttach: onAttach
            )
            // Room for the counter at the right edge.
            .padding(.trailing, composer.remaining == nil ? 0 : 48)
            if composer.draft.isEmpty {
                Text(
                    denied ? "You can't send messages in this channel." : placeholder
                )
                .font(.system(size: 16))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .lineLimit(1)
                .padding(.leading, ComposerTextView.insets.width)
                .padding(.top, ComposerTextView.insets.height + 2)
                .allowsHitTesting(false)
            }
        }
        .frame(height: height)
        .background(Color(nsColor: Palette.composer), in: RoundedRectangle(cornerRadius: 8))
        .overlay(alignment: .bottomTrailing) { counter }
        .opacity(denied ? 0.5 : 1)
        // The user panel's margins, so both bottom edges line up.
        .padding([.horizontal, .bottom], 8)
    }

    // Before the store knows the channel the user can already type.
    private var denied: Bool {
        composer.canSend == false
    }

    @ViewBuilder private var counter: some View {
        if let remaining = composer.remaining {
            Text(remaining.formatted())
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(Color(nsColor: remaining < 0 ? Palette.danger : Palette.textMuted))
                .padding(.trailing, 14)
                .padding(.bottom, 20)
        }
    }
}

struct ComposerField: NSViewRepresentable {
    @Binding var text: String
    let enabled: Bool
    let maxHeight: CGFloat
    @Binding var height: CGFloat
    let onSubmit: () -> Void
    let onEscape: () -> Void
    var onAttach: (ComposerTextView) -> Void = { _ in }

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let (scrollView, textView) = ComposerTextView.scrollable()
        textView.string = text
        context.coordinator.attach(textView)
        onAttach(textView)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = context.coordinator.textView else {
            return
        }
        if textView.string != text {
            context.coordinator.replaceText(with: text)
        }
        textView.isEditable = enabled
        textView.isSelectable = enabled
        textView.onSubmit = onSubmit
        textView.onEscape = onEscape
        context.coordinator.measure()
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerField
        weak var textView: ComposerTextView?
        // The window's shared undo manager would replay typing against text replaced in code.
        private let undo = UndoManager()
        nonisolated(unsafe) private var keyObserver: NSObjectProtocol?
        private let notifications: NotificationCenter

        init(_ parent: ComposerField, notifications: NotificationCenter = .default) {
            self.parent = parent
            self.notifications = notifications
        }

        // A composer can go away before its window ever becomes key, e.g. a channel switch in
        // the background; block observers aren't removed on their own.
        deinit {
            if let keyObserver {
                notifications.removeObserver(keyObserver)
            }
        }

        func attach(_ textView: ComposerTextView) {
            self.textView = textView
            textView.delegate = self
            textView.onWindow = { [weak self] in self?.focusWhenKey() }
            focusWhenKey()
            // The height depends on the width, which the view only gets in layout.
            textView.postsFrameChangedNotifications = true
            NotificationCenter.default.addObserver(
                self, selector: #selector(resized), name: NSView.frameDidChangeNotification,
                object: textView)
        }

        // A channel's composer takes focus when it opens, as in the official client; at launch
        // the window only becomes key after the restored channel's composer exists.
        private func focusWhenKey() {
            guard let textView, let window = textView.window else {
                return
            }
            if let keyObserver {
                notifications.removeObserver(keyObserver)
                self.keyObserver = nil
            }
            if window.isKeyWindow {
                DispatchQueue.main.async { [weak textView] in textView?.takeFocus() }
                return
            }
            keyObserver = notifications.addObserver(
                forName: NSWindow.didBecomeKeyNotification, object: window, queue: .main
            ) { [weak self] _ in
                MainActor.assumeIsolated {
                    guard let self, let keyObserver = self.keyObserver else {
                        return
                    }
                    self.notifications.removeObserver(keyObserver)
                    self.keyObserver = nil
                    self.textView?.takeFocus()
                }
            }
        }

        func replaceText(with text: String) {
            textView?.string = text
            undo.removeAllActions()
        }

        func undoManager(for view: NSTextView) -> UndoManager? {
            undo
        }

        @objc private func resized(_ notification: Notification) {
            measure()
        }

        func textDidChange(_ notification: Notification) {
            guard let textView else {
                return
            }
            parent.text = textView.string
            measure()
        }

        // Later, so SwiftUI isn't changed in the middle of an update.
        func measure() {
            // Without a width yet, every character would wrap onto a line of its own.
            guard let textView, textView.bounds.width > 2 * ComposerTextView.insets.width else {
                return
            }
            let height = textView.fittingHeight(maxHeight: parent.maxHeight)
            guard height != parent.height else {
                return
            }
            let binding = parent.$height
            DispatchQueue.main.async {
                binding.wrappedValue = height
            }
        }
    }
}
