import Observation
import os

/// A logged-in account's session: its connection, the current user, and the guild, channel and
/// message lists, all fed in order from one store subscription.
@MainActor @Observable
public final class SessionModel {
    public enum Place: Hashable, Sendable {
        case home
        case guild(GuildId)
    }

    /// "Join Thread" or "Leave Thread" in a thread's menus.
    public struct MembershipItem: Equatable, Sendable {
        public let joins: Bool
        /// Archived threads can be neither joined nor left.
        public let enabled: Bool

        public var title: String { joins ? "Join Thread" : "Leave Thread" }

        public init(joins: Bool, enabled: Bool) {
            self.joins = joins
            self.enabled = enabled
        }
    }

    /// Why joining or leaving a thread failed, in Discord's words.
    public struct MembershipError: Equatable, Sendable {
        public let title: String
        public let message: String

        public init(title: String, message: String) {
            self.title = title
            self.message = message
        }
    }

    public let userId: UserId
    public private(set) var connection: ConnectionState = .offline
    public private(set) var currentUser: User?
    public let servers: ServerListModel
    public let directMessages: DirectMessageListModel
    /// Collapsed categories, Akari's own: Discord's clients don't sync them.
    public let collapsed: CollapsedCategories
    public private(set) var place: Place = .home
    /// The open guild's channels; `nil` at home.
    public private(set) var channels: ChannelListModel? {
        didSet { channels?.setOpen(messages?.channelId) }
    }
    /// The open channel's messages. The session can replace it on its own, when the open channel
    /// is deleted or hidden, and a new model loads nothing until the view calls `open()`, e.g.
    /// with `.task(id: messages.channelId) { await messages.open() }`.
    public private(set) var messages: MessageListModel? {
        didSet {
            oldValue?.retire()
            channels?.setOpen(messages?.channelId)
            if oldValue?.channelId != messages?.channelId {
                threadBrowser = nil
            }
            syncSplit()
        }
    }
    /// The thread beside the open channel, in the split view. Each channel keeps its own:
    /// opening another channel hides it, coming back shows it again, and so does a relaunch,
    /// unless Discord says the thread is gone or the user can no longer see it. Like `messages`,
    /// a new model loads nothing until the view calls `open()`.
    public private(set) var split: MessageListModel? {
        didSet { oldValue?.retire() }
    }
    /// The split thread's channel, for its header and menu.
    public private(set) var splitChannel: Channel?
    /// Set when the user opened the split, so its composer takes the focus; the view calls
    /// `splitFocused()` once it has.
    public private(set) var focusSplit = false
    /// Set when the X or Escape closed the split, so the channel's composer takes the focus; the
    /// view calls `channelFocused()` once it has.
    public private(set) var focusChannel = false
    /// The open Threads browser: the popout under the header's Threads button or the modal "See
    /// all threads" opens. Opening another channel closes it.
    public private(set) var threadBrowser: ThreadBrowserModel?
    /// The last join or leave that failed, until `dismissMembershipError()`.
    public private(set) var membershipError: MembershipError?
    /// The channels the user is reading: the open channel and the split thread, each while its
    /// list is fresh at the present's bottom, and unless the user scrolled clearly away from it
    /// or the session showed it on its own since the last click or key press; both only while
    /// the session is online, the app active, its window key and visible and the user engaged.
    /// The core hears of a change a list reports on the next turn.
    public private(set) var reading: Set<ChannelId> = []
    /// What the connection bar says; `nil` while online.
    public var notice: ConnectionNotice? {
        switch connection {
        case .online: nil
        case .offline where suspended: .offline
        case .offline, .connecting: wasOnline ? .reconnecting : .connecting
        case .closed(let error): .closed(error)
        }
    }

