import AkariKit
import SwiftUI

struct ChannelSidebar: View {
    let session: SessionModel
    let list: ChannelListModel
    let collapsed: CollapsedCategories
    let guildName: String

    var body: some View {
        VStack(spacing: 0) {
            ListHeader(title: guildName)
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    let open = session.messages?.channelId
                    let rows = collapsed.visible(list.shown(open: open), in: list.guildId) {
                        list.staysWhenCollapsed($0, open: open)
                    }
                    let firstThreads = Self.firstThreads(in: rows)
                    let lastThreads = Self.lastThreads(in: rows)
                    ForEach(rows, id: \.id) { channel in
                        let muted = list.muted.contains(channel.id)
                        if channel.kind == .guildCategory {
                            CategoryRow(
                                name: channel.name ?? "",
                                collapsed: collapsed.isCollapsed(channel.id, in: list.guildId),
                                muted: muted
                            ) {
                                collapsed.toggle(channel.id, in: list.guildId)
                            }
                        } else if channel.isThread {
                            ThreadRow(
                                channel: channel, first: firstThreads.contains(channel.id),
                                last: lastThreads.contains(channel.id),
                                selected: channel.id == open, muted: muted,
                                badge: list.badge(of: channel.id),
                                parentName: channel.parentId.flatMap { list.channel($0)?.name }
                            ) {
                                session.open(channel: channel.id)
                            }
                            .markReadMenu(enabled: session.canMarkRead(channel: channel.id)) {
                                session.markRead(channel: channel.id)
                            }
                        } else {
                            ChannelRow(
                                channel: channel, selected: channel.id == open, muted: muted,
                                badge: list.badge(of: channel.id)
                            ) {
                                session.open(channel: channel.id)
                            }
                            .markReadMenu(enabled: session.canMarkRead(channel: channel.id)) {
                                session.markRead(channel: channel.id)
                            }
                        }
                    }
                }
                .padding(.bottom, 72)
            }
        }
    }

    /// The threads whose spine starts under their parent's symbol: each parent's first one.
    static func firstThreads(in rows: [Channel]) -> Set<ChannelId> {
        var first: Set<ChannelId> = []
        for (index, row) in rows.enumerated() where row.isThread {
            let previous = index > 0 ? rows[index - 1] : nil
            if previous?.isThread != true || previous?.parentId != row.parentId {
                first.insert(row.id)
            }
        }
        return first
    }

    /// The threads whose spine ends at their branch: each parent's last listed thread.
    static func lastThreads(in rows: [Channel]) -> Set<ChannelId> {
        var last: Set<ChannelId> = []
        for (index, row) in rows.enumerated() where row.isThread {
            let next = rows.indices.contains(index + 1) ? rows[index + 1] : nil
            if next?.isThread != true || next?.parentId != row.parentId {
                last.insert(row.id)
            }
        }
        return last
    }
}

struct CategoryRow: View {
    let name: String
    let collapsed: Bool
    let muted: Bool
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 4) {
                Text(name)
                    .font(TypeScale.category.font)
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .rotationEffect(.degrees(collapsed ? -90 : 0))
                Spacer(minLength: 0)
            }
            .foregroundStyle(
                Color(nsColor: hovered ? Palette.interactiveTextActive : Palette.channelDefault)
            )
            .opacity(muted ? 0.5 : 1)
            .padding(.leading, 16)
            .padding(.trailing, 8)
            .frame(height: 24)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .padding(.top, 14)
    }
}

struct ChannelRow: View {
    let channel: Channel
    let selected: Bool
    let muted: Bool
    var badge = Badge(unread: false, mentions: 0)
    let open: () -> Void
    @State private var hovered = false
    @Environment(\.colorScheme) private var colorScheme

    var body: some View {
        Button {
            if channel.opensMessageList {
                open()
            }
        } label: {
            HStack(spacing: 8) {
                HStack(spacing: 8) {
                    ChannelSymbol(channel: channel)
                    Text(channel.name ?? "")
                        .font(TypeScale.channelName.weight(weight).font)
                        .foregroundStyle(
                            Color(
                                nsColor: selected || hovered || badge.unread
                                    ? Palette.interactiveTextActive : Palette.channelDefault)
                        )
                        .lineLimit(1)
                }
                // A muted channel's mentions still show in full red.
                .opacity(muted && !selected ? 0.5 : 1)
                Spacer(minLength: 0)
                if badge.mentions > 0 {
                    MentionBadge(count: badge.mentions)
                }
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 8)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(background)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(
            channel.opensMessageList
                ? channel.topic ?? "" : ChannelSymbols.unsupported(channel.kind)
        )
        .accessibilityValue(MentionBadge.accessibilityValue(badge, muted: muted))
        .padding(.horizontal, 8)
        .overlay(alignment: .leading) {
            if badge.unread && !selected {
                UnreadPill()
            }
        }
    }

    private var weight: TextStyle.Weight {
        selected && colorScheme == .light ? .semibold : .medium
    }

    private var background: Color {
        if selected {
            Color(nsColor: Palette.selectedBackground)
        } else if hovered {
            Color(nsColor: Palette.hoverBackground)
        } else {
            .clear
        }
    }
}

/// A thread under its parent channel: no symbol, hung from a spine under the parent's symbol.
struct ThreadRow: View {
    /// Under the center of the parent row's symbol: the row's inset 8, its padding 8, half of 20.
    nonisolated static let spine: CGFloat = 26
    /// Where the branch ends and the row itself begins.
    nonisolated static let branchEnd: CGFloat = 36

