import AkariKit
import AppKit
import SwiftUI

/// The Threads browser's measures, from Discord's CSS (canary 646904) and Chris's screenshots.
enum ThreadBrowserLayout {
    static let headerHeight: CGFloat = 48
    static let inset: CGFloat = 16
    static let iconBox: CGFloat = 24
    static let titleGap: CGFloat = 8
    static let dividerGap: CGFloat = 16
    static let closeGap: CGFloat = 8
    static let closeBox: CGFloat = 32
    static let labelHeight: CGFloat = 24
    static let labelGap: CGFloat = 8
    static let cardHeight: CGFloat = 72
    static let cardGap: CGFloat = 8
    static let resultsTop: CGFloat = 16
    static let resultsBottom: CGFloat = 8
    static let emptyHeight: CGFloat = 400
    // Discord loads the next page within 200 of the list's bottom.
    static let loadDistance: CGFloat = 200
    static let spinnerMargin: CGFloat = 64
    static let minHeight: CGFloat = 400
    static let popoutMinWidth: CGFloat = 480
    static let popoutMaxWidth: CGFloat = 600
    static let modalWidth: CGFloat = 680
    // The modal's room: 24 around it, and as much again as Discord's top bar above and below.
    static let modalMargin: CGFloat = TitleBar.height + 24
    // What Discord's field takes before sharing the rest with the spacer (both grow).
    static let searchBasis: CGFloat = 215

    static func popoutWidth(window: CGFloat) -> CGFloat {
        min(max(window * 0.35, popoutMinWidth), popoutMaxWidth)
    }

    /// Its header and list, 400 at least and 80% of the window at most; the minimum wins.
    static func popoutHeight(list: CGFloat, window: CGFloat) -> CGFloat {
        max(minHeight, min(headerHeight + list, window * 0.8))
    }

    /// With its 8 above and 16 below the browser and its border.
    static func modalSize(list: CGFloat, window: NSSize) -> NSSize {
        NSSize(
            width: min(modalWidth, window.width - 48),
            height: min(8 + headerHeight + list + 16 + 2, window.height - 2 * modalMargin))
    }

    /// The sections' list: 8 above the first label, each label 24 with 8 below, each card 72
    /// with 8 below, which is also the gap above the next label.
    static func listHeight(sections: [Int]) -> CGFloat {
        sections.reduce(labelGap) { height, rows in
            height + labelHeight + labelGap + CGFloat(rows) * (cardHeight + cardGap)
        }
    }

    static func listHeight(results: Int) -> CGFloat {
        resultsTop + CGFloat(results) * (cardHeight + cardGap) + resultsBottom
    }

    @MainActor
    static func listHeight(_ browser: ThreadBrowserModel) -> CGFloat {
        if let results = browser.results, !results.isEmpty {
            return listHeight(results: results.count)
        }
        if browser.isEmpty {
            return emptyHeight
        }
        if browser.showsSpinner {
            return 2 * spinnerMargin + 32
        }
        return listHeight(sections: browser.sections.map(\.rows.count))
    }

    static func searchWidth(free: CGFloat) -> CGFloat {
        min(free, (free + searchBasis) / 2)
    }
}

/// A channel's Threads browser: the header with the search field, and the sections, the
/// search's results, the spinner or the empty state.
struct ThreadBrowserView: View {
    static let emptyTitle = "There are no threads."
    static let emptyText = "Stay focused on a conversation with a thread - a temporary text channel."
    static let placeholder = "Search for Thread Name"

    let browser: ThreadBrowserModel
    let session: SessionModel
    /// Fixed for tests; otherwise the times follow the clock.
    var now: Date?
    /// Each change gives the search field the focus.
    var fieldRequests = 0

    var body: some View {
        VStack(spacing: 0) {
            ThreadBrowserHeader(
                browser: browser, close: session.closeThreadBrowser, fieldRequests: fieldRequests)
            // Never taller than the room left: the header stays in a small window.
            content
                .frame(maxWidth: .infinity, minHeight: 0, maxHeight: .infinity, alignment: .top)
                .clipped()
        }
        .task(id: browser.parentId) { await browser.open() }
    }