    let loop = EventLoop()
    let drafts: Drafts
    private var wasOnline = false
    // Before the first connect the store says offline too; only a suspend is "offline".
    private var suspended = false
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let onClosed: @MainActor (GatewayError?) -> Void
    @ObservationIgnored private let memory: AccountMemory
    @ObservationIgnored private var subscription: StoreSubscription?
    @ObservationIgnored private var lastChannels: [GuildId: ChannelId] = [:]
    @ObservationIgnored private var lastHomeChannel: ChannelId?
    // Only channels opened here can hold this session's unsent messages.
    @ObservationIgnored private var openedChannels: Set<ChannelId> = []
    @ObservationIgnored private var clickedThread: ChannelId?
    // Each channel's thread in the split view, with the channel's server, kept across launches.
    @ObservationIgnored private var splits: [ChannelId: AccountMemory.Split]
    // Split threads Discord answered for, or the user opened, since this session's READY.
    @ObservationIgnored private var checkedSplits: Set<ChannelId> = []
    // One Get Channel per thread at a time: a box click shares the launch's.
    @ObservationIgnored private var threadRequests: [ChannelId: Task<Bool?, Never>] = [:]
    @ObservationIgnored private var sawReady = false
    @ObservationIgnored private(set) var splitFetch: Task<Void, Never>?
    @ObservationIgnored private(set) var escaping: Task<Void, Never>?
    @ObservationIgnored private var spotToRestore: AccountMemory.Spot?
    @ObservationIgnored private var ended = false
    @ObservationIgnored private var appActive = false
    @ObservationIgnored private var windowKey = false
    @ObservationIgnored private var windowVisible = false
    // The official client acks after a break only once the user clicks or types (D88).
    @ObservationIgnored private var engaged = false
    // Lists the user scrolled clearly away from, or a split the session showed on its own: they
    // wait for the next click or key press.
    @ObservationIgnored private var waitingLists: Set<ChannelId> = []
    @ObservationIgnored private var holdingReading = false
    // The core forgets the channel whenever the connection drops; a call that raced the drop
    // may have set it again, so the next derivation is sent whatever it is.
    @ObservationIgnored private var resendReading = false
    @ObservationIgnored private var toldReading: [ChannelId] = []
    // On a 401 the core has stopped reading itself, and close() can come inside that batch.
    @ObservationIgnored private var accountClosed = false
    // The list reports from inside the table's scroll callbacks, too often for a call into the
    // core each; it hears the last value.
    @ObservationIgnored private(set) var pendingReading: Task<Void, Never>?

    /// `onClosed` runs once when the session ends without `close()`; `AuthenticationFailed`
    /// means the user has to log in again. The first READY reopens the place and channel
    /// `memory` holds for the account.
    public convenience init(
        userId: UserId, account: Account, memory: AccountMemory = AccountMemory(),
        onClosed: @escaping @MainActor (GatewayError?) -> Void
    ) {
        self.init(
            userId: userId, account: account, memory: memory, drafts: Drafts(),
            onClosed: onClosed)
    }

    init(
        userId: UserId, account: Account, memory: AccountMemory, drafts: Drafts,
        onClosed: @escaping @MainActor (GatewayError?) -> Void
    ) {
        self.userId = userId
        self.account = account
        self.memory = memory
        self.drafts = drafts
        self.onClosed = onClosed
        spotToRestore = memory.lastSpot(of: userId)
        splits = memory.splits(of: userId)
        store = account.store()
        servers = ServerListModel(store: store, account: userId, memory: memory)
        directMessages = DirectMessageListModel(store: store)
        collapsed = memory.collapsedCategories()
    }

    deinit {
        loop.cancel()
    }

    /// Subscribes, reads, connects and applies changes until the session ends.
    public func start() {
        guard subscription == nil, !ended else {
            return
        }
        let subscription = store.subscribe()
        self.subscription = subscription
        connection = store.connection()
        currentUser = store.currentUser()
        servers.reload()
        directMessages.reload()
        // Restored before this subscription, so no event will say so.
        if currentUser != nil {
            restoreFromCache()
        }
        do {
            try account.connect()
        } catch {
            subscription.close()
            closed(error as? GatewayError)
            return
        }
        loop.start(
            Task { [weak self] in
                await withTaskCancellationHandler {
                    while let events = await subscription.next() {
                        guard let self else {
                            return
                        }
                        self.apply(EventBatch(events))
                    }
                } onCancel: {
                    subscription.close()
                }
            })
        LaunchLog.mark("session started")
        // The store keeps a window for a channel it doesn't know yet, so the last channel's
        // messages load while the gateway connects; READY then places it.
        if let channel = spotToRestore?.channel {
            openedChannels.insert(channel)
            messages = list(of: channel)
        }
        // Reopening the last place counts as opening it, also when the cache or READY does.
        engage()
        updateReading()
    }

