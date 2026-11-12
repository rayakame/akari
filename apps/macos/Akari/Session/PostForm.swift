import AkariKit
import AppKit
import SwiftUI

/// The forum's header row: the search field with "New Post", which turns into the form for a
/// new post (the title, the message, the tags, Post); the status row under it while a search
/// runs or answered.
struct ForumHeaderRow: View {
    let forum: ForumModel
    let guidelines: (() -> Void)?
    var submit: () -> Void = {}
    var shiftEscape: () -> Void = {}
    /// Half the page at most for the message, as the channel composer.
    var messageMaxHeight: CGFloat = 300
    @State private var query = ""
    @State private var messageHeight: CGFloat = PostFormLayout.messageMin
    @FocusState private var focus: Field?

    enum Field {
        case title, message
    }

    private var form: PostFormModel { forum.form }

    /// What the empty field says: Discord's "Search or create a post..." where the user may
    /// post, "Title" in the open form.
    static func placeholder(_ form: PostFormModel) -> String {
        if form.isOpen {
            return PostFormModel.titlePlaceholder
        }
        return form.canCreate ? PostFormModel.rowPlaceholder : "Search"
    }

    var body: some View {
        VStack(spacing: 0) {
            VStack(spacing: 0) {
                titleRow
                if form.isOpen {
                    openForm
                }
            }
            .padding(8)
            .background(Color(nsColor: Palette.inputBackground))
            if let status {
                Rectangle()
                    .fill(Color(nsColor: Palette.borderSubtle))
                    .frame(height: 1)
                statusRow(status)
            }
        }
        .background(Color(nsColor: Palette.panel))
        .clipShape(RoundedRectangle(cornerRadius: ForumMeasures.radius))
        .overlay {
            RoundedRectangle(cornerRadius: ForumMeasures.radius)
                .strokeBorder(
                    Color(nsColor: focus != nil ? Palette.brand : Palette.borderSubtle), lineWidth: 1)
        }
        .onChange(of: query) { _, text in
            typed(text)
        }
        .onChange(of: forum.query) { _, _ in follow() }
        .onChange(of: form.title) { _, _ in follow() }
        .onChange(of: form.isOpen) { _, open in
            follow()
            if open {
                focus = form.title.isEmpty ? .title : .message
            }
        }
        .onAppear { follow() }
        .alert(
            form.alert?.title ?? "",
            isPresented: Binding(
                get: { form.alert != nil }, set: { if !$0 { form.dismissAlert() } }),
            presenting: form.alert
        ) { _ in
            Button("Okay") { form.dismissAlert() }
        } message: { alert in
            Text(alert.message)
        }
    }

    // The field is the search's while the form is closed and the title's while it's open.
    private func typed(_ text: String) {
        if form.isOpen {
            form.title = text
        } else {
            forum.setQuery(text)
        }
        follow()
    }

    private func follow() {
        let shown = form.isOpen ? form.title : forum.query
        if shown != query {
            query = shown
        }
    }

    private var status: String? {
        if let status = forum.status {
            return status
        }
        return !form.allowed && focus == .title ? PostFormModel.noPermission : nil
    }

