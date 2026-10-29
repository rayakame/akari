import AkariKit
import AppKit
import SwiftUI

/// A forum's page in the message list's place: the search row, the guidelines, the tags bar, the
/// posts as list or gallery cards, "Older Posts", the placeholders and the "N New Posts" pill.
struct ForumPage: View {
    let forum: ForumModel
    let session: SessionModel
    /// Fixed for tests; otherwise the times follow the clock.
    var now: Date?
    @State private var guidelinesOpen = false

    private static let top = "top"

    var body: some View {
        GeometryReader { proxy in
            ScrollViewReader { scroller in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        header
                            .id(Self.top)
                            .padding(.horizontal, ForumMeasures.inset)
                        sections(size: ForumMeasures.content(proxy.size))
                            .padding(.horizontal, inset)
                    }
                    .padding(.top, ForumMeasures.top)
                    .padding(.bottom, ForumMeasures.bottom)
                    .background {
                        ScrollEnd(distance: loadDistance(width: ForumMeasures.content(proxy.size).width)) {
                            if forum.older == .more {
                                Task { await forum.loadMore() }
                            }
                        }
                    }
                }
                .overlay(alignment: .bottom) {
                    if forum.newPosts > 0, forum.search == .none {
                        NewPostsPill(count: forum.newPosts) {
                            forum.refresh()
                            withAnimation { scroller.scrollTo(Self.top, anchor: .top) }
                        }
                        .padding(.bottom, 16)
                    }
                }
            }
        }
        .frame(minWidth: ForumMeasures.columnMin)
        .background(Color(nsColor: Palette.chat))
        .environment(\.placeholders, session.placeholders)
        .task(id: forum.forumId) { await forum.open() }
    }

    private var inset: CGFloat {
        forum.settings.layout == .gallery ? ForumMeasures.galleryInset : ForumMeasures.inset
    }

    private var name: String {
        forum.channel?.name ?? ""
    }

    // The gallery loads a card's height ahead, at least 200.
    private func loadDistance(width: CGFloat) -> CGFloat {
        guard forum.settings.layout == .gallery else {
            return ForumMeasures.loadDistance
        }
        let column = ForumMeasures.columns(width: width - 2 * ForumMeasures.galleryInset).width
        return max(ForumMeasures.loadDistance, ForumMeasures.galleryCardHeight(width: column))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForumSearchRow(
                forum: forum,
                guidelines: forum.channel?.topic?.isEmpty == false
                    ? { withAnimation(.easeInOut(duration: 0.3)) { guidelinesOpen.toggle() } }
                    : nil,
                shiftEscape: session.markServerRead)
            if guidelinesOpen, let topic = forum.channel?.topic, !topic.isEmpty {
                ForumGuidelines(topic: topic) {
                    withAnimation(.easeInOut(duration: 0.3)) { guidelinesOpen = false }
                }
                .padding(.top, 16)
                .transition(.opacity)
            }
            ForumTagsBar(forum: forum)
            if forum.isEmpty {
                ForumEmptyState(channel: name, tags: forum.settings.tags.count)
            }
        }
    }

    @ViewBuilder private func sections(size: CGSize) -> some View {
        if forum.searchDenied {
            ForumPermissionBox(text: ForumPermissionBox.search(name))
        } else {
            cards(forum.posts, width: size.width)
            if forum.search == .none {
                if !forum.canReadHistory {
                    ForumPermissionBox(text: ForumPermissionBox.history(name))
                } else if !forum.archived.isEmpty {
                    if !forum.posts.isEmpty {
                        OlderPostsDivider()
                    }
                    cards(forum.archived, width: size.width)
                }
            }
            if forum.showsPlaceholders {
                ForumPlaceholders(
                    layout: forum.settings.layout, size: size,
                    width: size.width - 2 * inset)
            }
        }
    }

    @ViewBuilder private func cards(_ cards: [ForumModel.Card], width: CGFloat) -> some View {
        switch forum.settings.layout {
        case .list:
            ForEach(cards) { card in
                ForumListCard(card: card, filter: forum.settings.tags, now: now) { open(card) }
                    .onAppear { forum.cardShown(card.id) }
                    .onDisappear { forum.cardHidden(card.id) }
                    .padding(.bottom, ForumMeasures.cardGap)
            }
        case .gallery:
            let columns = ForumMeasures.columns(width: width - 2 * ForumMeasures.galleryInset)
            let size = CGSize(
                width: columns.width, height: ForumMeasures.galleryCardHeight(width: columns.width))
            let rows = stride(from: 0, to: cards.count, by: columns.count).map {
                Array(cards[$0..<min($0 + columns.count, cards.count)])
            }
            ForEach(rows, id: \.first?.id) { row in
                HStack(alignment: .top, spacing: ForumMeasures.galleryGap) {
                    ForEach(row) { card in
                        ForumGalleryCard(card: card, filter: forum.settings.tags, size: size, now: now) {
                            open(card)
                        }
                        .onAppear { forum.cardShown(card.id) }
                        .onDisappear { forum.cardHidden(card.id) }
                    }
                }
                .padding(.bottom, ForumMeasures.galleryGap)
            }
        }
    }

    // Until posts open beside the forum, a card opens its post in full, as Shift+click does in
    // Discord.
    private func open(_ card: ForumModel.Card) {
        session.go(to: card.id)
    }
}