    /// The hosts the account trusts for masked links, kept with the rest of its memory.
    public func trustedLinks() -> TrustedLinks {
        memory.trustedLinks(of: userId)
    }

    /// A guild opens the channel last opened there, else its first text channel; home opens
    /// the conversation last opened there while it's listed.
    public func open(_ place: Place) {
        engage()
        enter(place)
    }

    /// A row of the channel or DM list. Opening a channel engages the user, also the one
    /// already open. As in the official client, the open channel's row closes its split, and a
    /// thread's row closes its channel's split, which the channel forgets.
    public func open(channel id: ChannelId) {
        engage()
        if id == messages?.channelId {
            forgetSplit(of: id)
        }
        if let thread = store.channel(id: id), thread.isThread, let parent = thread.parentId {
            forgetSplit(of: parent)
        }
        show(channel: id)
    }

    /// Opens a channel where it lives: in its guild, or at home for a conversation. A channel
    /// the store doesn't have opens where the user is; one without a message list doesn't open.
    public func go(to channel: ChannelId) {
        // The place opens its own last or first channel on the way, which isn't being read.
        readingOnce {
            if let found = store.channel(id: channel) {
                guard found.opensMessageList else {
                    return
                }
                let place: Place = found.guildId.map { .guild($0) } ?? .home
                if place != self.place {
                    open(place)
                }
            }
            engage()
            show(channel: channel)
        }
    }

    /// The box under a message in the open channel `parent`, or a "started a thread" line's
    /// thread name there: opens the thread beside the channel, in place of the one there. A
    /// thread the store doesn't have (archived, or never synced) is fetched first; nothing opens
    /// when Discord refuses it, it's gone or the fetch fails, nor once the user opened something
    /// else or clicked another thread meanwhile.
    public func openBeside(thread id: ChannelId, from parent: ChannelId) async {
        guard messages?.channelId == parent else {
            return
        }
        clickedThread = id
        engage()
        if store.channel(id: id) == nil {
            let opened = messages
            let found = await ask(about: id, in: parent, check: false) ?? false
            guard found, !ended, clickedThread == id, messages === opened else {
                return
            }
            // The launch's request may have shown it meanwhile; the click is the user's.
            waitingLists.remove(id)
        }
        if sawReady {
            checkedSplits.insert(id)
        }
        if let guild = store.channel(id: parent)?.guildId {
            splits[parent] = .init(thread: id, guild: guild)
            rememberSplits()
        }
        syncSplit()
        focusSplit = split != nil
        updateReading()
    }

    /// The split view's X: closes it, and the channel forgets it.
    public func closeSplit() {
        guard let open = messages?.channelId, split != nil else {
            return
        }
        forgetSplit(of: open)
        focusChannel = true
        updateReading()
    }

    /// The split view's "Open in Full View" and its title: the thread opens full width. The
    /// channel keeps its split, as in the official client, which only navigates there.
    public func openInFullView() {
        guard let thread = split?.channelId else {
            return
        }
        go(to: thread)
    }

    /// The split's composer has the focus the user asked for with `openBeside` or
    /// `openInSplitView`.
    public func splitFocused() {
        focusSplit = false
    }

    public func channelFocused() {
        focusChannel = false
    }

    private func forgetSplit(of channel: ChannelId) {
        guard splits[channel] != nil else {
            return
        }
        splits[channel] = nil
        rememberSplits()
        syncSplit()
    }

    /// The full view's "Open in Split View" for a thread: its channel opens with the thread
    /// beside it.
    public func openInSplitView() {
        guard let open = messages?.channelId, let thread = channels?.channel(open),
            thread.isThread, let parent = thread.parentId,
            let found = store.channel(id: parent), found.opensMessageList,
            let guild = found.guildId
        else {
            return
        }
        splits[parent] = .init(thread: open, guild: guild)
        if sawReady {
            checkedSplits.insert(open)
        }
        rememberSplits()
        go(to: parent)
        focusSplit = split != nil
    }

    /// Escape in either list: marks the open channel and the split thread read, ends both new
    /// messages lines, and loads each present a list doesn't show; scrolling both to the bottom
    /// is the view's part. With nothing to mark or load in either, it ends both lines and closes
    /// the split instead, as the official client does, also with a list scrolled up, and returns
    /// false: nothing to scroll.
    @discardableResult public func escape() -> Bool {
        let lists = [messages, split].compactMap { $0 }
        if split != nil, !lists.contains(where: \.escapeHasWork) {
            for list in lists {
                list.endNewMessagesLine()
            }
            closeSplit()
            return false
        }
        escaping = Task {
            await withDiscardingTaskGroup { group in
                for list in lists {
                    group.addTask { await list.escape() }
                }
            }
        }
        return true
    }

