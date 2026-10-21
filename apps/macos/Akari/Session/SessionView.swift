import AkariKit
import SwiftUI

struct SessionView<MessageArea: View>: View {
    let session: SessionModel
    var warning: AppModel.Warning?
    var dismissWarning: () -> Void = {}
    var reconnect: () -> Void = {}
    @ViewBuilder let messageArea: (MessageListModel, ChannelName) -> MessageArea
    @State private var sidebarWidth = SidebarWidth.stored(in: .standard)

    var body: some View {
        VStack(spacing: 0) {
            TitleBar(title: windowTitle)
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
        .navigationTitle(windowTitle.text)
        .alert(
            session.membershipError?.title ?? "",
            isPresented: Binding(
                get: { session.membershipError != nil },
                set: { if !$0 { session.dismissMembershipError() } }),
            presenting: session.membershipError
        ) { _ in
            Button("OK") { session.dismissMembershipError() }
        } message: { error in
            Text(error.message)
        }
    }

    // The full view's "…" offers joining or leaving the open thread (Toolbar Overflow).
    private var membership: ChannelHeader.Membership? {
        guard let open = openChannel, open.isThread,
            let item = session.membershipItem(for: open.id)
        else {
            return nil
        }
        return ChannelHeader.Membership(item: item) { [session] in
            Task {
                if item.joins {
                    await session.join(thread: open.id, from: .toolbarOverflow)
                } else {
                    await session.leave(thread: open.id, from: .toolbarOverflow)
                }
            }
        }
    }

    private var sidebar: some View {
        ZStack(alignment: .bottom) {
            HStack(spacing: 0) {
                ServerRail(session: session).frame(width: SidebarWidth.rail)
                // Over the list, not above it, so its header starts level with the channel header.
                list
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
                    .padding(.leading, 1)
                    .overlay {
                        ContentCorner().stroke(Color(nsColor: Palette.frameBorder), lineWidth: 1)
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
            .padding(.leading, hasListHeader ? SidebarWidth.rail + 1 : sidebarWidth)
            .offset(y: ChannelHeader.height)
            .allowsHitTesting(false)
    }

    private var hasListHeader: Bool {
        session.place == .home || session.channels != nil
    }

    @ViewBuilder private var list: some View {
        switch session.place {
        case .home:
            DirectMessageList(session: session)
        case .guild(let guildId):
            if let channels = session.channels {
                ChannelSidebar(
                    session: session, list: channels, collapsed: session.collapsed,
                    guildName: session.servers.guilds.first { $0.id == guildId }?.name ?? "")
            }
        }
    }

    private var page: some View {
        VStack(spacing: 0) {
            ChannelHeader(
                channel: openChannel, name: openName, parent: openParent,
                openParent: session.openParent, membership: membership)
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
        // The top edge of the list's frame runs on over the page, in the same color: the
        // official line is one color across the window, so it's laid over the frame's.
        .overlay(alignment: .top) {
            // Not a background: that would reach up into the title bar, over the title.
            ZStack {
                Color(nsColor: Palette.frame)
                Color(nsColor: Palette.frameBorder)
            }
            .frame(height: 1)
        }
    }

    private var windowTitle: WindowTitle {
        let server: String? =
            switch session.place {
            case .home: nil
            case .guild(let guildId): session.servers.guilds.first { $0.id == guildId }?.name
            }
        // The avatar the DM list shows for it.
        let open = conversation.map { conversation in
            let person = conversation.channel.kind == .dm ? conversation.recipients.first : nil
            let avatarId = person?.id.rawValue ?? conversation.id.rawValue
            return (name: conversation.name, avatarId: avatarId)
        }
        return WindowTitle.at(session.place, server: server, conversation: open)
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
        return session.channels?.channel(id) ?? conversation?.channel
    }

    private var openParent: Channel? {
        guard let openChannel, openChannel.isThread, let parentId = openChannel.parentId else {
            return nil
        }
        return session.channels?.channel(parentId)
    }

    private var openName: String {
        conversation?.name ?? openChannel?.name ?? ""
    }

    private var channelName: ChannelName {
        let kind: ChannelName.Kind =
            switch openChannel?.kind {
            case .dm: .direct(conversation?.recipients.first)
            case .groupDm: .group
            case .publicThread?, .privateThread?, .newsThread?:
                .thread(owner: session.messages?.threadOwner)
            case let kind?: .guildChannel(kind)
            case nil: .guildChannel(.guildText)
            }
        return ChannelName(kind: kind, name: openName)
    }
}
