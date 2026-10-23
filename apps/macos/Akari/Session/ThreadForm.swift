import AkariKit
import AppKit
import SwiftUI

// Discord's form, canary 649224 (module 595840 and its CSS), in Akari's colors.
enum ThreadFormLayout {
    static let margin: CGFloat = 16
    static let circle: CGFloat = 64
    static let circleIcon: CGFloat = 32
    static let fieldGap: CGFloat = 24
    static let labelGap: CGFloat = 8
    static let fieldHeight: CGFloat = 40
    static let fieldPadding: CGFloat = 12
    static let radius: CGFloat = 8
    static let noteGap: CGFloat = 4
    static let checkbox: CGFloat = 20
    static let checkboxGap: CGFloat = 12
    static let previewBottom: CGFloat = 24
    static let stripHeight: CGFloat = 24
}

/// The "New Thread" form in the split view's place: its header, the fields at the bottom of a
/// scroller, the source message's preview, and the first message's composer.
struct ThreadFormView: View {
    static let closeQuestion = "Are you sure you want to go?"
    static let closeDetail = "Any changes you made will be lost if you exit now."

    let form: ThreadFormModel
    let session: SessionModel
    @State private var asking = false

    var body: some View {
        VStack(spacing: 0) {
            // Each dialog on a view of its own: two on one view can lose one.
            ThreadFormHeader(close: close)
                .alert(Self.closeQuestion, isPresented: $asking) {
                    Button("Exit", role: .destructive) { session.discardThreadForm() }
                    Button("Back to editing", role: .cancel) {}
                } message: {
                    Text(Self.closeDetail)
                }
            GeometryReader { proxy in
                ScrollView(.vertical) {
                    VStack(alignment: .leading, spacing: 0) {
                        Spacer(minLength: 0)
                        fields
                            .padding(ThreadFormLayout.margin)
                        if let source = form.source {
                            MessagePreview(message: source)
                                .padding(.bottom, ThreadFormLayout.previewBottom)
                        }
                    }
                    .frame(width: proxy.size.width)
                    .frame(minHeight: proxy.size.height, alignment: .bottom)
                }
                .defaultScrollAnchor(.bottom)
            }
            submitArea
        }
        .alert(
            form.alert?.title ?? "",
            isPresented: Binding(
                get: { form.alert != nil }, set: { if !$0 { form.dismissAlert() } }),
            presenting: form.alert
        ) { _ in
            Button("OK") { form.dismissAlert() }
        } message: { alert in
            Text(alert.message)
        }
    }

    private func close() {
        if form.hasText {
            asking = true
        } else {
            session.discardThreadForm()
        }
    }

    private func submit() {
        Task { await session.submitThreadForm() }
    }

    private var fields: some View {
        VStack(alignment: .leading, spacing: 0) {
            ThreadFormIcon(isPrivate: form.createsPrivate)
                .padding(.vertical, ThreadFormLayout.margin)
            VStack(alignment: .leading, spacing: ThreadFormLayout.fieldGap) {
                nameField
                if form.privateMode != .disabled {
                    privateField
                }
            }
        }
    }

    private var nameField: some View {
        VStack(alignment: .leading, spacing: 0) {
            ThreadFormLabel(text: form.nameLabel)
            ThreadNameField(
                text: Bindable(form).name, placeholder: form.namePlaceholder,
                failed: form.nameError != nil, focusNow: session.focusThreadForm,
                onFocused: session.threadFormFocused, submit: submit,
                escape: { session.escape() })
            if let error = form.nameError {
                ThreadFormNote(text: error, error: true)
            }
        }
    }

    private var privateField: some View {
        VStack(alignment: .leading, spacing: 0) {
            ThreadFormLabel(text: ThreadFormModel.privateLabel)
            ThreadFormCheckbox(
                isOn: Binding(
                    get: { form.createsPrivate }, set: { form.isPrivate = $0 }),
                enabled: form.privateMode == .enabled, label: ThreadFormModel.privateDetail)
            if form.createsPrivate {
                ThreadFormNote(text: ThreadFormModel.privateHint, error: false)
            }
        }
    }

    private var submitArea: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let error = form.messageError {
                ThreadStarterError(text: error)
            }
            if let slowmode = form.slowmode {
                ThreadFormStrip(slowmode: slowmode)
            }
            ThreadFormComposer(
                form: form, submit: submit, escape: { session.escape() },
                markServerRead: session.markServerRead)
        }
    }
}