    private var titleRow: some View {
        HStack(alignment: .top, spacing: 0) {
            prefix
                .frame(width: ForumMeasures.control, height: ForumMeasures.control)
                .padding(.trailing, 8)
            VStack(alignment: .leading, spacing: 0) {
                TextField(
                    "", text: $query,
                    prompt: Text(Self.placeholder(form))
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                )
                .textFieldStyle(.plain)
                .font(TypeScale.forumField.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .focused($focus, equals: .title)
                .frame(minHeight: ForumMeasures.control)
                .onKeyPress(.return, phases: .down) { press in
                    returnPressed(shift: press.modifiers.contains(.shift))
                }
                .onExitCommand(perform: escape)
                .accessibilityLabel(Self.placeholder(form))
                if form.isOpen, let error = form.titleError {
                    PostFormError(text: error)
                }
            }
            .padding(.trailing, 12)
            if !form.isOpen {
                if let guidelines {
                    GuidelinesButton(action: guidelines)
                        .padding(.trailing, 8)
                }
                NewPostButton(title: PostFormModel.newPost, enabled: form.canCreate) {
                    forum.openForm()
                }
                .help(form.isMedia ? PostFormModel.needsUploads : "")
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            // As the official client: a click on the row with a draft reopens the form.
            if !form.isOpen, form.hasDraft {
                forum.openForm()
            } else {
                focus = .title
            }
        }
    }

    @ViewBuilder private var prefix: some View {
        if form.isOpen {
            Button(action: form.discard) {
                Image(systemName: "xmark")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .frame(width: ForumMeasures.control, height: ForumMeasures.control)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .background(CursorArea(.pointingHand))
            .help("Clear")
            .accessibilityLabel("Clear")
        } else if forum.search == .searching {
            ProgressView().controlSize(.small)
                .accessibilityHidden(true)
        } else {
            Image(systemName: form.hasDraft ? "pencil" : "magnifyingglass")
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Color(nsColor: Palette.interactiveText))
                .accessibilityHidden(true)
        }
    }

    private var openForm: some View {
        VStack(alignment: .leading, spacing: 0) {
            PostFormMessage(
                form: form, maxHeight: messageMaxHeight, height: $messageHeight,
                focused: $focus, submit: submit, escape: escape, shiftEscape: shiftEscape)
            if let error = form.messageError {
                PostFormError(text: error)
            }
            if !form.usableTags.isEmpty {
                PostFormTags(form: form)
                    .padding(.top, PostFormLayout.tagsAbove)
                    .padding(.bottom, PostFormLayout.tagsBelow)
                Rectangle()
                    .fill(Color(nsColor: Palette.borderSubtle))
                    .frame(height: 1)
                    .padding(.horizontal, -8)
            }
            HStack(spacing: 8) {
                Spacer(minLength: 0)
                if let slowmode = form.slowmode {
                    PostFormSlowmode(slowmode: slowmode)
                }
                if let guidelines {
                    GuidelinesButton(action: guidelines)
                }
                NewPostButton(
                    title: PostFormModel.post, enabled: form.canPost, busy: form.submitting,
                    action: submit)
            }
            .frame(height: PostFormLayout.controls)
            .padding(.top, PostFormLayout.controlsAbove)
            if form.tagMissing {
                Text(PostFormModel.tagRequired)
                    .font(TypeScale.formAlert.font)
                    .foregroundStyle(Color(nsColor: Palette.danger))
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.top, 8)
            }
        }
        .padding(.horizontal, 8)
    }

    private func statusRow(_ status: String) -> some View {
        HStack(spacing: 8) {
            Text(status)
                .font(TypeScale.forumStatus.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .lineLimit(1)
            Spacer(minLength: 0)
            if forum.search == .found {
                Button("Clear") {
                    query = ""
                    forum.clearSearch()
                }
                .buttonStyle(.plain)
                .font(TypeScale.forumStatusAction.font)
                .foregroundStyle(Color(nsColor: Palette.textLink))
                .background(CursorArea(.pointingHand))
            }
        }
        .padding(.horizontal, 12)
        .frame(height: ForumMeasures.control)
        .background(Color(nsColor: Palette.inputBackground))
    }

    // Shift+Enter opens the form; in the open form Enter goes on to the message. A search runs
    // no sooner for Enter.
    private func returnPressed(shift: Bool) -> KeyPress.Result {
        if form.isOpen {
            if !form.title.trimmingCharacters(in: .whitespaces).isEmpty {
                focus = .message
            }
            return .handled
        }
        guard shift, form.canCreate else {
            return .ignored
        }
        forum.openForm()
        return .handled
    }

    private func escape() {
        if NSApp.currentEvent?.modifierFlags.contains(.shift) == true {
            shiftEscape()
        } else if form.isOpen {
            form.close()
        } else {
            query = ""
        }
    }
}

enum PostFormLayout {
    static let messageMin: CGFloat = 80
    static let tagsAbove: CGFloat = 16
    static let tagsBelow: CGFloat = 12
    static let controls: CGFloat = 40
    static let controlsAbove: CGFloat = 12
}

/// "New Post" and "Post": Discord's small primary button with its bubble, a spinner while the
/// post is made.
struct NewPostButton: View {
    let title: String
    var enabled = true
    var busy = false
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if busy {
                    ProgressView().controlSize(.small)
                } else {
                    Image(systemName: "plus.bubble.fill")
                        .font(.system(size: 13, weight: .semibold))
                }
                Text(title)
                    .font(TypeScale.formAlert.font)
            }
            .foregroundStyle(.white)
            .padding(.horizontal, 12)
            .frame(height: ForumMeasures.control)
            .background(
                Color(nsColor: hovered && enabled ? Palette.brandHover : Palette.brand),
                in: RoundedRectangle(cornerRadius: 8))
            .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .disabled(!enabled || busy)
        .opacity(enabled ? 1 : 0.5)
        .onHover { hovered = $0 }
        .background(CursorArea(enabled && !busy ? .pointingHand : .arrow))
        .accessibilityLabel(title)
    }
}

/// The post's first message: the channel composer's field, starting as the forum's template.
struct PostFormMessage: View {
    let form: PostFormModel
    let maxHeight: CGFloat
    @Binding var height: CGFloat
    var focused: FocusState<ForumHeaderRow.Field?>.Binding
    let submit: () -> Void
    let escape: () -> Void
    let shiftEscape: () -> Void
    @State private var textView: ComposerTextView?

