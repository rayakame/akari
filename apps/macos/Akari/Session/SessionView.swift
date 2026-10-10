import AkariKit
import SwiftUI

struct SessionView<MessageArea: View>: View {
    let session: SessionModel
    var warning: AppModel.Warning?
    var dismissWarning: () -> Void = {}
    var reconnect: () -> Void = {}
    @ViewBuilder let messageArea: (MessageListModel, ChannelName) -> MessageArea
    @State private var collapsed = CollapsedCategories()
    @State private var sidebarWidth = SidebarWidth.stored(in: .standard)

    var body: some View {
        VStack(spacing: 0) {
            Color(nsColor: Palette.frame).frame(height: 32)
            ConnectionBar(notice: session.notice, reconnect: reconnect)
            // The login screen shows the logout warning; only this one belongs to a session.
            if warning == .tokenNotSaved, let warning {
                WarningLine(text: warning.text, dismiss: dismissWarning)
            }
            HStack(spacing: 0) {
                sidebar.frame(width: sidebarWidth)
                page
            }
            .overlay(alignment: .topLeading) { headerLine }
        }
        .ignoresSafeArea(edges: .top)
        .navigationTitle(title)
    }

    private var sidebar: some View {
        ZStack(alignment: .bottom) {
            HStack(spacing: 0) {
                ServerRail(session: session).frame(width: SidebarWidth.rail)
                Rectangle().fill(Color(nsColor: Palette.frameBorder)).frame(width: 1)
                // Over the list, not above it, so its header starts level with the channel header.
                list
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .overlay(alignment: .top) {
                        Rectangle().fill(Color(nsColor: Palette.frameBorder)).frame(height: 1)
                    }
            }
            UserPanel(user: session.currentUser).padding(8)
        }
        .background(Color(nsColor: Palette.frame))
        .overlay(alignment: .trailing) {
            SidebarResizer(width: $sidebarWidth)
        }
    }

    // One line under the server and channel headers; it starts where the channel list does, so
    // resizing the sidebar can't break it.
    private var headerLine: some View {
        Rectangle()
            .fill(Color(nsColor: Palette.borderSubtle))
            .frame(height: 1)
            .padding(.leading, session.channels == nil ? sidebarWidth : SidebarWidth.rail + 1)
            .offset(y: ChannelHeader.height)
            .allowsHitTesting(false)
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
                messageArea(messages, channelName)
            } else if session.currentUser == nil {
                EmptyState(text: nil)
            } else if session.place == .home {
                EmptyState(text: "No conversation open")
            } else {
                EmptyState(text: "No text channels you can see here")
            }
        }
        // A background reaches into the title bar by default, over the strip.
        .background(Color(nsColor: Palette.chat), ignoresSafeAreaEdges: [])
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

    private var channelName: ChannelName {
        ChannelName(inGuild: openChannel?.guildId != nil, name: openName)
    }
}