struct ThreadFormHeader: View {
    static let title = ThreadFormModel.title

    let close: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            Image(systemName: ChannelSymbols.name(for: .publicThread))
                .font(.system(size: 18))
                .foregroundStyle(Color(nsColor: Palette.channelDefault))
                .opacity(0.6)
                .frame(width: 20, height: 20)
                .padding(.leading, -1)
                .padding(.trailing, 9)
                .accessibilityHidden(true)
            Text(Self.title)
                .font(TypeScale.formTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .lineLimit(1)
                .accessibilityAddTraits(.isHeader)
            Spacer(minLength: 8)
            HeaderIconButton(symbol: "xmark", label: "Close", action: close)
                .padding(.leading, 8)
        }
        .padding(.leading, 16)
        .padding(.trailing, 8)
        .frame(height: ChannelHeader.height)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: Palette.borderSubtle))
                .frame(height: 1)
                .offset(y: 1)
        }
        .padding(.bottom, 1)
    }
}

struct ThreadFormIcon: View {
    let isPrivate: Bool

    var body: some View {
        Circle()
            .fill(Color(nsColor: Palette.backgroundModStrong))
            .frame(width: ThreadFormLayout.circle, height: ThreadFormLayout.circle)
            .overlay {
                Image(systemName: ChannelSymbols.name(for: .publicThread))
                    .font(.system(size: 26))
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .frame(width: ThreadFormLayout.circleIcon, height: ThreadFormLayout.circleIcon)
                    .overlay(alignment: .bottomTrailing) {
                        if isPrivate {
                            Image(systemName: "lock.fill")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(Color(nsColor: Palette.interactiveText))
                                .offset(x: 4, y: 2)
                        }
                    }
            }
            .accessibilityHidden(true)
    }
}

struct ThreadFormLabel: View {
    let text: String

    var body: some View {
        Text(text)
            .font(TypeScale.formTitle.font)
            .foregroundStyle(Color(nsColor: Palette.textStrong))
            .padding(.bottom, ThreadFormLayout.labelGap)
    }
}

/// Under a field: its error, or the private checkbox's hint.
struct ThreadFormNote: View {
    let text: String
    let error: Bool

    var body: some View {
        HStack(alignment: .top, spacing: 4) {
            if error {
                Image(systemName: "exclamationmark.circle.fill")
                    .font(.system(size: 13))
                    .frame(width: 16, height: 16)
                    .accessibilityHidden(true)
            }
            Text(text)
                .font(TypeScale.formNote.font)
                .fixedSize(horizontal: false, vertical: true)
        }
        .foregroundStyle(Color(nsColor: error ? Palette.textError : Palette.interactiveText))
        .padding(.top, ThreadFormLayout.noteGap)
    }
}

struct ThreadStarterError: View {
    let text: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "exclamationmark.circle.fill")
                .font(.system(size: 13))
                .frame(width: 16, height: 16)
                .accessibilityHidden(true)
            Text(text)
                .font(TypeScale.formAlert.font)
        }
        .foregroundStyle(Color(nsColor: Palette.textError))
        .padding(.leading, 16)
        .padding(.top, 8)
        .padding(.bottom, 8)
    }
}

/// The parent's slowmode for new threads, on the right above the composer.
struct ThreadFormStrip: View {
    let slowmode: Slowmode

    var body: some View {
        HStack(spacing: 4) {
            Spacer(minLength: 0)
            Image(systemName: "timer")
                .font(.system(size: 11, weight: .medium))
            if let until = slowmode.until, until > Date(), !slowmode.exempt {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text(ComposerStatus.countdown(max(0, until.timeIntervalSince(context.date))))
                        .monospacedDigit()
                }
            } else {
                Text(Self.text(slowmode))
            }
        }
        .font(TypeScale.formStrip.font)
        .foregroundStyle(Color(nsColor: Palette.textMuted))
        .padding(.trailing, 16)
        .frame(height: ThreadFormLayout.stripHeight)
        .help(Self.help(slowmode))
    }

    static func text(_ slowmode: Slowmode) -> String {
        slowmode.exempt ? "Slowmode Immune" : "Slowmode is enabled."
    }

    static func help(_ slowmode: Slowmode) -> String {
        let seconds = Int(slowmode.interval)
        let (value, unit) =
            seconds % 3600 == 0
            ? (seconds / 3600, "hour")
            : seconds % 60 == 0 ? (seconds / 60, "minute") : (seconds, "second")
        return "Slowmode is enabled. Members can send one message every \(value) "
            + (value == 1 ? unit : "\(unit)s")
    }
}