    let channel: Channel
    /// The parent's first thread, whose spine starts under the parent's symbol.
    let first: Bool
    /// The parent's last thread, whose spine ends at its branch.
    let last: Bool
    let selected: Bool
    let muted: Bool
    var badge = Badge(unread: false, mentions: 0)
    var parentName: String?
    let open: () -> Void
    @State private var hovered = false
    @Environment(\.colorScheme) private var colorScheme

    /// What VoiceOver calls the row: the thread and the channel it belongs to.
    static func spokenName(name: String, parent: String?) -> String {
        parent.map { "\(name), thread in \($0)" } ?? "\(name), thread"
    }

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Text(channel.name ?? "")
                    .font(TypeScale.channelName.weight(weight).font)
                    .foregroundStyle(
                        Color(
                            nsColor: selected || hovered || badge.unread
                                ? Palette.interactiveTextActive : Palette.channelDefault)
                    )
                    .lineLimit(1)
                    .opacity(muted && !selected ? 0.5 : 1)
                Spacer(minLength: 0)
                if badge.mentions > 0 {
                    MentionBadge(count: badge.mentions)
                }
            }
            .padding(.vertical, 4)
            .padding(.horizontal, 8)
            .frame(height: 32)
            .background(
                RoundedRectangle(cornerRadius: 8)
                    .fill(background)
            )
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .accessibilityLabel(Self.spokenName(name: channel.name ?? "", parent: parentName))
        .accessibilityValue(MentionBadge.accessibilityValue(badge, muted: muted))
        .padding(.leading, Self.branchEnd)
        .padding(.trailing, 8)
        .background {
            Spine(first: first, last: last)
                .fill(Color(nsColor: Palette.threadSpine))
        }
        .overlay(alignment: .leading) {
            if badge.unread && !selected {
                UnreadPill()
            }
        }
    }

    private var weight: TextStyle.Weight {
        selected && colorScheme == .light ? .semibold : .medium
    }

    private var background: Color {
        if selected {
            Color(nsColor: Palette.selectedBackground)
        } else if hovered {
            Color(nsColor: Palette.hoverBackground)
        } else {
            .clear
        }
    }
}

// Discord's geometry, as one filled outline so rows join without a seam: 2 wide, a round cap on
// the first row's top and the branch's end, 2-point fillets at a joint, a bend into the last.
nonisolated private struct Spine: Shape {
    static let width: CGFloat = 2
    static let reach: CGFloat = 8
    static let fillet: CGFloat = 2
    static let bend: CGFloat = 4

    let first: Bool
    let last: Bool

    func path(in rect: CGRect) -> Path {
        let half = Self.width / 2
        let x = ThreadRow.spine
        let middle = rect.midY
        let end = x + Self.reach
        // A later row starts in the 2 between rows, where the row above ends.
        let top = rect.minY - (first ? 0 : 2)
        var path = Path()
        path.move(to: CGPoint(x: x - half, y: top))
        if first {
            path.addArc(
                tangent1End: CGPoint(x: x - half, y: top - half),
                tangent2End: CGPoint(x: x, y: top - half), radius: half)
            path.addArc(
                tangent1End: CGPoint(x: x + half, y: top - half),
                tangent2End: CGPoint(x: x + half, y: top), radius: half)
        } else {
            path.addLine(to: CGPoint(x: x + half, y: top))
        }
        let inner = last ? Self.bend - half : Self.fillet
        path.addArc(
            tangent1End: CGPoint(x: x + half, y: middle - half),
            tangent2End: CGPoint(x: end, y: middle - half), radius: inner)
        path.addArc(
            tangent1End: CGPoint(x: end + half, y: middle - half),
            tangent2End: CGPoint(x: end + half, y: middle), radius: half)
        path.addArc(
            tangent1End: CGPoint(x: end + half, y: middle + half),
            tangent2End: CGPoint(x: end, y: middle + half), radius: half)
        if last {
            path.addArc(
                tangent1End: CGPoint(x: x - half, y: middle + half),
                tangent2End: CGPoint(x: x - half, y: top), radius: Self.bend + half)
        } else {
            path.addArc(
                tangent1End: CGPoint(x: x + half, y: middle + half),
                tangent2End: CGPoint(x: x + half, y: rect.maxY), radius: Self.fillet)
            path.addLine(to: CGPoint(x: x + half, y: rect.maxY))
            path.addLine(to: CGPoint(x: x - half, y: rect.maxY))
        }
        path.closeSubpath()
        return path
    }
}

struct ChannelSymbol: View {
    let channel: Channel
    var size: CGFloat = 20

    var body: some View {
        Image(systemName: ChannelSymbols.name(for: channel.kind))
            .font(.system(size: size * 0.8))
            .foregroundStyle(Color(nsColor: Palette.interactiveText))
            .frame(width: size, height: size)
            .overlay(alignment: .bottomTrailing) {
                if channel.nsfw {
                    Image(systemName: ChannelSymbols.nsfwBadge)
                        .font(.system(size: 9))
                        .foregroundStyle(Color(nsColor: Palette.danger))
                        .offset(x: 3, y: 3)
                }
            }
    }
}

/// The 4 × 8 pill on the column's left edge beside an unread row.
struct UnreadPill: View {
    var body: some View {
        UnevenRoundedRectangle(bottomTrailingRadius: 4, topTrailingRadius: 4)
            .fill(Color(nsColor: Palette.textStrong))
            .frame(width: 4, height: 8)
    }
}
