import AkariKit
import SwiftUI

struct ServerRail: View {
    let session: SessionModel

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 8) {
                RailItem(selected: session.place == .home, help: "Direct Messages") { highlighted in
                    RailIcon(highlighted: highlighted) {
                        Image(systemName: "bubble.left.and.bubble.right.fill")
                            .font(.system(size: 18))
                    }
                } action: {
                    session.open(.home)
                }
                ForEach(session.directMessages.unread) { conversation in
                    RailItem(
                        selected: false, help: conversation.name,
                        badge: session.directMessages.badge(of: conversation.id)
                    ) { _ in
                        RailAvatar(conversation: conversation)
                    } action: {
                        Self.open(conversation, in: session)
                    }
                }
                Rectangle()
                    .fill(Color(nsColor: Palette.frameBorder))
                    .frame(width: 32, height: 1)
                ForEach(session.servers.items) { item in
                    switch item {
                    case .guild(let guild):
                        guildItem(guild)
                    case .folder(let folder):
                        let expanded = session.servers.isExpanded(folder: folder.id)
                        FolderColumn(color: folder.folder.color, expanded: expanded) {
                            RailItem(
                                selected: FolderTile.selected(
                                    folder, expanded: expanded, place: session.place),
                                help: FolderTile.help(folder),
                                badge: session.servers.tileBadge(of: folder)
                            ) { _ in
                                FolderTile(item: folder, expanded: expanded)
                            } action: {
                                session.servers.toggle(folder: folder.id)
                            }
                            if expanded {
                                ForEach(folder.guilds, id: \.id) { guild in
                                    guildItem(guild)
                                }
                            }
                        }
                    }
                }
                let unavailable = session.servers.unavailableIds.count
                if unavailable > 0 {
                    RailIcon(highlighted: false) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 18))
                    }
                    .help(
                        unavailable == 1
                            ? "1 server unavailable" : "\(unavailable) servers unavailable")
                }
            }
            .padding(.bottom, 72)
            .frame(maxWidth: .infinity)
        }
        // Outside the scroll view: one that touches the title bar's safe area stretches under it.
        .padding(.top, 4)
    }

    /// Opens an unread DM from the rail, at home wherever the user is.
    static func open(_ conversation: DirectMessageListModel.Conversation, in session: SessionModel) {
        session.go(to: conversation.id)
    }

    private func guildItem(_ guild: Guild) -> some View {
        RailItem(
            selected: session.place == .guild(guild.id), help: guild.name,
            badge: session.servers.badge(of: guild.id)
        ) { highlighted in
            GuildIcon(name: guild.name, highlighted: highlighted)
        } action: {
            session.open(.guild(guild.id))
        }
    }
}

struct RailItem<Icon: View>: View {
    let selected: Bool
    let help: String
    var badge = Badge(unread: false, mentions: 0)
    @ViewBuilder let icon: (_ highlighted: Bool) -> Icon
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            icon(selected || hovered)
                .mentionBadge(badge.mentions)
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(help)
        .accessibilityValue(MentionBadge.accessibilityValue(badge))
        .frame(width: SidebarWidth.rail, height: 40)
        .overlay(alignment: .leading) {
            RailPill(height: RailPill.height(selected: selected, hovered: hovered, unread: badge.unread))
        }
    }
}

struct RailPill: View {
    let height: CGFloat

    /// Selection and hover win over the short unread pill.
    static func height(selected: Bool, hovered: Bool, unread: Bool) -> CGFloat {
        if selected {
            40
        } else if hovered {
            20
        } else {
            unread ? 8 : 0
        }
    }

    var body: some View {
        UnevenRoundedRectangle(bottomTrailingRadius: 4, topTrailingRadius: 4)
            .fill(Color(nsColor: Palette.textStrong))
            .frame(width: 4, height: max(height, 8))
            .opacity(height == 0 ? 0 : 1)
            .animation(.easeOut(duration: 0.2), value: height)
    }
}

enum RailShape {
    /// A rail icon's corners, also a collapsed folder's.
    static let cornerRadius: CGFloat = 12
}

struct RailIcon<Content: View>: View {
    let highlighted: Bool
    @ViewBuilder let content: Content

    var body: some View {
        RoundedRectangle(cornerRadius: RailShape.cornerRadius, style: .continuous)
            .fill(Color(nsColor: highlighted ? Palette.brand : Palette.hoverBackground))
            .frame(width: 40, height: 40)
            .overlay {
                content.foregroundStyle(
                    highlighted ? Color.white : Color(nsColor: Palette.textDefault))
            }
            .animation(.easeOut(duration: 0.2), value: highlighted)
    }
}

struct GuildIcon: View {
    let name: String
    let highlighted: Bool

    var body: some View {
        let initials = Initials.of(name)
        RailIcon(highlighted: highlighted) {
            Text(initials)
                .font(.system(size: Self.fontSize(initials.count), weight: .medium))
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .padding(.horizontal, 2)
        }
    }

    static func fontSize(_ length: Int) -> CGFloat {
        switch length {
        case ...2: 18
        case 3...4: 16
        case 5: 14
        case 6: 12
        default: 10
        }
    }
}

/// A DM with unread mentions on the rail: the other person's avatar, or a group's.
private struct RailAvatar: View {
    let conversation: DirectMessageListModel.Conversation

    var body: some View {
        if conversation.channel.kind == .dm, let user = conversation.recipients.first {
            InitialsAvatar(user, size: 40)
        } else {
            InitialsAvatar(name: conversation.name, id: conversation.id.rawValue, size: 40)
        }
    }
}