/// The header row as search: the magnifying glass (a spinner while searching), the field, the
/// guidelines button; the status row under it while a search runs or answered.
struct ForumSearchRow: View {
    let forum: ForumModel
    let guidelines: (() -> Void)?
    var shiftEscape: () -> Void = {}
    @State private var query = ""
    @FocusState private var focused: Bool

    static let placeholder = "Search"

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 0) {
                Group {
                    if forum.search == .searching {
                        ProgressView().controlSize(.small)
                    } else {
                        Image(systemName: "magnifyingglass")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    }
                }
                .frame(width: ForumMeasures.control, height: ForumMeasures.control)
                .padding(.trailing, 8)
                .accessibilityHidden(true)
                TextField(
                    "", text: $query,
                    prompt: Text(Self.placeholder)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                )
                .textFieldStyle(.plain)
                .font(TypeScale.forumField.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .focused($focused)
                .onExitCommand {
                    if NSApp.currentEvent?.modifierFlags.contains(.shift) == true {
                        shiftEscape()
                    } else {
                        query = ""
                    }
                }
                .accessibilityLabel(Self.placeholder)
                .padding(.trailing, 12)
                if let guidelines {
                    GuidelinesButton(action: guidelines)
                }
            }
            .padding(8)
            .background(Color(nsColor: Palette.inputBackground))
            if let status = forum.status {
                Rectangle()
                    .fill(Color(nsColor: Palette.borderSubtle))
                    .frame(height: 1)
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
        }
        .background(Color(nsColor: Palette.panel))
        .clipShape(RoundedRectangle(cornerRadius: ForumMeasures.radius))
        .overlay {
            RoundedRectangle(cornerRadius: ForumMeasures.radius)
                .strokeBorder(
                    Color(nsColor: focused ? Palette.brand : Palette.borderSubtle), lineWidth: 1)
        }
        .onChange(of: query) { _, text in
            forum.setQuery(text)
            // Past 100 characters the model's query stays: the field follows it.
            if forum.query != text {
                query = forum.query
            }
        }
        .onChange(of: forum.query) { _, text in
            if text != query {
                query = text
            }
        }
        .onAppear { query = forum.query }
    }
}

/// "Post Guidelines": the icon button at the search row's right.
struct GuidelinesButton: View {
    let action: () -> Void
    @State private var hovered = false

    static let title = "Post Guidelines"