    /// The channel whose threads the header's Threads button browses: the open text or
    /// announcement channel, or the parent of a thread open full width; `nil` elsewhere (forums,
    /// voice, DMs), where it doesn't show.
    public var threadsButtonParent: ChannelId? {
        guard let open = messages?.channelId, let channels, let shown = channels.channel(open)
        else {
            return nil
        }
        let browsed = shown.isThread ? shown.parentId.flatMap(channels.channel) : shown
        guard let browsed, browsed.kind == .guildText || browsed.kind == .guildNews else {
            return nil
        }
        return browsed.id
    }

    /// Opens `parent`'s Threads browser, in place of one already open.
    public func openThreadBrowser(of parent: ChannelId, as presentation: ThreadBrowserModel.Presentation) {
        engage()
        threadBrowser = ThreadBrowserModel(
            parentId: parent, presentation: presentation, userId: userId, store: store,
            account: account)
    }

    public func closeThreadBrowser() {
        threadBrowser = nil
    }

    /// A row of the Threads browser or the channel list's "More Active Threads": closes the
    /// browser; a click opens the thread full width and closes its channel's split, as a row
    /// does, and Shift+click opens it beside its channel, going there first.
    public func open(browsed thread: ChannelId, beside: Bool) async {
        let parent = threadBrowser?.parentId ?? store.channel(id: thread)?.parentId
        threadBrowser = nil
        guard let parent else {
            return
        }
        guard beside else {
            open(channel: thread)
            return
        }
        if messages?.channelId != parent {
            go(to: parent)
        }
        await openBeside(thread: thread, from: parent)
    }

    /// The active threads of a channel the list doesn't show (not joined, or joined but past
    /// their window), the newest message first, at most three: the channel list's "More Active
    /// Threads" popout.
    public func moreActiveThreads(of channel: ChannelId) -> [ThreadPeek] {
        let browser = store.threadBrowser(parentId: channel)
        let listed = Set(channels?.channels.map(\.id) ?? []).union(messages.map { [$0.channelId] } ?? [])
        let threads = store.channels(ids: (browser.joined + browser.others).filter { !listed.contains($0) })
        return threads
            .sorted { Self.newest($0) > Self.newest($1) }
            .prefix(3)
            .map { thread in
                let owner = thread.thread?.ownerId
                return ThreadPeek(
                    id: thread.id, name: thread.name ?? "", ownerId: owner,
                    owner: owner.flatMap { $0 == userId ? currentUser : store.user(id: $0) },
                    time: thread.thread.map { ThreadBrowserModel.lastActivity(thread, $0) }
                        ?? ThreadBox.time(of: thread.id.rawValue))
            }
    }

    private static func newest(_ thread: Channel) -> UInt64 {
        thread.thread?.lastMessageId?.rawValue ?? thread.id.rawValue
    }

    /// What a thread of the open server, or the split thread, offers in its menus; `nil` for
    /// anything else.
    public func membershipItem(for thread: ChannelId) -> MembershipItem? {
        let shown =
            channels?.channel(thread) ?? splitChannel.flatMap { $0.id == thread ? $0 : nil }
            ?? threadBrowser?.channel(thread)
        guard let info = shown?.thread else {
            return nil
        }
        return MembershipItem(joins: !info.joined, enabled: !info.archived)
    }

    /// Joins the thread; the store lists it once Discord says yes.
    public func join(thread: ChannelId, from location: ThreadLocation) async {
        await changeMembership(of: thread, joins: true, from: location)
    }

    /// Leaves the thread. An open thread stays open, its row on top until the user goes elsewhere.
    public func leave(thread: ChannelId, from location: ThreadLocation) async {
        await changeMembership(of: thread, joins: false, from: location)
    }

    public func dismissMembershipError() {
        membershipError = nil
    }