    @ViewBuilder private var content: some View {
        if let results = browser.results, !results.isEmpty {
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(results) { row in
                        card(row)
                    }
                }
                .padding(.top, ThreadBrowserLayout.resultsTop)
                .padding(.bottom, ThreadBrowserLayout.resultsBottom)
                .padding(.horizontal, ThreadBrowserLayout.inset)
            }
        } else if browser.showsSpinner {
            ProgressView()
                .padding(.vertical, ThreadBrowserLayout.spinnerMargin)
                .frame(maxWidth: .infinity)
        } else if browser.isEmpty {
            ThreadBrowserEmptyState()
        } else {
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(Array(browser.sections.enumerated()), id: \.element.id) { index, section in
                        Text(section.title)
                            .font(TypeScale.browserLabel.font)
                            .foregroundStyle(Color(nsColor: Palette.textDefault))
                            .lineLimit(1)
                            .frame(height: ThreadBrowserLayout.labelHeight)
                            .padding(.top, index == 0 ? ThreadBrowserLayout.labelGap : 0)
                            .padding(.bottom, ThreadBrowserLayout.labelGap)
                        ForEach(section.rows) { row in
                            card(row)
                        }
                    }
                }
                .padding(.horizontal, ThreadBrowserLayout.inset)
                .background {
                    ScrollEnd(distance: ThreadBrowserLayout.loadDistance) {
                        if browser.older == .more {
                            Task { await browser.loadMore() }
                        }
                    }
                }
            }
        }
    }

    private func card(_ row: ThreadBrowserModel.Row) -> some View {
        ThreadBrowserCard(row: row, now: now) { beside in
            Task { await session.open(browsed: row.id, beside: beside) }
        }
        .threadMenu(
            markReadEnabled: session.canMarkRead(channel: row.id),
            markRead: { session.markRead(channel: row.id) },
            membership: session.membershipItem(for: row.id),
            changeMembership: { changeMembership(of: row.id) })
    }

    // A row's menu says "Context Menu" to Discord, as the official client's does.
    private func changeMembership(of thread: ChannelId) {
        guard let item = session.membershipItem(for: thread) else {
            return
        }
        Task {
            if item.joins {
                await session.join(thread: thread, from: .contextMenu)
            } else {
                await session.leave(thread: thread, from: .contextMenu)
            }
        }
    }
}

struct ThreadBrowserHeader: View {
    let browser: ThreadBrowserModel
    let close: () -> Void
    var fieldRequests = 0
    @State private var query = ""
    // VoiceOver starts here when there's no field to start in.
    @AccessibilityFocusState private var titleSpoken: Bool

    private static let title = "Threads"

    var body: some View {
        GeometryReader { proxy in
            HStack(spacing: 0) {
                Image(systemName: ChannelSymbols.name(for: .publicThread))
                    .font(.system(size: 20))
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .frame(width: ThreadBrowserLayout.iconBox, height: ThreadBrowserLayout.iconBox)
                    .accessibilityHidden(true)
                Text(Self.title)
                    .font(TypeScale.browserTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .lineLimit(1)
                    .fixedSize()
                    .padding(.leading, ThreadBrowserLayout.titleGap)
                    .accessibilityAddTraits(.isHeader)
                    .accessibilityFocused($titleSpoken)
                if browser.canSearch {
                    Rectangle()
                        .fill(Color(nsColor: Palette.borderSubtle))
                        .frame(width: 1, height: 24)
                        .padding(.horizontal, ThreadBrowserLayout.dividerGap)
                    ThreadSearchField(query: $query, submit: submit, requests: fieldRequests)
                        .frame(width: searchWidth(in: proxy.size.width))
                }
                Spacer(minLength: 0)
                if browser.presentation == .modal {
                    ModalCloseButton(action: close)
                        .padding(.leading, ThreadBrowserLayout.closeGap)
                }
            }
            .padding(.horizontal, ThreadBrowserLayout.inset)
            .frame(width: proxy.size.width, height: ThreadBrowserLayout.headerHeight - 1)
        }
        .frame(height: ThreadBrowserLayout.headerHeight - 1)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: Palette.borderSubtle))
                .frame(height: 1)
                .offset(y: 1)
        }
        .padding(.bottom, 1)
        .onChange(of: query) { _, text in browser.setQuery(text) }
        .onAppear {
            if !browser.canSearch {
                titleSpoken = true
            }
        }
    }

    private func submit() {
        Task { await browser.submit() }
    }

    private func searchWidth(in width: CGFloat) -> CGFloat {
        let title = (Self.title as NSString).size(withAttributes: [
            .font: TypeScale.browserTitle.nsFont
        ]).width.rounded(.up)
        var fixed =
            2 * ThreadBrowserLayout.inset + ThreadBrowserLayout.iconBox
            + ThreadBrowserLayout.titleGap + title + 2 * ThreadBrowserLayout.dividerGap + 1
        if browser.presentation == .modal {
            fixed += ThreadBrowserLayout.closeGap + ThreadBrowserLayout.closeBox
        }
        return ThreadBrowserLayout.searchWidth(free: max(0, width - fixed))
    }
}

