import AkariKit
import AppKit
import SwiftUI

struct ComposerView: View {
    let composer: ComposerModel
    let placeholder: String
    /// The message area's height; the composer grows to half of it.
    let areaHeight: CGFloat
    var onSend: () -> Void = {}
    var onEscape: () -> Void = {}
    @State private var height: CGFloat = 56

    var body: some View {
        VStack(spacing: 0) {
            ZStack(alignment: .topLeading) {
                ComposerField(
                    text: Bindable(composer).draft, enabled: composer.canSend,
                    maxHeight: areaHeight / 2, height: $height,
                    onSubmit: {
                        onSend()
                        Task { await composer.submit() }
                    },
                    onEscape: onEscape)
                if composer.draft.isEmpty {
                    Text(
                        composer.canSend ? placeholder : "You can't send messages in this channel."
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
            .opacity(composer.canSend ? 1 : 0.5)
            ComposerStatus(composer: composer)
        }
        .padding(.horizontal, 8)
    }

    @ViewBuilder private var counter: some View {
        if let remaining = composer.remaining {
            Text(remaining.formatted())
                .font(.system(size: 12))
                .monospacedDigit()
                .foregroundStyle(Color(nsColor: remaining < 0 ? Palette.danger : Palette.textMuted))
                .padding(.trailing, 12)
                .padding(.bottom, 6)
        }
    }

    static func placeholder(guildChannel: Bool, name: String) -> String {
        guildChannel ? "Write a message in #\(name)" : "Write a message to \(name)"
    }
}

struct ComposerField: NSViewRepresentable {
    @Binding var text: String
    let enabled: Bool
    let maxHeight: CGFloat
    @Binding var height: CGFloat
    let onSubmit: () -> Void
    let onEscape: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let (scrollView, textView) = ComposerTextView.scrollable()
        textView.string = text
        context.coordinator.attach(textView)
        // The composer takes focus when a channel opens, as in Discord.
        DispatchQueue.main.async {
            if let window = textView.window, window.isKeyWindow,
                !(window.firstResponder is NSTextView)
            {
                window.makeFirstResponder(textView)
            }
        }
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let textView = context.coordinator.textView else {
            return
        }
        if textView.string != text {
            textView.string = text
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

        init(_ parent: ComposerField) {
            self.parent = parent
        }

        func attach(_ textView: ComposerTextView) {
            self.textView = textView
            textView.delegate = self
            // The height depends on the width, which the view only gets in layout.
            textView.postsFrameChangedNotifications = true
            NotificationCenter.default.addObserver(
                self, selector: #selector(resized), name: NSView.frameDidChangeNotification,
                object: textView)
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
