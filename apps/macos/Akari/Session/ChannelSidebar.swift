import AkariKit
import SwiftUI

struct ChannelSidebar: View {
    let session: SessionModel
    let list: ChannelListModel
    let collapsed: CollapsedCategories
    let guildName: String

    var body: some View {
        VStack(spacing: 0) {
            Text(guildName)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(Color(nsColor: Palette.textStrong))
                .lineLimit(1)
                .padding(.horizontal, 16)
                .frame(maxWidth: .infinity, minHeight: 48, maxHeight: 48, alignment: .leading)
                .overlay(alignment: .bottom) {
                    Rectangle().fill(Color(nsColor: Palette.borderSubtle)).frame(height: 1)
                }
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 2) {
                    let open = session.messages?.channelId
                    ForEach(
                        collapsed.visible(list.channels, in: list.guildId, open: open), id: \.id
                    ) {
                        channel in
                        if channel.kind == .guildCategory {
                            CategoryRow(
                                name: channel.name ?? "",
                                collapsed: collapsed.isCollapsed(channel.id, in: list.guildId)
                            ) {
                                collapsed.toggle(channel.id, in: list.guildId)
                            }
                        } else {
                            ChannelRow(channel: channel, selected: channel.id == open) {
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
    let toggle: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: toggle) {
            HStack(spacing: 4) {
                Text(name)
                    .font(.system(size: 14, weight: .medium))
                    .lineLimit(1)
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .rotationEffect(.degrees(collapsed ? -90 : 0))
                Spacer(minLength: 0)
            }
            .foregroundStyle(
                Color(nsColor: hovered ? Palette.interactiveTextActive : Palette.channelDefault)
            )
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
                ChannelSymbol(channel: channel)
                Text(channel.name ?? "")
                    .font(.system(size: 16, weight: selected ? selectedWeight : .regular))
                    .foregroundStyle(
                        Color(
                            nsColor: selected || hovered
                                ? Palette.interactiveTextActive : Palette.channelDefault)
                    )
                    .lineLimit(1)
                Spacer(minLength: 0)
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
        .padding(.horizontal, 8)
    }

    private var selectedWeight: Font.Weight {
        colorScheme == .light ? .semibold : .medium
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
