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
        context.coordinator.viewer.focus = context.environment.windowFocus
        link.controller = context.coordinator
        context.coordinator.show(state)
    }

    static func dismantleNSView(_ scrollView: NSScrollView, coordinator: MessageTableController) {
        coordinator.dismantle()
    }
}

// Lets the SwiftUI parts around the table (jump bar, composer) reach its controller.
final class MessageTableLink {
    weak var controller: MessageTableController?
    weak var composer: ComposerTextView?
}

// Escape in either pane jumps both tables to the present (Q37).
@MainActor
final class PaneTables {
    private let links = NSHashTable<MessageTableLink>.weakObjects()

    func add(_ link: MessageTableLink) {
        links.add(link)
    }

    func jumpToPresent() {
        for link in links.allObjects {
            link.controller?.jumpToPresent(load: false)
        }
    }
}

struct MessageArea: View {
    let messages: MessageListModel
    let name: ChannelName
    let links: LinkFollower
    let go: (ChannelId) -> Void
    // A thread and the channel whose line names it.
    let openThread: (ChannelId, ChannelId) -> Void
    var openThreads: (ChannelId) -> Void = { _ in }
    // What this list's messages offer in their hover bars and menus.
    var messageActions: MessageActionsModel?
    // A clicked emoji's or sticker's popout.
    var expressions: (ExpressionTarget) -> ExpressionPopoutModel? = { _ in nil }
    // A forum post's Follow or Following.
    var followPost: (Bool) -> Void = { _ in }
    let openGuild: (GuildId) -> Void
    // The open channel, as its menu would.
    let markRead: () -> Void
    let markServerRead: () -> Void
    var tables: PaneTables?
    // Without it, Escape is this list's own.
    var escape: (() -> Void)?
    var autoFocus = true
    var focusNow = false
    var onFocused: () -> Void = {}
    var images: ImageCache?
    var placeholders: PlaceholderCache?
    var motion = MotionPolicy.default
    var focused = false
    /// `view_image_descriptions`: media with a description shows its ALT button.
    var showsAltText = false
    var ageGate = AgeGate.open
    var agreeToAgeGate: () -> Void = {}
    var leaveAgeGate: () -> Void = {}
    @State private var link = MessageTableLink()

    var body: some View {
        if ageGate != .open {
            AgeGateView(gate: ageGate, agree: agreeToAgeGate, back: leaveAgeGate)
        } else {
            list
        }
    }

    @ViewBuilder private var list: some View {
        let bridge = MessageListBridge(
            messages: messages, link: link, links: links, go: go, openThread: openThread,
            openThreads: openThreads, openGuild: openGuild, markServerRead: markServerRead,
            escapeAll: escape, messageActions: messageActions, expressions: expressions,
            followPost: followPost)
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
                            if let kind = JumpBar.kind(
                                atPresent: messages.atPresent, isStale: messages.isStale,
                                hasRows: !messages.rows.isEmpty,
                                fromReply: messages.replyReturn != nil)
                            {
                                JumpBar(kind) {
                                    if kind == .reply {
                                        bridge.returnToReply()
                                    } else {
                                        bridge.jumpToPresent()
                                    }
                                }
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
                    onAttach: { link.composer = $0 },
                    autoFocus: autoFocus, focusNow: focusNow, onFocused: onFocused,
                    onReplyJump: { bridge.jump(to: $0) }
                )
                .id(messages.channelId)
            }
        }
        .task(id: messages.channelId) { await messages.open() }
        .onAppear { [link] in tables?.add(link) }
    }

    private var state: MessageTableState {
        MessageTableState(
            rows: messages.rows, atPresent: messages.atPresent,
            reachedOldest: messages.reachedOldest, loading: messages.loading,
            failedLoads: messages.failedLoads, start: name.start, postBar: messages.postBar,
            channelId: messages.channelId, lastLoad: messages.lastLoad,
            openedAt: messages.openedAt, openedFrom: messages.openedFrom,
            revealedSpoilers: messages.revealedSpoilers, revealedMedia: messages.revealedMedia,
            shownSensitive: messages.shownSensitive, showsAltText: showsAltText,
            hiddenDownloads: messages.channelsHidingDownloads,
            newMessages: messages.newMessagesLine,
            unreadJumps: messages.unreadJumps, messageJump: messages.messageJump,
            replyingTo: messages.replyingTo,
            startRoom: messages.composer.locked ? LockedBanner.room : 0,
            images: ImageCacheRef(images, placeholders: placeholders), motion: motion,
            focused: focused)
    }
}