    private func changeMembership(
        of thread: ChannelId, joins: Bool, from location: ThreadLocation
    ) async {
        guard let item = membershipItem(for: thread), item.enabled, item.joins == joins else {
            return
        }
        do {
            if joins {
                try await account.joinThread(threadId: thread, location: location)
            } else {
                try await account.leaveThread(threadId: thread, location: location)
            }
        } catch {
            membershipError = Self.membershipError(error as? RequestError)
        }
    }

    // TOO_MANY_THREAD_MEMBERS.
    private static let threadFull: UInt32 = 30033

    private static func membershipError(_ error: RequestError?) -> MembershipError {
        if case .Discord(_, threadFull, _)? = error {
            return MembershipError(
                title: "Cannot join thread",
                message: "This thread has reached the maximum possible number of members.")
        }
        return MembershipError(title: "Error", message: "An error occurred, please try again.")
    }

    /// The thread header's link: opens the open thread's channel.
    public func openParent() {
        guard let open = messages?.channelId,
            let thread = channels?.channel(open), thread.isThread, let parent = thread.parentId
        else {
            return
        }
        go(to: parent)
    }

    // Without engaging: only the user's choices and the launch do.
    private func enter(_ place: Place) {
        defer { updateReading() }
        guard place != self.place else {
            // The session starts at home, so choosing home before READY cancels a server restore.
            if case .guild = spotToRestore?.place {
                spotToRestore = nil
                messages = nil
                rememberSpot()
            }
            return
        }
        // Where the user went before the first READY wins over where they were last time.
        spotToRestore = nil
        self.place = place
        defer { rememberSpot() }
        guard case .guild(let guildId) = place else {
            channels = nil
            if let last = lastHomeChannel,
                directMessages.conversations.contains(where: { $0.id == last })
            {
                show(channel: last)
            } else {
                messages = nil
            }
            return
        }
        let list = ChannelListModel(guildId: guildId, store: store)
        channels = list
        openListedChannel(in: list, guildId: guildId)
    }

    /// Whether "Mark As Read" has something to mark: the server's badge shows unread or
    /// mentions.
    public func canMarkRead(guild: GuildId) -> Bool {
        Self.showsUnread(servers.badge(of: guild))
    }

    public func canMarkRead(folder: ServerListModel.FolderItem) -> Bool {
        Self.showsUnread(servers.badge(of: folder))
    }

    /// A channel of the open server, or a conversation.
    public func canMarkRead(channel: ChannelId) -> Bool {
        (channels?.badges[channel] ?? directMessages.badges[channel]).map(Self.showsUnread)
            ?? false
    }

    /// Also ends the new messages line of the open channel when it is in the server.
    public func markRead(guild: GuildId) {
        account.markGuildsRead(guildIds: [guild])
        markedRead(guilds: [guild])
    }

    /// One bulk ack for all of the folder's servers.
    public func markRead(folder: ServerListModel.FolderItem) {
        let guilds = folder.guilds.map(\.id)
        account.markGuildsRead(guildIds: guilds)
        markedRead(guilds: guilds)
    }

    /// Also ends the new messages line when it is the open channel or the split thread.
    public func markRead(channel: ChannelId) {
        account.markRead(channelId: channel)
        for list in [messages, split] where list?.channelId == channel {
            list?.endNewMessagesLine()
        }
    }

    private func markedRead(guilds: [GuildId]) {
        if case .guild(let open) = place, guilds.contains(open) {
            messages?.endNewMessagesLine()
            split?.endNewMessagesLine()
        }
    }

    /// Shift+Escape: marks the open server read; does nothing at home.
    public func markServerRead() {
        if case .guild(let guildId) = place {
            markRead(guild: guildId)
        }
    }

    /// What reading needs from the host: the app active, and its window key and visible (not
    /// minimized or covered). Losing one ends the user's engagement until the next click or
    /// key press.
    public func setAppActive(_ active: Bool) {
        let lost = appActive && !active
        appActive = active
        readingInputChanged(lost: lost)
    }

    public func setWindowKey(_ key: Bool) {
        let lost = windowKey && !key
        windowKey = key
        readingInputChanged(lost: lost)
    }

    public func setWindowVisible(_ visible: Bool) {
        let lost = windowVisible && !visible
        windowVisible = visible
        readingInputChanged(lost: lost)
    }

    /// A click or key press in the window; scroll-wheel events aren't one. While suspended, a
    /// click doesn't count: the next one after `resume()` does.
    public func userInteracted() {
        guard !suspended else {
            return
        }
        engage()
        updateReading()
    }