/// Discord's small search input: Enter submits, the X clears.
struct ThreadSearchField: View {
    @Binding var query: String
    let submit: () -> Void
    /// Each change takes the focus.
    var requests = 0
    @FocusState private var focused: Bool
    @AccessibilityFocusState private var spoken: Bool

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
            TextField(
                "", text: $query,
                prompt: Text(ThreadBrowserView.placeholder)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
            )
            .textFieldStyle(.plain)
            .font(TypeScale.browserText.font)
            .foregroundStyle(Color(nsColor: Palette.textDefault))
            .focused($focused)
            .accessibilityFocused($spoken)
            .onSubmit(submit)
            .accessibilityLabel(ThreadBrowserView.placeholder)
            .background(CursorArea(.iBeam))
            if !query.isEmpty {
                SearchClearButton { query = "" }
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 32)
        .background(Color(nsColor: Palette.inputBackground), in: RoundedRectangle(cornerRadius: 8))
        .overlay {
            RoundedRectangle(cornerRadius: 8)
                .strokeBorder(
                    Color(nsColor: focused ? Palette.brand : Palette.borderSubtle),
                    lineWidth: focused ? 2 : 1)
        }
        .onAppear {
            focused = true
            spoken = true
        }
        .onChange(of: requests) { focused = true }
    }
}

/// The search field's X: brighter while hovered, as Discord's.
struct SearchClearButton: View {
    let clear: () -> Void
    /// Fixed for tests; otherwise the pointer's.
    var hovered: Bool?
    @State private var pointerOver = false

    var body: some View {
        Button(action: clear) {
            Image(systemName: "xmark.circle.fill")
                .font(.system(size: 12))
                .foregroundStyle(
                    Color(
                        nsColor: hovered ?? pointerOver
                            ? Palette.interactiveTextActive : Palette.interactiveText))
        }
        .buttonStyle(.plain)
        .onHover { pointerOver = $0 }
        .background(CursorArea(.pointingHand))
        .accessibilityLabel("Clear")
    }
}

/// A modal's X, Discord's icon-only button: a 32 square, radius 8, filled while hovered or
/// pressed.
struct ModalCloseButton: View {
    enum Look {
        case idle
        case hovered
        case pressed
    }

    let action: () -> Void
    /// Fixed for tests; otherwise the pointer's.
    var look: Look?
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "xmark")
                .font(.system(size: 14, weight: .semibold))
        }
        .buttonStyle(Style(look: look, hovered: hovered))
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .help("Close")
        .accessibilityLabel("Close")
    }

    private struct Style: ButtonStyle {
        let look: Look?
        let hovered: Bool

        static let radius: CGFloat = 8

        func makeBody(configuration: Configuration) -> some View {
            let look = look ?? (configuration.isPressed ? .pressed : hovered ? .hovered : .idle)
            let shape = RoundedRectangle(cornerRadius: Self.radius)
            let glyph = look == .idle ? Palette.interactiveText : Palette.interactiveTextActive
            return configuration.label
                .foregroundStyle(Color(nsColor: glyph))
                .frame(width: ThreadBrowserLayout.closeBox, height: ThreadBrowserLayout.closeBox)
                .background(fill(look), in: shape)
                .overlay {
                    if look != .idle {
                        shape.strokeBorder(Color(nsColor: Palette.iconOnlyBorder), lineWidth: 1)
                    }
                }
                .contentShape(shape)
        }

        private func fill(_ look: Look) -> Color {
            switch look {
            case .idle: .clear
            case .hovered: Color(nsColor: Palette.iconOnlyHover)
            case .pressed: Color(nsColor: Palette.iconOnlyPressed)
            }
        }
    }
}

/// A thread's card: its name, the second line and its members' faces.
struct ThreadBrowserCard: View {
    let row: ThreadBrowserModel.Row
    var now: Date?
    let open: (_ beside: Bool) -> Void
    @State private var hovered = false

