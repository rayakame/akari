import AkariKit
import SwiftUI

struct SessionView<MessageArea: View>: View {
    let session: SessionModel
    var warning: AppModel.Warning?
    var dismissWarning: () -> Void = {}
    var reconnect: () -> Void = {}
    @ViewBuilder let messageArea: (MessageListModel, ChannelName) -> MessageArea
    var splitWidth = SplitWidth.shared
    @State private var sidebarWidth = SidebarWidth.stored(in: .standard)

    var body: some View {
        VStack(spacing: 0) {
            TitleBar(title: windowTitle)
            ConnectionBar(notice: session.notice, reconnect: reconnect)
            // The login screen shows the logout warning; only this one belongs to a session.
            if warning == .tokenNotSaved, let warning {
                WarningLine(text: warning.text, dismiss: dismissWarning)
            }
            GeometryReader { geometry in
                let pageWidth = geometry.size.width - sidebarWidth
                HStack(spacing: 0) {
                    sidebar.frame(width: sidebarWidth)
                    page(width: pageWidth)
                }
                .overlay(alignment: .topLeading) { headerLine(pageWidth: pageWidth) }
            }
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

    // The full view's "…" for the open thread.
    private var threadMenu: ThreadMenu? {
        guard let open = openChannel, open.isThread,
            let item = session.membershipItem(for: open.id)
        else {
            return nil
        }
        return .fullView(membership: item)
    }

    private func act(_ item: ThreadMenu.Item, on thread: ChannelId, in menu: ThreadMenu) {
        switch item {
        case .membership(let membership):
            Task { [session] in
                if membership.joins {
                    await session.join(thread: thread, from: menu.location)
                } else {
                    await session.leave(thread: thread, from: menu.location)
                }
            }
        case .openInSplitView:
            session.openInSplitView()
        case .openInFullView:
            session.openInFullView()
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
    // resizing the sidebar can't break it. A docked split draws its own, after the gap.
    private func headerLine(pageWidth: CGFloat) -> some View {
        let docked = session.split != nil && !SplitWidth.floats(in: pageWidth)
        return Rectangle()
            .fill(Color(nsColor: Palette.borderSubtle))
            .frame(height: 1)
            .padding(.leading, hasListHeader ? SidebarWidth.rail + 1 : sidebarWidth)
            .padding(
                .trailing, docked ? SplitWidth.gap + splitWidth.shown(in: pageWidth) + 1 : 0)
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

    // The channel, and the thread beside it: docked after a gap, or over the channel when the
    // page is too narrow for both, as Discord's thread sidebar.
    @ViewBuilder private func page(width: CGFloat) -> some View {
        if let split = session.split, let thread = session.splitChannel {
            let splitShown = splitWidth.shown(in: width)
            if SplitWidth.floats(in: width) {
                ZStack(alignment: .trailing) {
                    channelCard(docked: false)
                    // Only the shadow is clipped to the page: clipping the card would clip its
                    // tables' layers.
                    UnevenRoundedRectangle(topLeadingRadius: SplitCardEdge.radius)
                        .fill(Color.black)
                        .shadow(color: .black.opacity(0.52), radius: 20, y: 8)
                        .frame(width: splitShown)
                        .frame(maxWidth: .infinity, alignment: .trailing)
                        .clipped()
                        .allowsHitTesting(false)
                    splitCard(split, thread: thread, floating: true)
                        .frame(width: splitShown)
                }
            } else {
                HStack(spacing: 0) {
                    channelCard(docked: true)
                    SplitGap(split: splitWidth, page: width)
                    splitCard(split, thread: thread, floating: false)
                        .frame(width: splitShown)
                }
                .background(Color(nsColor: Palette.frame), ignoresSafeAreaEdges: [])
            }
        } else {
            channelCard(docked: false)
        }
    }

    private func channelCard(docked: Bool) -> some View {
        main
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
            .overlay {
                if docked {
                    ZStack {
                        CornerCap(corner: .topTrailing).fill(Color(nsColor: Palette.frame))
                        ChannelCardEdge().stroke(Color(nsColor: Palette.frame), lineWidth: 1)
                        ChannelCardEdge().stroke(Color(nsColor: Palette.frameBorder), lineWidth: 1)
                    }
                    .allowsHitTesting(false)
                }
            }
    }

    // Discord's pane: a 1 border on its top and sides in the frame's border color (the chat's
    // background color while it floats), its top-left corner rounded 12 over what lies beneath.
    private func splitCard(
        _ split: MessageListModel, thread: Channel, floating: Bool
    ) -> some View {
        VStack(spacing: 0) {
            SplitHeader(
                thread: thread, menu: splitMenu,
                act: { item in
                    if let menu = splitMenu {
                        act(item, on: thread.id, in: menu)
                    }
                }, openFullView: session.openInFullView, close: session.closeSplit)
            messageArea(
                split,
                ChannelName(kind: .thread(owner: split.threadOwner), name: thread.name ?? ""))
        }
        .background(Color(nsColor: Palette.chat), ignoresSafeAreaEdges: [])
        .padding([.top, .horizontal], 1)
        .overlay {
            ZStack {
                CornerCap(corner: .topLeading)
                    .fill(Color(nsColor: floating ? Palette.chat : Palette.frame))
                SplitCardEdge().stroke(Color(nsColor: Palette.frame), lineWidth: 1)
                SplitCardEdge()
                    .stroke(
                        Color(nsColor: floating ? Palette.chat : Palette.frameBorder),
                        lineWidth: 1)
            }
            .allowsHitTesting(false)
        }
    }

    // The split view's "…" for its thread.
    private var splitMenu: ThreadMenu? {
        session.split.flatMap { session.membershipItem(for: $0.channelId) }.map {
            .splitView(membership: $0)
        }
    }

    private var main: some View {
        VStack(spacing: 0) {
            ChannelHeader(
                channel: openChannel, name: openName, parent: openParent,
                openParent: session.openParent, menu: threadMenu
            ) { item in
                if let open = openChannel, let menu = threadMenu {
                    act(item, on: open.id, in: menu)
                }
            }
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
        .frame(maxWidth: .infinity)
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