    var body: some View {
        ZStack(alignment: .topLeading) {
            ComposerField(
                text: Bindable(form).message, enabled: !form.submitting, maxHeight: maxHeight,
                height: $height, onSubmit: submit, onEscape: escape, onShiftEscape: shiftEscape,
                onAttach: { textView = $0 }, autoFocus: false,
                focusNow: focused.wrappedValue == .message,
                onFocused: {})
                .padding(.trailing, form.remaining == nil ? 0 : 48)
            if form.message.isEmpty {
                Text(PostFormModel.messagePlaceholder)
                    .font(TypeScale.body.font)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .lineLimit(1)
                    .padding(.leading, ComposerTextView.insets.width)
                    .padding(.top, ComposerTextView.insets.height + 2)
                    .allowsHitTesting(false)
            }
        }
        .frame(height: max(height, PostFormLayout.messageMin), alignment: .top)
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
                    .padding(8)
            }
        }
        .padding(.top, 4)
    }
}

/// The tags the user may apply, as small pills that toggle; a sixth is off once five are chosen.
/// They wrap onto more rows where Discord has "See more tags".
struct PostFormTags: View {
    let form: PostFormModel

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "tag.fill")
                .font(.system(size: 12))
                .foregroundStyle(Color(nsColor: Palette.interactiveText))
                .frame(width: 16, height: 24)
                .accessibilityHidden(true)
            FlowLayout(spacing: 4) {
                ForEach(form.usableTags, id: \.id) { tag in
                    PostFormTag(
                        tag: tag, chosen: form.tags.contains(tag.id),
                        enabled: form.canChoose(tag.id)
                    ) {
                        form.toggle(tag: tag.id)
                    }
                }
            }
        }
    }
}

struct PostFormTag: View {
    let tag: ForumTag
    let chosen: Bool
    let enabled: Bool
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            ForumTagPill(tag: tag, size: .small, look: chosen ? .selected : .idle, hovered: hovered)
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.5)
        .onHover { hovered = $0 }
        .background(CursorArea(enabled ? .pointingHand : .arrow))
        .accessibilityLabel("Add tag, \(tag.name)")
        .accessibilityAddTraits(chosen ? .isSelected : [])
    }
}

/// Rows of views, each row as many as fit.
struct FlowLayout: Layout {
    var spacing: CGFloat = 4

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = rows(subviews, width: proposal.width ?? .infinity)
        let height = rows.map(\.height).reduce(0, +) + spacing * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(
        in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
    ) {
        var y = bounds.minY
        for row in rows(subviews, width: bounds.width) {
            var x = bounds.minX
            for index in row.indexes {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: .unspecified)
                x += size.width + spacing
            }
            y += row.height + spacing
        }
    }

    private func rows(_ subviews: Subviews, width: CGFloat) -> [(indexes: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(indexes: [Int], width: CGFloat, height: CGFloat)] = []
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            if let last = rows.indices.last, rows[last].width + spacing + size.width <= width {
                rows[last].indexes.append(index)
                rows[last].width += spacing + size.width
                rows[last].height = max(rows[last].height, size.height)
            } else {
                rows.append(([index], size.width, size.height))
            }
        }
        return rows
    }
}

/// The forum's slowmode for new posts in the controls row: the countdown while it runs.
struct PostFormSlowmode: View {
    let slowmode: Slowmode

    var body: some View {
        HStack(spacing: 4) {
            Image(systemName: "timer")
                .font(.system(size: 11, weight: .medium))
            if let until = slowmode.until, until > Date(), !slowmode.exempt {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    Text(ComposerStatus.countdown(max(0, until.timeIntervalSince(context.date))))
                        .monospacedDigit()
                }
            } else {
                Text(ThreadFormStrip.text(slowmode))
            }
        }
        .font(TypeScale.formStrip.font)
        .foregroundStyle(Color(nsColor: Palette.textMuted))
        .help(Self.help(slowmode))
    }

    /// Discord's tooltip for a forum: "Members can create one post every …".
    static func help(_ slowmode: Slowmode) -> String {
        let seconds = Int(slowmode.interval)
        let (value, unit) =
            seconds % 3600 == 0
            ? (seconds / 3600, "hour")
            : seconds % 60 == 0 ? (seconds / 60, "minute") : (seconds, "second")
        return "Slowmode is enabled. Members can create one post every \(value) "
            + (value == 1 ? unit : "\(unit)s") + "."
    }
}

/// A field's error under it: the mark and Discord's sentence in the danger color.
struct PostFormError: View {
    let text: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 4) {
            Image(systemName: "exclamationmark.circle.fill")
                .font(.system(size: 11))
            Text(text)
                .font(TypeScale.forumStatus.font)
                .fixedSize(horizontal: false, vertical: true)
        }
        .foregroundStyle(Color(nsColor: Palette.danger))
        .padding(.top, 8)
        .accessibilityElement(children: .combine)
    }
}