    /// Disconnects but keeps the session, e.g. before the Mac sleeps. Reading resumes after the
    /// next click or key press.
    public func suspend() {
        suspended = true
        engaged = false
        updateReading()
        account.disconnect()
    }

    public func resume() {
        suspended = false
        do {
            try account.connect()
        } catch {
            closed(error as? GatewayError)
        }
    }

    func flushCache() async {
        await account.flushCache()
    }

    // A new account starts with an empty outbox, so its text moves into the drafts.
    func keepUnsentAsDrafts() {
        for channel in openedChannels.sorted() {
            guard let pending = store.window(channelId: channel)?.pendingIds, !pending.isEmpty
            else {
                continue
            }
            let lines = store.messages(channelId: channel, ids: pending).map(\.content)
            drafts.append(lines, to: channel)
        }
    }

    /// Ends the session. Releasing the model without calling it ends the event loop too.
    public func close() {
        ended = true
        updateReading()
        account.close()
        accountClosed = true
        subscription?.close()
        loop.cancel()
        if case .closed = connection {
            return
        }
        connection = .closed(error: nil)
    }

    func apply(_ batch: EventBatch) {
        if batch.ready || batch.leftOnline, !toldReading.isEmpty {
            resendReading = true
        }
        // A list that hasn't applied the batch yet, e.g. READY's stale mark, isn't being read.
        readingOnce { update(with: batch) }
    }

    private func update(with batch: EventBatch) {
        // Before anything here sets the channel as being read, which the core acks at once.
        if batch.ready {
            sawReady = true
            messages?.newSession()
            split?.newSession()
        }
        if let state = batch.connection {
            if case .closed(let error) = state {
                closed(error)
            } else {
                connection = state
                if state == .online {
                    wasOnline = true
                }
            }
        }
        if batch.everythingChanged || batch.currentUserChanged {
            currentUser = store.currentUser()
        }
        // A list the session replaces on its own isn't one the user chose to read.
        if case .guild(let id) = place, batch.ready || batch.guildListChanged,
            store.guild(id: id) == nil
        {
            engaged = false
            enter(.home)
        }
        let open = messages?.channelId
        // An open thread stays open when it stops being listed, as in the official client.
        let wasListed = open.flatMap(listed).map { !$0.isThread } ?? false
        // A thread needs its parent's VIEW_CHANNEL; losing it sends no THREAD_DELETE.
        let parent = open.flatMap { channels?.channel($0) }.flatMap {
            $0.isThread ? $0.parentId : nil
        }
        let parentWasListed = parent.flatMap(listed) != nil
        servers.apply(batch)
        directMessages.apply(batch)
        channels?.apply(batch)
        if batch.ready {
            forgetSplitsOfLeftServers()
        } else {
            keepSplits { channel, split in
                !batch.removedChannels.contains(channel)
                    && !batch.removedChannels.contains(split.thread)
                    && !batch.removedGuilds.contains(split.guild)
            }
        }
        // A split shown after READY came from a batch or Discord's answer, not the user.
        let onItsOwn = sawReady && !batch.ready
        // Before a channel the user can no longer see gives way, so its split goes with it.
        syncSplit(onItsOwn: onItsOwn)
        // DMs and threads the list doesn't show leave when removed, a thread also with its parent.
        if let open,
            batch.removedChannels.contains(open) || (wasListed && listed(open) == nil)
                || (parentWasListed && parent.flatMap(listed) == nil)
        {
            engaged = false
            if case .guild(let guildId) = place, let list = channels {
                openListedChannel(in: list, guildId: guildId)
            } else {
                messages = nil
                rememberSpot()
            }
        }
        if batch.restored {
            restoreFromCache()
        }
        if batch.ready {
            LaunchLog.mark("READY in the app")
        }
        if batch.ready, let spot = spotToRestore {
            spotToRestore = nil
            restore(spot)
        }
        syncSplit(onItsOwn: onItsOwn)
        if let thread = split?.channelId,
            batch.everythingChanged || batch.channelsChanged.values.contains(where: { $0.contains(thread) })
        {
            let read = store.channel(id: thread)
            if let read, read != splitChannel {
                splitChannel = read
            }
        }
        messages?.apply(batch)
        split?.apply(batch)
        threadBrowser?.apply(batch)
    }