    var body: some View {
        Button(action: action) {
            Image(systemName: "book.closed")
                .font(.system(size: 16))
                .foregroundStyle(
                    Color(nsColor: hovered ? Palette.interactiveTextActive : Palette.interactiveText))
                .frame(width: ForumMeasures.control, height: ForumMeasures.control)
                .background(
                    Color(nsColor: hovered ? Palette.backgroundModStrong : Palette.backgroundModNormal),
                    in: RoundedRectangle(cornerRadius: 8))
                .contentShape(RoundedRectangle(cornerRadius: 8))
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .help(Self.title)
        .accessibilityLabel(Self.title)
    }
}

/// The forum's topic as its guidelines, at most 220 tall until "View All".
struct ForumGuidelines: View {
    let topic: String
    let close: () -> Void
    @State private var expanded = false
    @State private var fullHeight: CGFloat = 0

    static let maxHeight: CGFloat = 220

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 8) {
                Image(systemName: "book.closed")
                    .font(.system(size: 18))
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .accessibilityHidden(true)
                Text(GuidelinesButton.title)
                    .font(TypeScale.forumTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                    .accessibilityAddTraits(.isHeader)
                Spacer(minLength: 0)
                Button("Close", action: close)
                    .buttonStyle(.plain)
                    .font(TypeScale.forumSort.font)
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
                    .background(CursorArea(.pointingHand))
            }
            Text(topic)
                .font(TypeScale.forumGuidelines.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
                .background {
                    GeometryReader { proxy in
                        Color.clear
                            .onAppear { fullHeight = proxy.size.height }
                            .onChange(of: proxy.size.height) { _, height in fullHeight = height }
                    }
                }
                .frame(maxHeight: expanded ? nil : Self.maxHeight, alignment: .top)
                .clipped()
            if !expanded, fullHeight > Self.maxHeight {
                Button("View All") { expanded = true }
                    .buttonStyle(.plain)
                    .font(TypeScale.forumSort.font.weight(.semibold))
                    .foregroundStyle(Color(nsColor: Palette.textLink))
                    .background(CursorArea(.pointingHand))
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: Palette.panel), in: RoundedRectangle(cornerRadius: ForumMeasures.radius))
        .overlay {
            RoundedRectangle(cornerRadius: ForumMeasures.radius)
                .strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1)
        }
    }
}

/// "Sort & View", the divider and the tags on one row, "All" after the last that fits.
struct ForumTagsBar: View {
    let forum: ForumModel
    @State private var selecting = false

    static let gap: CGFloat = 8

    /// The tags whose pills fit before "All" in `width`; the others aren't in the row at all, so
    /// neither VoiceOver nor Tab reaches them.
    static func fitting(_ tags: [ForumTag], in width: CGFloat) -> [ForumTag] {
        var used = AllTagsButton.width
        var shown: [ForumTag] = []
        for tag in tags {
            used += ForumTagPill.width(tag) + gap
            guard used <= width else {
                break
            }
            shown.append(tag)
        }
        return shown
    }

    var body: some View {
        HStack(spacing: 0) {
            ForumSortMenu(forum: forum)
            if !forum.tags.isEmpty {
                RoundedRectangle(cornerRadius: 4)
                    .fill(Color(nsColor: Palette.borderSubtle))
                    .frame(width: 2, height: 24)
                    .padding(.horizontal, 16)
                GeometryReader { proxy in
                    HStack(spacing: Self.gap) {
                        ForEach(Self.fitting(forum.tags, in: proxy.size.width), id: \.id) { tag in
                            TagFilterPill(
                                tag: tag, selected: forum.settings.tags.contains(tag.id)
                            ) { forum.toggle(tag: tag.id) }
                        }
                        AllTagsButton(count: forum.settings.tags.count) { selecting = true }
                            .popover(isPresented: $selecting, arrowEdge: .bottom) {
                                SelectTags(forum: forum)
                            }
                    }
                    .frame(height: ForumMeasures.control)
                }
                .frame(height: ForumMeasures.control)
            }
        }
        .padding(.top, 16)
        .padding(.bottom, 4)
    }
}