/// The first message's composer: the channel composer's field and look, with Discord's
/// placeholder and a red border while the message is missing.
struct ThreadFormComposer: View {
    let form: ThreadFormModel
    let submit: () -> Void
    let escape: () -> Void
    let markServerRead: () -> Void
    @State private var height: CGFloat = 56

    var body: some View {
        ZStack(alignment: .topLeading) {
            ComposerField(
                text: Bindable(form).message, enabled: true, maxHeight: 300, height: $height,
                onSubmit: submit, onEscape: escape, onShiftEscape: markServerRead,
                autoFocus: false)
                .padding(.trailing, form.remaining == nil ? 0 : 48)
            if form.message.isEmpty {
                Text(ThreadFormModel.placeholder)
                    .font(TypeScale.body.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .lineLimit(1)
                    .padding(.leading, ComposerTextView.insets.width)
                    .padding(.top, ComposerTextView.insets.height + 2)
                    .allowsHitTesting(false)
            }
        }
        .frame(height: height)
        .background(Color(nsColor: Palette.composer), in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            if form.messageError != nil {
                RoundedRectangle(cornerRadius: 8)
                    .strokeBorder(Color(nsColor: Palette.danger), lineWidth: 1)
            }
        }
        .overlay(alignment: .bottomTrailing) {
            if let remaining = form.remaining {
                Text(remaining.formatted())
                    .font(TypeScale.small.font)
                    .monospacedDigit()
                    .foregroundStyle(
                        Color(nsColor: remaining < 0 ? Palette.danger : Palette.textMuted))
                    .padding(.trailing, 14)
                    .padding(.bottom, 20)
            }
        }
        .padding([.horizontal, .bottom], 8)
    }
}

/// Discord's checkbox: a 20-point box, its label 12 beside it.
struct ThreadFormCheckbox: View {
    @Binding var isOn: Bool
    let enabled: Bool
    let label: String
    @State private var hovered = false

