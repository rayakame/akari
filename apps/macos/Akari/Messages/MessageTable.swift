import AkariKit
import AppKit
import SwiftUI

// Fed with values read in a SwiftUI body, so Observation drives the updates.
struct MessageTable: NSViewRepresentable {
    let state: MessageTableState
    let actions: MessageListActions
    let link: MessageTableLink

    func makeCoordinator() -> MessageTableController {
        MessageTableController()
    }

    func makeNSView(context: Context) -> NSScrollView {
        context.coordinator.scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        context.coordinator.actions = actions
        link.controller = context.coordinator
        context.coordinator.show(state)
    }
}

// Lets the SwiftUI parts around the table (jump bar, composer) reach its controller.
final class MessageTableLink {
    weak var controller: MessageTableController?
    weak var composer: ComposerTextView?
}

struct MessageArea: View {
    let messages: MessageListModel
    let name: ChannelName
    let links: LinkFollower
    let go: (ChannelId) -> Void
    // A thread and the channel whose line names it.
    let openThread: (ChannelId, ChannelId) -> Void
    let openGuild: (GuildId) -> Void
    // The open channel, as its menu would.
    let markRead: () -> Void
    let markServerRead: () -> Void
    @State private var link = MessageTableLink()

    var body: some View {
        let bridge = MessageListBridge(
            messages: messages, link: link, links: links, go: go, openThread: openThread,
            openGuild: openGuild, markServerRead: markServerRead)
        GeometryReader { geometry in
            VStack(spacing: 0) {
                MessageTable(state: state, actions: bridge, link: link)
                    .id(messages.channelId)
                    .overlay(alignment: .top) {
                        VStack(spacing: 0) {
                            if messages.composer.locked {
                                LockedBanner()
                            }
                            if let bar = messages.unreadBar {
                                UnreadBar(
                                    text: UnreadBar.text(bar), jump: { bridge.jumpToUnread() },
                                    markRead: markRead)
                            }
                            if StaleCapsule.isShown(
                                atPresent: messages.atPresent, isStale: messages.isStale,
                                hasRows: !messages.rows.isEmpty)
                            {
                                StaleCapsule()
                            }
                        }
                    }
                    .overlay(alignment: .bottom) {
                        VStack(spacing: 0) {
                            if let text = JumpBar.text(
                                atPresent: messages.atPresent, isStale: messages.isStale,
                                hasRows: !messages.rows.isEmpty)
                            {
                                JumpBar(text: text) { bridge.jumpToPresent() }
                            }
                            // Above the composer, as in the official client: send errors on
                            // the left, slowmode on the right, over the list's last pixels.
                            ComposerStatus(composer: messages.composer)
                        }
                    }
                ComposerView(
                    composer: messages.composer, placeholder: name.placeholder,
                    areaHeight: geometry.size.height,
                    onSend: { link.controller?.jumpToPresent(load: false) },
                    onEscape: { bridge.escape() },
                    onShiftEscape: { bridge.markServerRead() },
                    onShiftPageUp: { bridge.jumpToUnread() },
                    onAttach: { link.composer = $0 }
                )
                .id(messages.channelId)
            }
        }
        .task(id: messages.channelId) { await messages.open() }
    }

    private var state: MessageTableState {
        MessageTableState(
            rows: messages.rows, atPresent: messages.atPresent,
            reachedOldest: messages.reachedOldest, loading: messages.loading,
            failedLoads: messages.failedLoads, start: name.start,
            channelId: messages.channelId, lastLoad: messages.lastLoad,
            openedAt: messages.openedAt, openedFrom: messages.openedFrom,
            revealedSpoilers: messages.revealedSpoilers, newMessages: messages.newMessagesLine,
            unreadJumps: messages.unreadJumps, messageJump: messages.messageJump,
            startRoom: messages.composer.locked ? LockedBanner.room : 0)
    }
}

final class MessageListBridge: MessageListActions {
    let messages: MessageListModel
    let link: MessageTableLink
    let links: LinkFollower
    let go: (ChannelId) -> Void
    let thread: (ChannelId, ChannelId) -> Void
    let guild: (GuildId) -> Void
    let serverRead: () -> Void

    init(
        messages: MessageListModel, link: MessageTableLink, links: LinkFollower,
        go: @escaping (ChannelId) -> Void,
        openThread: @escaping (ChannelId, ChannelId) -> Void = { _, _ in },
        openGuild: @escaping (GuildId) -> Void = { _ in },
        markServerRead: @escaping () -> Void
    ) {
        self.messages = messages
        self.link = link
        self.links = links
        self.go = go
        thread = openThread
        guild = openGuild
        serverRead = markServerRead
    }

    func retry(_ message: MessageId) {
        Task { await messages.composer.retry(message) }
    }

    func delete(_ message: MessageId) {
        messages.composer.discard(message)
    }

    func loadMore(_ edge: MessageTimeline.Edge) {
        Task {
            switch edge {
            case .older: await messages.loadOlder()
            case .newer: await messages.loadNewer()
            case .beginning: break
            }
        }
    }

    func jumpToLatest() {
        Task { await messages.jumpToPresent() }
    }

    func escape() {
        Task { await messages.escape() }
        link.controller?.jumpToPresent(load: false)
        link.composer?.takeFocus()
    }

    // The jump bar's button jumps like Escape but leaves marking read to reading.
    func jumpToPresent() {
        link.controller?.jumpToPresent(load: true)
        link.composer?.takeFocus()
    }

    func markServerRead() {
        serverRead()
    }

    func jumpToUnread() {
        Task { await messages.jumpToUnread() }
    }

    func newMessagesLineOnScreen(_ onScreen: Bool, line: MessageId) {
        // Measured a turn earlier: it would count for a line the model has moved since.
        guard line == messages.newMessagesLine else {
            return
        }
        messages.setNewMessagesLineOnScreen(onScreen)
    }

    func atBottomChanged(_ atBottom: Bool) {
        messages.setAtBottom(atBottom)
    }

    func userScrolled(by distance: CGFloat, height: CGFloat) {
        messages.userScrolled(by: distance, height: height)
    }

    func reveal(_ spoiler: SpoilerKey) {
        messages.reveal(spoiler)
    }

    func openChannel(_ id: ChannelId) {
        go(id)
    }

    func openThread(_ id: ChannelId, from parent: ChannelId) {
        thread(id, parent)
    }

    func jump(to message: MessageId) {
        Task { await messages.jump(to: message) }
    }

    func openGuild(_ id: GuildId) {
        guild(id)
    }

    func openLink(_ url: URL, masked: Bool) {
        links.follow(url, masked: masked)
    }
}