/// A tag in the bar: in or out of the filter on a click.
struct TagFilterPill: View {
    let tag: ForumTag
    let selected: Bool
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            ForumTagPill(tag: tag, look: selected ? .selected : .idle, hovered: hovered)
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .accessibilityAddTraits(selected ? .isSelected : [])
    }
}

/// "All", or the filter's count in the brand's bubble, and the chevron: opens Select Tags.
struct AllTagsButton: View {
    let count: Int
    let action: () -> Void

    // At least 52 inside and 8 at each side: "All" and the count both fit.
    static let width: CGFloat = 52 + 2 * 8

    var body: some View {
        Button(action: action) {
            HStack(spacing: 4) {
                if count > 0 {
                    Text("\(count)")
                        .font(TypeScale.forumTagSmall.font.weight(.medium))
                        .foregroundStyle(.white)
                        .frame(minWidth: 16, minHeight: 16)
                        .padding(2)
                        .background(Color(nsColor: Palette.brand), in: Circle())
                } else {
                    Text("All")
                        .font(TypeScale.forumTag.font)
                        .foregroundStyle(Color(nsColor: Palette.interactiveText))
                }
                Image(systemName: "chevron.down")
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: Palette.interactiveText))
            }
            .frame(minWidth: 52)
            .padding(.horizontal, 4 + 4)
            .frame(height: ForumMeasures.control)
            .background(Color(nsColor: Palette.baseLowest), in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .background(CursorArea(.pointingHand))
        .accessibilityLabel(count > 0 ? "\(count) tags selected" : "All tags")
    }
}

/// The popout of every tag, with the filter's count and "Clear all".
struct SelectTags: View {
    let forum: ForumModel

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Select Tags")
                    .font(TypeScale.forumEmptyTitle.font)
                    .foregroundStyle(Color(nsColor: Palette.textStrong))
                if !forum.settings.tags.isEmpty {
                    Text("\(forum.settings.tags.count)")
                        .font(TypeScale.forumTagSmall.font)
                        .foregroundStyle(Color(nsColor: Palette.textMuted))
                }
                Spacer(minLength: 16)
                Button("Clear all") { forum.clearTags() }
                    .buttonStyle(.plain)
                    .font(TypeScale.forumStatusAction.font)
                    .foregroundStyle(Color(nsColor: Palette.textLink))
                    .disabled(forum.settings.tags.isEmpty)
                    .background {
                        if !forum.settings.tags.isEmpty {
                            CursorArea(.pointingHand)
                        }
                    }
            }
            FlowRows(gap: 8) {
                ForEach(forum.tags, id: \.id) { tag in
                    TagFilterPill(tag: tag, selected: forum.settings.tags.contains(tag.id)) {
                        forum.toggle(tag: tag.id)
                    }
                }
            }
        }
        .padding(16)
        .frame(width: 420)
    }
}

/// "Sort & View": the forum's sort, view and tag matching, and "Reset to default".
struct ForumSortMenu: View {
    let forum: ForumModel
    @State private var hovered = false

    static let title = "Sort & View"

    var body: some View {
        Menu {
            Picker("Sort by", selection: binding(\.sort, forum.setSort)) {
                Text("Recently Active").tag(ForumSort.latestActivity)
                Text("Date Posted").tag(ForumSort.creationDate)
            }
            .pickerStyle(.inline)
            Picker("View as", selection: binding(\.layout, forum.setLayout)) {
                Text("List").tag(ForumLayout.list)
                Text("Gallery").tag(ForumLayout.gallery)
            }
            .pickerStyle(.inline)
            if !forum.tags.isEmpty {
                Picker("Tag Matching", selection: binding(\.matching, forum.setMatching)) {
                    Text("Match Some").tag(TagMatching.some)
                    Text("Match All").tag(TagMatching.all)
                }
                .pickerStyle(.inline)
            }
            Divider()
            Button("Reset to default") { forum.resetToDefault() }
        } label: {
            HStack(spacing: 0) {
                Image(systemName: "arrow.up.arrow.down")
                    .font(.system(size: 12, weight: .semibold))
                Text(Self.title)
                    .font(TypeScale.forumSort.font)
                    .padding(.leading, 8)
                    .padding(.trailing, 4)
                Image(systemName: "chevron.down")
                    .font(.system(size: 12, weight: .semibold))
            }
            .foregroundStyle(
                Color(nsColor: hovered ? Palette.interactiveTextActive : Palette.interactiveText))
            .padding(.leading, 12)
            .padding(.trailing, 8)
            .frame(height: ForumMeasures.control)
            .background(
                Color(nsColor: hovered ? Palette.backgroundModNormal : Palette.baseLowest),
                in: Capsule())
            .contentShape(Capsule())
        }
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .accessibilityLabel("Set sort or view")
    }