    var body: some View {
        Button {
            isOn.toggle()
        } label: {
            HStack(alignment: .center, spacing: ThreadFormLayout.checkboxGap) {
                box
                Text(label)
                    .font(TypeScale.formText.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .frame(minHeight: ThreadFormLayout.checkbox)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.5)
        .onHover { hovered = $0 }
        .background(CursorArea(enabled ? .pointingHand : .arrow))
        .accessibilityRepresentation {
            Toggle(label, isOn: $isOn).disabled(!enabled)
        }
    }

    private var box: some View {
        RoundedRectangle(cornerRadius: 4)
            .fill(
                Color(
                    nsColor: isOn
                        ? (hovered && enabled ? Palette.brandHover : Palette.brand)
                        : Palette.inputBackground)
            )
            .overlay {
                RoundedRectangle(cornerRadius: 4)
                    .strokeBorder(
                        Color(
                            nsColor: isOn
                                ? Palette.borderSubtle
                                : hovered && enabled
                                    ? Palette.checkboxBorderHover : Palette.checkboxBorder),
                        lineWidth: 1)
            }
            .overlay {
                if isOn {
                    Image(systemName: "checkmark")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
            .frame(width: ThreadFormLayout.checkbox, height: ThreadFormLayout.checkbox)
    }
}

/// The name field: AppKit's own editing in Discord's box. Return submits the form, Escape
/// closes it.
struct ThreadNameField: View {
    @Binding var text: String
    let placeholder: String
    let failed: Bool
    var focusNow = false
    var onFocused: () -> Void = {}
    let submit: () -> Void
    let escape: () -> Void
    @State private var editing = false

    var body: some View {
        Representable(field: self, editing: $editing)
            .padding(.horizontal, ThreadFormLayout.fieldPadding)
            .frame(height: ThreadFormLayout.fieldHeight)
            .background(
                Color(nsColor: failed ? Palette.inputErrorTint : Palette.inputBackground),
                in: RoundedRectangle(cornerRadius: ThreadFormLayout.radius))
            .overlay {
                RoundedRectangle(cornerRadius: ThreadFormLayout.radius)
                    .strokeBorder(Color(nsColor: borderColor), lineWidth: editing ? 2 : 1)
            }
            .accessibilityElement(children: .contain)
    }

    private var borderColor: NSColor {
        failed ? Palette.textError : editing ? Palette.brand : Palette.borderNormal
    }

    struct Representable: NSViewRepresentable {
        let field: ThreadNameField
        @Binding var editing: Bool

        func makeCoordinator() -> Coordinator {
            Coordinator(self)
        }

        func makeNSView(context: Context) -> NameTextField {
            let textField = NameTextField()
            textField.onWindow = { [weak coordinator = context.coordinator] in
                coordinator?.focusIfAsked()
            }
            textField.isBordered = false
            textField.drawsBackground = false
            textField.focusRingType = .none
            textField.font = TypeScale.formText.nsFont
            textField.textColor = Palette.textDefault
            textField.lineBreakMode = .byClipping
            textField.cell?.isScrollable = true
            textField.cell?.wraps = false
            textField.delegate = context.coordinator
            textField.stringValue = field.text
            context.coordinator.textField = textField
            return textField
        }

        func updateNSView(_ textField: NameTextField, context: Context) {
            context.coordinator.parent = self
            if textField.stringValue != field.text {
                textField.stringValue = field.text
            }
            textField.placeholderAttributedString = NSAttributedString(
                string: field.placeholder,
                attributes: [
                    .font: TypeScale.formText.nsFont, .foregroundColor: Palette.textMuted,
                ])
            textField.setAccessibilityLabel(field.placeholder)
            context.coordinator.focusIfAsked()
        }

        final class Coordinator: NSObject, NSTextFieldDelegate {
            var parent: Representable
            weak var textField: NSTextField?
            private var focusAsked = false

            init(_ parent: Representable) {
                self.parent = parent
            }

            // Once per request, also when the field joins its window after the request.
            func focusIfAsked() {
                guard parent.field.focusNow else {
                    focusAsked = false
                    return
                }
                guard !focusAsked, let textField, let window = textField.window else {
                    return
                }
                focusAsked = true
                let focused = parent.field.onFocused
                DispatchQueue.main.async { [weak textField] in
                    guard let textField, textField.window === window else {
                        return
                    }
                    window.makeFirstResponder(textField)
                    focused()
                }
            }

            func controlTextDidChange(_ notification: Notification) {
                guard let textField = notification.object as? NSTextField else {
                    return
                }
                parent.field.text = textField.stringValue
            }

            func controlTextDidBeginEditing(_ notification: Notification) {
                parent.editing = true
            }

            func controlTextDidEndEditing(_ notification: Notification) {
                parent.editing = false
            }

            func control(
                _ control: NSControl, textView: NSTextView, doCommandBy selector: Selector
            ) -> Bool {
                switch selector {
                case #selector(NSResponder.insertNewline(_:)):
                    // Return during input-method composition commits it.
                    guard !textView.hasMarkedText() else {
                        return false
                    }
                    parent.field.submit()
                    return true
                case #selector(NSResponder.cancelOperation(_:)):
                    guard !textView.hasMarkedText() else {
                        return false
                    }
                    parent.field.escape()
                    return true
                default:
                    return false
                }
            }
        }
    }
}

final class NameTextField: NSTextField {
    var onWindow: () -> Void = {}

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        onWindow()
    }
}

/// The message a thread starts from, as the list draws it.
struct MessagePreview: NSViewRepresentable {
    let message: Message
    var now = Date()

    func makeNSView(context: Context) -> MessageCell {
        MessageCell()
    }

    func updateNSView(_ cell: MessageCell, context: Context) {
        let calendar = Calendar.current
        let locale = Locale.current
        cell.configure(
            message, body: text, startsGroup: true,
            groupTime: MessageFormat.groupTime(
                message.timestamp, now: now, calendar: calendar, locale: locale),
            shortTime: MessageFormat.shortTime(message.timestamp, calendar: calendar, locale: locale),
            fullDate: MessageFormat.fullDate(message.timestamp, calendar: calendar, locale: locale))
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: MessageCell, context: Context)
        -> CGSize?
    {
        let width = proposal.width ?? SplitWidth.standard
        return CGSize(
            width: width,
            height: MessageCell.height(message, body: text, startsGroup: true, width: width))
    }

    private var text: NSAttributedString {
        MessageText.build(
            message, revealed: [], now: now,
            times: TimestampText(locale: .current, timeZone: .current, calendar: .current)
        ).text
    }
}
