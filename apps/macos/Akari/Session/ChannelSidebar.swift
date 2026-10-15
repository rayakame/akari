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
                    ForEach(
                        collapsed.visible(list.shown(open: open), in: list.guildId) {
                            list.staysWhenCollapsed($0, open: open)
                        },
                        id: \.id
                    ) {
                        channel in
                        let muted = list.muted.contains(channel.id)
                        if channel.kind == .guildCategory {
                            CategoryRow(
                                name: channel.name ?? "",
                                collapsed: collapsed.isCollapsed(channel.id, in: list.guildId),
                                muted: muted
                            ) {
                                collapsed.toggle(channel.id, in: list.guildId)
                            }
                        } else {
                            ChannelRow(
                                channel: channel, selected: channel.id == open, muted: muted,
                                badge: list.badge(of: channel.id)
                            ) {
                                session.open(channel: channel.id)
                            }
                        }
                    }
                }
                .padding(.bottom, 72)
            }
        }
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
        .accessibilityValue(MentionBadge.accessibilityValue(badge))
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