    private func binding<Value>(
        _ path: KeyPath<ForumModel.Settings, Value>, _ set: @escaping (Value) -> Void
    ) -> Binding<Value> {
        Binding(get: { forum.settings[keyPath: path] }, set: set)
    }
}

/// Subviews in rows, wrapping at the width.
struct FlowRows: Layout {
    let gap: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        let rows = rows(width: proposal.width ?? .infinity, subviews: subviews)
        let height = rows.map(\.height).reduce(0, +) + gap * CGFloat(max(rows.count - 1, 0))
        return CGSize(width: proposal.width ?? rows.map(\.width).max() ?? 0, height: height)
    }

    func placeSubviews(
        in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()
    ) {
        var y = bounds.minY
        for row in rows(width: bounds.width, subviews: subviews) {
            var x = bounds.minX
            for index in row.indices {
                let size = subviews[index].sizeThatFits(.unspecified)
                subviews[index].place(at: CGPoint(x: x, y: y), proposal: .unspecified)
                x += size.width + gap
            }
            y += row.height + gap
        }
    }

    private func rows(width: CGFloat, subviews: Subviews) -> [(indices: [Int], width: CGFloat, height: CGFloat)] {
        var rows: [(indices: [Int], width: CGFloat, height: CGFloat)] = []
        var current: (indices: [Int], width: CGFloat, height: CGFloat) = ([], 0, 0)
        for index in subviews.indices {
            let size = subviews[index].sizeThatFits(.unspecified)
            let needed = current.indices.isEmpty ? size.width : current.width + gap + size.width
            if needed > width, !current.indices.isEmpty {
                rows.append(current)
                current = ([index], size.width, size.height)
            } else {
                current = (current.indices + [index], needed, max(current.height, size.height))
            }
        }
        if !current.indices.isEmpty {
            rows.append(current)
        }
        return rows
    }
}

/// "OLDER POSTS", Discord's eyebrow above the archived posts: 16 above, 8 below.
struct OlderPostsDivider: View {
    static let title = "Older Posts"

    var body: some View {
        Text(Self.title.uppercased())
            .font(TypeScale.forumEyebrow.font)
            .foregroundStyle(Color(nsColor: Palette.textMuted))
            .frame(height: 16)
            .padding(.top, 16)
            .padding(.bottom, 8)
            .accessibilityAddTraits(.isHeader)
    }
}

/// Discord's box in place of what the user may not see.
struct ForumPermissionBox: View {
    let text: String

    static func history(_ channel: String) -> String {
        "You do not have permission to view the post history of #\(channel)."
    }

    static func search(_ channel: String) -> String {
        "You do not have permission to search the post history of #\(channel)."
    }

    var body: some View {
        Text(text)
            .font(TypeScale.forumBox.font)
            .foregroundStyle(Color(nsColor: Palette.textMuted))
            .multilineTextAlignment(.center)
            .padding(.vertical, 4)
            .padding(.horizontal, 8)
            .frame(maxWidth: .infinity, minHeight: 40)
            .background(Color(nsColor: Palette.baseLowest), in: RoundedRectangle(cornerRadius: 4))
            .padding(.top, 4)
    }
}

