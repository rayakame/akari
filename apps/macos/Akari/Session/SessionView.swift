import AkariKit
import SwiftUI

/// The main window of docs/ui/layout.md.
struct SessionView<MessageArea: View>: View {
    let session: SessionModel
    @ViewBuilder let messageArea: (MessageListModel) -> MessageArea
    @State private var collapsed = CollapsedCategories()
    @State private var sidebarWidth = SidebarWidth.stored(in: .standard)

    var body: some View {
        VStack(spacing: 0) {
            Color(nsColor: Palette.frame).frame(height: 32)
            HStack(spacing: 0) {
                sidebar.frame(width: sidebarWidth)
                page
            }
        }
        .ignoresSafeArea(edges: .top)
        .navigationTitle(title)
    }

    private var sidebar: some View {
        ZStack(alignment: .bottom) {
            HStack(spacing: 0) {
                ServerRail(session: session).frame(width: SidebarWidth.rail)
                Rectangle().fill(Color(nsColor: Palette.frameBorder)).frame(width: 1)
                VStack(spacing: 0) {
                    Rectangle().fill(Color(nsColor: Palette.frameBorder)).frame(height: 1)
                    list
                }
            }
            UserPanel(user: session.currentUser).padding(8)
        }
        .background(Color(nsColor: Palette.frame))
        .overlay(alignment: .trailing) {
            SidebarResizer(width: $sidebarWidth)
        }
    }

    @ViewBuilder private var list: some View {
        switch session.place {
        case .home:
            DirectMessageList(session: session)
        case .guild(let guildId):
            if let channels = session.channels {
                ChannelSidebar(
                    session: session, list: channels, collapsed: collapsed,
                    guildName: session.guilds.guilds.first { $0.id == guildId }?.name ?? "")
            }
        }
    }

    private var page: some View {
        VStack(spacing: 0) {
            ChannelHeader(channel: openChannel, name: openName)
            if let messages = session.messages {
                messageArea(messages)
            } else if session.currentUser == nil {
                EmptyState(text: nil)
            } else if session.place == .home {
                EmptyState(text: "No conversation open")
            } else {
                EmptyState(text: "No text channels you can see here")
            }
        }
        .background(Color(nsColor: Palette.chat))
    }

    private var title: String {
        switch session.place {
        case .home: "Direct Messages"
        case .guild(let guildId): session.guilds.guilds.first { $0.id == guildId }?.name ?? "Akari"
        }
    }

    private var conversation: DirectMessageListModel.Conversation? {
        session.messages.flatMap { messages in
            session.directMessages.conversations.first { $0.id == messages.channelId }
        }
    }

    private var openChannel: Channel? {
        guard let id = session.messages?.channelId else {
            return nil
        }
        return session.channels?.channels.first { $0.id == id } ?? conversation?.channel
    }

    private var openName: String {
        conversation?.name ?? openChannel?.name ?? ""
    }
}