final class MessageListBridge: MessageListActions {
    let messages: MessageListModel
    let link: MessageTableLink
    let links: LinkFollower
    let go: (ChannelId) -> Void
    let thread: (ChannelId, ChannelId) -> Void
    let threads: (ChannelId) -> Void
    let guild: (GuildId) -> Void
    let serverRead: () -> Void
    let escapeAll: (() -> Void)?
    let messageActions: MessageActionsModel?
    let expressions: (ExpressionTarget) -> ExpressionPopoutModel?
    let follow: (Bool) -> Void

    init(
        messages: MessageListModel, link: MessageTableLink, links: LinkFollower,
        go: @escaping (ChannelId) -> Void,
        openThread: @escaping (ChannelId, ChannelId) -> Void = { _, _ in },
        openThreads: @escaping (ChannelId) -> Void = { _ in },
        openGuild: @escaping (GuildId) -> Void = { _ in },
        markServerRead: @escaping () -> Void,
        escapeAll: (() -> Void)? = nil,
        messageActions: MessageActionsModel? = nil,
        expressions: @escaping (ExpressionTarget) -> ExpressionPopoutModel? = { _ in nil },
        followPost: @escaping (Bool) -> Void = { _ in }
    ) {
        self.messages = messages
        self.link = link
        self.links = links
        self.go = go
        thread = openThread
        threads = openThreads
        guild = openGuild
        serverRead = markServerRead
        self.escapeAll = escapeAll
        self.messageActions = messageActions
        self.expressions = expressions
        follow = followPost
    }

    func followPost(_ follows: Bool) {
        follow(follows)
    }

    func copyPostLink() {
        guard let link = messages.postLink else {
            return
        }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(link, forType: .string)
    }

    func expressionPopout(_ target: ExpressionTarget) -> ExpressionPopoutModel? {
        expressions(target)
    }

    func retry(_ message: MessageId) {
        Task { await messages.composer.retry(message) }
    }

    func openMedia(_ key: ImageKey) {
        Task { [messages] in
            if let url = await messages.originalURL(key) {
                NSWorkspace.shared.open(url)
            }
        }
    }

    func viewer(_ items: [ViewerModel.Item], at index: Int, of row: MessageListModel.Row) -> ViewerModel? {
        messages.viewer(items, at: index, of: row)
    }

    func revealMedia(_ key: MediaKey) {
        messages.reveal(media: key)
    }

    func mediaURL(_ key: ImageKey) async -> URL? {
        await messages.originalURL(key)
    }

    func hidesDownloads(of row: MessageListModel.Row) -> Bool {
        messages.hidesDownloads(of: row)
    }

    func saveOriginal(_ key: ImageKey, to destination: URL) async throws {
        _ = try await messages.saveOriginal(key, to: destination)
    }

    func toggleSensitive(_ key: MediaKey) {
        messages.toggleSensitive(key)
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
        if let escapeAll {
            escapeAll()
        } else {
            // A pending reply takes the key: nothing is marked and nothing scrolls.
            let replying = messages.composer.reply != nil
            Task { await messages.escape() }
            if !replying {
                link.controller?.jumpToPresent(load: false)
            }
        }
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

    func openThreads(of channel: ChannelId) {
        threads(channel)
    }

    func jump(to message: MessageId) {
        Task { await messages.jump(to: message) }
    }

    func jumpToTarget(of row: MessageListModel.Row) {
        Task { await messages.jumpToTarget(of: row) }
    }

    func returnToReply() {
        Task { await messages.returnToReply() }
    }

    func jumpShown(_ number: Int) {
        messages.shownJump = max(messages.shownJump, number)
    }

    func lastShownJump() -> Int {
        messages.shownJump
    }

    func openGuild(_ id: GuildId) {
        guild(id)
    }

    func openLink(_ url: URL, masked: Bool) {
        links.follow(url, masked: masked)
    }

    func messageBar(for row: MessageListModel.Row, shift: Bool) -> [MessageActionItem] {
        messageActions?.bar(for: row, shift: shift) ?? []
    }

    func messageMenu(for row: MessageListModel.Row) -> [[MessageActionItem]] {
        messageActions?.menu(for: row) ?? []
    }

    func perform(_ action: MessageAction, on row: MessageListModel.Row, shift: Bool) {
        messageActions?.perform(action, on: row, shift: shift)
    }

    func markUnread(from row: MessageListModel.Row) {
        Task { [messageActions] in await messageActions?.markUnread(from: row) }
    }
}