    private func keepSplits(where keep: (ChannelId, AccountMemory.Split) -> Bool) {
        let kept = splits.filter { keep($0.key, $0.value) }
        if kept.count != splits.count {
            splits = kept
            rememberSplits()
        }
    }

    // READY leaves out a server the user left meanwhile and lists one that's down as unavailable.
    private func forgetSplitsOfLeftServers() {
        let down = Set(store.unavailableGuildIds())
        keepSplits { _, split in
            store.guild(id: split.guild) != nil || down.contains(split.guild)
        }
    }

    private func syncSplit(onItsOwn: Bool = false) {
        guard let open = messages?.channelId, let thread = splits[open]?.thread else {
            showSplit(nil)
            return
        }
        guard let found = store.channel(id: thread), found.isThread, found.parentId == open else {
            showSplit(nil)
            // Before READY the cache or the gateway may still bring it; without its channel
            // Discord's answer couldn't be placed.
            if sawReady, store.channel(id: open) != nil {
                checkSplit(thread, of: open)
            }
            return
        }
        guard canView(found) else {
            splits[open] = nil
            rememberSplits()
            showSplit(nil)
            return
        }
        // READY lists the joined threads; the cache keeps the others, deleted ones too.
        if sawReady, found.thread?.joined != true {
            checkSplit(thread, of: open)
        }
        guard split?.channelId != thread else {
            return
        }
        openedChannels.insert(thread)
        showSplit(list(of: thread), channel: found)
        if onItsOwn {
            waitingLists.insert(thread)
        }
    }

    // Discord sends no THREAD_DELETE when the user loses a private thread or its channel.
    private func canView(_ thread: Channel) -> Bool {
        guard let permissions = store.permissions(channelId: thread.id) else {
            return true
        }
        guard permissions.contains(.viewChannel) else {
            return false
        }
        guard let info = thread.thread, info.private, !info.joined else {
            return true
        }
        return permissions.contains(.manageThreads)
    }

    private func showSplit(_ list: MessageListModel?, channel: Channel? = nil) {
        if split !== list {
            split = list
            focusSplit = false
        }
        if splitChannel != channel {
            splitChannel = channel
        }
    }

    // Once per session. A failed request isn't Discord's answer: the split waits for the next
    // launch; an answer for a server the store lacks is asked again.
    private func checkSplit(_ thread: ChannelId, of parent: ChannelId) {
        guard checkedSplits.insert(thread).inserted else {
            return
        }
        splitFetch = Task { [weak self] in
            guard let self, let found = await ask(about: thread, in: parent, check: true), !ended
            else {
                return
            }
            if found {
                if messages?.channelId == parent {
                    syncSplit(onItsOwn: true)
                    updateReading()
                }
            } else if store.threadUnavailable(threadId: thread) {
                keepSplits { channel, split in channel != parent || split.thread != thread }
                syncSplit()
            } else {
                checkedSplits.remove(thread)
            }
        }
    }

    // `nil` when the request failed.
    private func ask(about thread: ChannelId, in parent: ChannelId, check: Bool) async -> Bool? {
        if let running = threadRequests[thread] {
            return await running.value
        }
        let request = Task { [account] in
            if check {
                try? await account.checkThread(parentId: parent, threadId: thread)
            } else {
                try? await account.fetchThread(parentId: parent, threadId: thread)
            }
        }
        threadRequests[thread] = request
        let found = await request.value
        if threadRequests[thread] == request {
            threadRequests[thread] = nil
        }
        return found
    }

    private func rememberSplits() {
        memory.remember(splits: splits, of: userId)
    }

    // Once per session: the cache's state shows the last place before READY.
    private func restoreFromCache() {
        LaunchLog.mark("cache restored")
        // A place the cache doesn't have, e.g. a guild unavailable at the last write, is READY's.
        guard let spot = spotToRestore, isCached(spot) else {
            return
        }
        spotToRestore = nil
        restore(spot)
    }

    private func isCached(_ spot: AccountMemory.Spot) -> Bool {
        switch spot.place {
        case .guild(let guildId):
            return servers.guilds.contains { $0.id == guildId }
        case .home:
            return spot.channel.map { channel in
                directMessages.conversations.contains { $0.id == channel }
            } ?? true
        }
    }

