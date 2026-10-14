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

    public let userId: UserId
    public private(set) var connection: ConnectionState = .offline
    public private(set) var currentUser: User?
    public let servers: ServerListModel
    public let directMessages: DirectMessageListModel
    /// Collapsed categories, Akari's own: Discord's clients don't sync them.
    public let collapsed: CollapsedCategories
    public private(set) var place: Place = .home
    /// The open guild's channels; `nil` at home.
    public private(set) var channels: ChannelListModel?
    /// The open channel's messages. The session can replace it on its own, when the open channel
    /// is deleted or hidden, and a new model loads nothing until the view calls `open()`, e.g.
    /// with `.task(id: messages.channelId) { await messages.open() }`.
    public private(set) var messages: MessageListModel?
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
    @ObservationIgnored private var spotToRestore: AccountMemory.Spot?
    @ObservationIgnored private var ended = false

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
            messages = MessageListModel(
                channelId: channel, account: account, store: store, drafts: drafts)
        }
    }

    /// The hosts the account trusts for masked links, kept with the rest of its memory.
    public func trustedLinks() -> TrustedLinks {
        memory.trustedLinks(of: userId)
    }

    /// A guild opens the channel last opened there, else its first text channel; home opens
    /// the conversation last opened there while it's listed.
    public func open(_ place: Place) {
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
                open(channel: last)
            } else {
                messages = nil
            }
            return
        }
        let list = ChannelListModel(guildId: guildId, store: store)
        channels = list
        openListedChannel(in: list, guildId: guildId)
    }

    public func open(channel id: ChannelId) {
        spotToRestore = nil
        switch place {
        case .guild(let guildId): lastChannels[guildId] = id
        case .home: lastHomeChannel = id
        }
        defer { rememberSpot() }
        guard messages?.channelId != id else {
            return
        }
        openedChannels.insert(id)
        messages = MessageListModel(channelId: id, account: account, store: store, drafts: drafts)
    }

    /// Opens a channel where it lives: in its guild, or at home for a conversation. A channel
    /// the store doesn't have opens where the user is; one without a message list doesn't open.
    public func go(to channel: ChannelId) {
        if let found = store.channel(id: channel) {
            guard found.opensMessageList else {
                return
            }
            let place: Place = found.guildId.map { .guild($0) } ?? .home
            if place != self.place {
                open(place)
            }
        }
        open(channel: channel)
    }

    /// Disconnects but keeps the session, e.g. before the Mac sleeps.
    public func suspend() {
        suspended = true
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
        account.close()
        subscription?.close()
        loop.cancel()
        if case .closed = connection {
            return
        }
        connection = .closed(error: nil)
    }

    func apply(_ batch: EventBatch) {
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
        if case .guild(let id) = place, batch.ready || batch.guildListChanged,
            store.guild(id: id) == nil
        {
            open(.home)
        }
        let open = messages?.channelId
        let wasListed = open.map(isListed) ?? false
        servers.apply(batch)
        directMessages.apply(batch)
        let muted = channels?.apply(batch) ?? []
        // Like the official client, which collapses a category as it's muted there.
        if case .guild(let guildId) = place {
            for category in muted {
                collapsed.collapse(category, in: guildId)
            }
        }
        // Threads and DMs aren't in the channel list; they leave only when removed.
        if let open, batch.removedChannels.contains(open) || (wasListed && !isListed(open)) {
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
        messages?.apply(batch)
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

    private func isListed(_ channel: ChannelId) -> Bool {
        channels?.channels.contains { $0.id == channel } == true
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
            open(.guild(guildId))
        case .home:
            open(.home)
            if let channel = spot.channel,
                directMessages.conversations.contains(where: { $0.id == channel })
            {
                open(channel: channel)
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
        let first = list.channels.first(where: \.opensMessageList)
        if let channel = last ?? first {
            open(channel: channel.id)
        } else {
            messages = nil
        }
    }

    private func closed(_ error: GatewayError?) {
        guard !ended else {
            return
        }
        ended = true
        connection = .closed(error: error)
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