/// "Be the first to start the conversation!", or the filter's words.
struct ForumEmptyState: View {
    let channel: String
    /// The tag filter's size.
    let tags: Int

    static func words(channel: String, tags: Int) -> (title: String, text: String) {
        switch tags {
        case 0:
            ("Be the first to start the conversation!", "What do you want to post about in #\(channel)?")
        case 1:
            ("There are no posts with this tag yet!", "Be the first to start a conversation with this tag")
        default:
            ("There are no posts with these tags yet!", "Be the first to start a conversation with these tags")
        }
    }

    var body: some View {
        let words = Self.words(channel: channel, tags: tags)
        VStack(spacing: 8) {
            Image(systemName: ChannelSymbols.name(for: .guildForum))
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(Color(nsColor: Palette.textMuted))
                .padding(.bottom, 8)
                .accessibilityHidden(true)
            Text(words.title)
                .font(TypeScale.forumEmptyTitle.font)
                .foregroundStyle(Color(nsColor: Palette.textStrong))
            Text(words.text)
                .font(TypeScale.forumEmptyText.font)
                .foregroundStyle(Color(nsColor: Palette.textDefault))
        }
        .multilineTextAlignment(.center)
        .frame(maxWidth: .infinity)
        .padding(.vertical, 48)
    }
}

/// Loading cards: base-low, 102 tall, 8 apart, at 0.33, 1 and 0.66 by turns; gallery tiles for
/// the window's rows.
struct ForumPlaceholders: View {
    let layout: ForumLayout
    let size: CGSize
    let width: CGFloat

    private static let opacities: [Double] = [0.33, 1, 0.66]

    var body: some View {
        switch layout {
        case .list:
            ForEach(0..<ForumMeasures.placeholderCards(height: size.height), id: \.self) { index in
                RoundedRectangle(cornerRadius: ForumMeasures.radius)
                    .fill(Color(nsColor: Palette.panel))
                    .frame(height: ForumMeasures.placeholderHeight)
                    .opacity(Self.opacities[index % 3])
                    .padding(.bottom, ForumMeasures.cardGap)
            }
            .accessibilityHidden(true)
        case .gallery:
            let columns = ForumMeasures.columns(width: width)
            let height = ForumMeasures.galleryCardHeight(width: columns.width)
            let rows = max(Int((size.height / height).rounded(.up)), 1)
            ForEach(0..<rows, id: \.self) { row in
                HStack(spacing: ForumMeasures.galleryGap) {
                    ForEach(0..<columns.count, id: \.self) { column in
                        RoundedRectangle(cornerRadius: ForumMeasures.radius)
                            .fill(Color(nsColor: Palette.panel))
                            .frame(width: columns.width, height: height)
                            .opacity(Self.opacities[(row * columns.count + column) % 3])
                    }
                }
                .padding(.bottom, ForumMeasures.galleryGap)
            }
            .accessibilityHidden(true)
        }
    }
}

/// "{count} New Post(s)": Discord's pill at the list's bottom; a click refreshes the list.
struct NewPostsPill: View {
    let count: UInt32
    let action: () -> Void
    @State private var hovered = false

    static func title(_ count: UInt32) -> String {
        count == 1 ? "1 New Post" : "\(count) New Posts"
    }

    var body: some View {
        Button(action: action) {
            Text(Self.title(count))
                .font(TypeScale.forumPill.font)
                .foregroundStyle(Color(nsColor: Palette.textLink))
                .padding(.vertical, 8)
                .padding(.horizontal, 16)
                .frame(height: 36)
                .background(
                    Color(nsColor: hovered ? Palette.surfaceHigher : Palette.surfaceHigh),
                    in: Capsule())
                .overlay { Capsule().strokeBorder(Color(nsColor: Palette.borderSubtle), lineWidth: 1) }
                .shadow(color: .black.opacity(0.24), radius: 8, y: 4)
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
    }
}