    private func listed(_ channel: ChannelId) -> Channel? {
        channels?.channels.first { $0.id == channel }
    }

    private func restore(_ spot: AccountMemory.Spot) {
        switch spot.place {
        case .guild(let guildId):
            guard servers.guilds.contains(where: { $0.id == guildId }) else {
                messages = nil
                rememberSpot()
                return
            }
            if let channel = spot.channel {
                lastChannels[guildId] = channel
            }
            enter(.guild(guildId))
        case .home:
            enter(.home)
            if let channel = spot.channel,
                directMessages.conversations.contains(where: { $0.id == channel })
            {
                show(channel: channel)
            } else {
                messages = nil
                rememberSpot()
            }
        }
    }

    private func rememberSpot() {
        memory.remember(.init(place: place, channel: messages?.channelId), of: userId)
    }

    private func openListedChannel(in list: ChannelListModel, guildId: GuildId) {
        let last = lastChannels[guildId].flatMap { last in
            list.channels.first { $0.id == last && $0.opensMessageList }
        }
        let first = list.channels.first { $0.opensMessageList && !$0.isThread }
        if let channel = last ?? first {
            show(channel: channel.id)
        } else {
            messages = nil
        }
    }

    private func show(channel id: ChannelId) {
        spotToRestore = nil
        switch place {
        case .guild(let guildId): lastChannels[guildId] = id
        case .home: lastHomeChannel = id
        }
        defer {
            rememberSpot()
            updateReading()
        }
        guard messages?.channelId != id else {
            return
        }
        openedChannels.insert(id)
        messages = list(of: id)
    }

    private func list(of channel: ChannelId) -> MessageListModel {
        let list = MessageListModel(
            channelId: channel, userId: userId, account: account, store: store, drafts: drafts)
        list.bottomChanged = { [weak self, weak list] scrolledAway in
            guard let self, let list, list === self.messages || list === self.split else {
                return
            }
            if scrolledAway {
                waitingLists.insert(list.channelId)
            }
            updateReading(later: true)
        }
        return list
    }

    private func engage() {
        engaged = true
        waitingLists.removeAll()
    }

    private func readingOnce(_ body: () -> Void) {
        let held = holdingReading
        holdingReading = true
        body()
        holdingReading = held
        updateReading()
    }

    private func readingInputChanged(lost: Bool) {
        if lost {
            engaged = false
        }
        updateReading()
    }

    private func updateReading(later: Bool = false) {
        guard !holdingReading else {
            return
        }
        // Not before READY: the core would ack inside READY's write, over Discord's position.
        let reads =
            connection == .online && !suspended && engaged && appActive && windowKey
            && windowVisible && !ended
        let lists = [messages, split].compactMap { $0 }.filter { list in
            reads && list.atPresentBottom && !waitingLists.contains(list.channelId)
        }
        for list in lists {
            list.readHere = true
        }
        let channels = Set(lists.map(\.channelId))
        if channels != reading {
            reading = channels
        }
        if !later {
            pendingReading?.cancel()
            pendingReading = nil
            tellReading()
        } else if pendingReading == nil {
            pendingReading = Task { [weak self] in
                guard let self, !Task.isCancelled else {
                    return
                }
                pendingReading = nil
                tellReading()
            }
        }
    }

    private func tellReading() {
        let channels = [messages, split].compactMap { $0?.channelId }.filter(reading.contains)
        guard !accountClosed, channels != toldReading || resendReading else {
            return
        }
        resendReading = false
        toldReading = channels
        account.setReading(channelIds: channels)
    }

    private static func showsUnread(_ badge: Badge) -> Bool {
        badge.unread || badge.mentions > 0
    }

    private func closed(_ error: GatewayError?) {
        guard !ended else {
            return
        }
        ended = true
        connection = .closed(error: error)
        updateReading()
        onClosed(error)
    }
}

// A Sendable holder, so the model's nonisolated deinit can cancel the loop.
final class EventLoop: Sendable {
    private let task = OSAllocatedUnfairLock<Task<Void, Never>?>(initialState: nil)

    var running: Task<Void, Never>? {
        task.withLock { $0 }
    }

    func start(_ task: Task<Void, Never>) {
        self.task.withLock { $0 = task }
    }

    func cancel() {
        running?.cancel()
    }
}
