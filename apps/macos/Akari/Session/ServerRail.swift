import AkariKit
import SwiftUI

struct ServerRail: View {
    let session: SessionModel

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 8) {
                RailItem(selected: session.place == .home, help: "Direct Messages") { highlighted, _ in
                    RailIcon(highlighted: highlighted) {
                        Image(systemName: "bubble.left.and.bubble.right.fill")
                            .font(.system(size: 18))
                    }
                } action: {
                    session.open(.home)
                }
                ForEach(session.directMessages.unread) { conversation in
                    // The open DM is here when it turned unread while the user wasn't reading.
                    RailItem(
                        selected: session.messages?.channelId == conversation.id,
                        help: conversation.name,
                        badge: session.directMessages.badge(of: conversation.id)
                    ) { _, hovered in
                        ConversationAvatar(
                            conversation: conversation, size: 40, place: .railIcon, hovered: hovered)
                    } action: {
                        Self.open(conversation, in: session)
                    }
                    .markReadMenu(enabled: session.canMarkRead(channel: conversation.id)) {
                        session.markRead(channel: conversation.id)
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
                            ) { _, _ in
                                FolderTile(item: folder, expanded: expanded)
                            } action: {
                                session.servers.toggle(folder: folder.id)
                            }
                            .markReadMenu(
                                "Mark Folder As Read", enabled: session.canMarkRead(folder: folder)
                            ) {
                                session.markRead(folder: folder)
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
        ) { highlighted, hovered in
            GuildIcon(name: guild.name, icon: guild.icon, highlighted: highlighted, hovered: hovered)
        } action: {
            session.open(.guild(guild.id))
        }
        .markReadMenu(enabled: session.canMarkRead(guild: guild.id)) {
            session.markRead(guild: guild.id)
        }
    }
}

struct RailItem<Icon: View>: View {
    let selected: Bool
    let help: String
    var badge = Badge(unread: false, mentions: 0)
    @ViewBuilder let icon: (_ highlighted: Bool, _ hovered: Bool) -> Icon
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            icon(selected || hovered, hovered)
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

/// A server on the rail: its icon, or its acronym tile (D137). A changed icon swaps once the
/// new one is ready.
struct GuildIcon: View {
    let name: String
    var icon: ImageKey?
    let highlighted: Bool
    var hovered = false

    var body: some View {
        ServerPicture(
            name: name, icon: icon, size: 40, cornerRadius: RailShape.cornerRadius,
            highlighted: highlighted, place: .railIcon, hovered: hovered)
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

/// A server's picture at any size: its icon alone when it has one, nothing drawn under it while
/// it loads or through its transparency, as the official client; the acronym tile (hover and
/// selection fill it) without an icon, or when the icon failed: the icon is still asked again.
struct ServerPicture: View {
    let name: String
    var icon: ImageKey?
    var size: CGFloat = 40
    var cornerRadius: CGFloat = RailShape.cornerRadius
    var highlighted = false
    var place = MotionPolicy.Place.userPanel
    var hovered = false
    @State private var failed = false

    var body: some View {
        ZStack {
            if icon == nil || failed {
                tile
            }
            if let icon {
                RemoteImage(
                    key: icon, cornerRadius: cornerRadius, place: place, hovered: hovered,
                    onFailure: { failed = $0 })
            }
        }
        .frame(width: size, height: size)
        .onChange(of: icon) {
            failed = false
        }
    }

    private var tile: some View {
        let initials = Initials.of(name)
        return RoundedRectangle(cornerRadius: cornerRadius, style: .continuous)
            .fill(Color(nsColor: highlighted ? Palette.brand : Palette.hoverBackground))
            .overlay {
                Text(initials)
                    .font(.system(size: GuildIcon.fontSize(initials.count) * size / 40, weight: .medium))
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
                    .padding(.horizontal, 2 * size / 40)
                    .foregroundStyle(highlighted ? Color.white : Color(nsColor: Palette.textDefault))
            }
            .animation(.easeOut(duration: 0.2), value: highlighted)
    }
}