    private static let radius: CGFloat = 8
    private static let padding: CGFloat = 15

    var body: some View {
        HStack(alignment: .center, spacing: 0) {
            VStack(alignment: .leading, spacing: 0) {
                Text(row.name)
                    .font(TypeScale.browserTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .lineLimit(1)
                    .frame(height: TypeScale.browserTitle.line)
                ThreadBrowserSubtitle(subtitle: row.subtitle, now: now)
                    .frame(height: TypeScale.browserText.line)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            if !row.faces.isEmpty {
                Facepile(faces: row.faces, more: row.more, behind: background)
                    .padding(.leading, 16)
            }
        }
        .padding(Self.padding + 1)
        .frame(height: ThreadBrowserLayout.cardHeight)
        .background(background, in: RoundedRectangle(cornerRadius: Self.radius))
        .overlay {
            RoundedRectangle(cornerRadius: Self.radius)
                .strokeBorder(
                    Color(nsColor: hovered ? Palette.borderNormal : Palette.borderSubtle),
                    lineWidth: 1)
        }
        .contentShape(RoundedRectangle(cornerRadius: Self.radius))
        .onHover { hovered = $0 }
        .onTapGesture { open(NSEvent.modifierFlags.contains(.shift)) }
        .background(CursorArea(.pointingHand))
        .padding(.bottom, ThreadBrowserLayout.cardGap)
        .accessibilityElement(children: .combine)
        .accessibilityAddTraits(.isButton)
    }

    private var background: Color {
        Color(nsColor: hovered ? Palette.surfaceHighest : Palette.surfaceHigher)
    }
}

/// "Ray: test • 6h ago", "No recent messages • 1d ago", "Started by Ray • Last active 3d ago".
struct ThreadBrowserSubtitle: View {
    let subtitle: ThreadBrowserModel.Row.Subtitle
    var now: Date?

    // Discord's separator in active rows, its own text.
    private static let separator = "\u{00a0} \u{2022} \u{00a0}"

    var body: some View {
        if let now {
            line(now: now)
        } else {
            TimelineView(.periodic(from: .now, by: 60)) { context in
                line(now: context.date)
            }
        }
    }

    @ViewBuilder private func line(now: Date) -> some View {
        HStack(spacing: 0) {
            switch subtitle {
            case .latest(let message, let author, let time):
                InitialsAvatar(message.author, size: 16)
                    .padding(.trailing, 8)
                Text(author)
                    .fontWeight(.semibold)
                    .frame(maxWidth: 150, alignment: .leading)
                    .fixedSize(horizontal: true, vertical: false)
                    .layoutPriority(1)
                Text(":\u{00a0}")
                content(of: message)
                    .layoutPriority(-1)
                Text(Self.separator + Self.time(time, now: now))
                    .fixedSize()
            case .noRecentMessages(let time):
                Text("No recent messages" + Self.separator + Self.time(time, now: now))
            case .startedBy(let owner, let name, let time):
                InitialsAvatar(owner, size: 16)
                    .padding(.trailing, 8)
                Text("Started by ")
                Text(name)
                    .fontWeight(.semibold)
                    .frame(maxWidth: 150, alignment: .leading)
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(.leading, 4)
                Text("\u{2022}")
                    .foregroundStyle(Color(nsColor: Palette.backgroundModMuted))
                    .padding(.horizontal, 8)
                Text("Last active \(Self.time(time, now: now))")
            case .lastActive(let time):
                Circle()
                    .fill(Color(nsColor: Palette.baseLowest))
                    .frame(width: 16, height: 16)
                    .overlay {
                        Image(systemName: ChannelSymbols.name(for: .publicThread))
                            .font(.system(size: 8))
                    }
                    .padding(.trailing, 8)
                Text("Last active \(Self.time(time, now: now))")
            }
        }
        .font(TypeScale.browserText.font)
        .foregroundStyle(Color(nsColor: Palette.textDefault))
        .lineLimit(1)
    }

    private func content(of message: Message) -> Text {
        if let placeholder = ThreadBox.placeholder(for: message) {
            return Text(placeholder).italic()
        }
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let built = ThreadBoxView.content(
            MessageText.build(message, revealed: [], now: Date(), times: times).text)
        let attributed =
            (try? AttributedString(built, including: \.appKit)) ?? AttributedString(built.string)
        return Text(attributed)
    }

    static func time(_ date: Date, now: Date) -> String {
        ThreadBox.relativeTime(since: date, now: now, locale: .current).text
    }
}

/// Members' faces, 24 each, a 16 step, each cut where the next one lies; "+3" or "50+" after.
struct Facepile: View {
    let faces: [ThreadBrowserModel.Row.Face]
    let more: String?
    let behind: Color

    private static let size: CGFloat = 24
    private static let step: CGFloat = 16
    private static let cut: CGFloat = 2

    var body: some View {
        HStack(spacing: 0) {
            ZStack(alignment: .leading) {
                ForEach(Array(faces.enumerated()), id: \.offset) { index, face in
                    avatar(face)
                        .mask {
                            if index < faces.count - 1 || more != nil {
                                Rectangle()
                                    .overlay {
                                        Circle()
                                            .frame(
                                                width: Self.size + 2 * Self.cut,
                                                height: Self.size + 2 * Self.cut)
                                            .offset(x: Self.step)
                                            .blendMode(.destinationOut)
                                    }
                                    .compositingGroup()
                            } else {
                                Rectangle()
                            }
                        }
                        .offset(x: CGFloat(index) * Self.step)
                }
            }
            .frame(
                width: Self.size + CGFloat(max(0, faces.count - 1)) * Self.step, height: Self.size,
                alignment: .leading)
            if let more {
                Text(more)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                    .padding(.horizontal, 6)
                    .frame(minWidth: Self.size, minHeight: Self.size)
                    .background(Color(nsColor: Palette.hoverBackground), in: Capsule())
                    .padding(.leading, Self.step - Self.size)
            }
        }
        .accessibilityHidden(true)
    }

    @ViewBuilder private func avatar(_ face: ThreadBrowserModel.Row.Face) -> some View {
        if let user = face.user {
            InitialsAvatar(user, size: Self.size)
        } else {
            Circle()
                .fill(Color(nsColor: InitialsAvatar.color(face.userId.rawValue)))
                .frame(width: Self.size, height: Self.size)
        }
    }
}

/// "There are no threads.", with a symbol where Discord shows its illustration.
struct ThreadBrowserEmptyState: View {
    var body: some View {
        VStack(spacing: 0) {
            Image(systemName: ChannelSymbols.name(for: .publicThread))
                .font(.system(size: 72, weight: .light))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .frame(width: 256, height: 144)
                .padding(.bottom, 12)
                .accessibilityHidden(true)
            Text(ThreadBrowserView.emptyTitle)
                .font(TypeScale.emptyTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .padding(.bottom, 8)
            Text(ThreadBrowserView.emptyText)
                .font(TypeScale.emptyText.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .multilineTextAlignment(.center)
        }
        .padding(32)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

/// Calls `near` while the enclosing scroll view's bottom is within `distance` of what shows, as it
/// scrolls and as its content changes.
struct ScrollEnd: NSViewRepresentable {
    let distance: CGFloat
    let near: () -> Void

    func makeNSView(context: Context) -> Probe {
        Probe()
    }

    func updateNSView(_ probe: Probe, context: Context) {
        probe.distance = distance
        probe.near = near
        probe.check()
    }

    static func dismantleNSView(_ probe: Probe, coordinator: ()) {
        probe.stop()
    }

    final class Probe: NSView {
        var distance: CGFloat = 0
        var near: () -> Void = {}

        override func hitTest(_ point: NSPoint) -> NSView? { nil }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            stop()
            guard window != nil, let clip = enclosingScrollView?.contentView else {
                return
            }
            clip.postsBoundsChangedNotifications = true
            NotificationCenter.default.addObserver(
                self, selector: #selector(scrolled), name: NSView.boundsDidChangeNotification,
                object: clip)
            check()
        }

        func stop() {
            NotificationCenter.default.removeObserver(
                self, name: NSView.boundsDidChangeNotification, object: nil)
        }

        @objc private func scrolled(_ notification: Notification) {
            check()
        }

        func check() {
            guard let scroll = enclosingScrollView, let document = scroll.documentView else {
                return
            }
            let shown = scroll.contentView.bounds
            let below = document.isFlipped ? document.frame.height - shown.maxY : shown.minY
            if below <= distance {
                near()
            }
        }
    }
}
