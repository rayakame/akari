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
    public let guilds: GuildListModel
    public private(set) var place: Place = .home
    /// The open guild's channels; `nil` at home.
    public private(set) var channels: ChannelListModel?
    /// The open channel's messages. The session can replace it on its own, when the open channel
    /// is deleted or hidden, and a new model loads nothing until the view calls `open()`, e.g.
    /// with `.task(id: messages.channelId) { await messages.open() }`.
    public private(set) var messages: MessageListModel?

    let loop = EventLoop()
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let onClosed: @MainActor (GatewayError?) -> Void
    @ObservationIgnored private var subscription: StoreSubscription?
    @ObservationIgnored private var lastChannels: [GuildId: ChannelId] = [:]
    @ObservationIgnored private var ended = false

    /// `onClosed` runs once when the session ends without `close()`; `AuthenticationFailed`
    /// means the user has to log in again.
    public init(
        userId: UserId, account: Account,
        onClosed: @escaping @MainActor (GatewayError?) -> Void
    ) {
        self.userId = userId
        self.account = account
        self.onClosed = onClosed
        store = account.store()
        guilds = GuildListModel(store: store)
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
        guilds.reload()
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
    }

    /// A guild opens the channel last opened there, else its first text channel.
    public func open(_ place: Place) {
        guard place != self.place else {
            return
        }
        self.place = place
        guard case .guild(let guildId) = place else {
            channels = nil
            messages = nil
            return
        }
        let list = ChannelListModel(guildId: guildId, store: store)
        channels = list
        openListedChannel(in: list, guildId: guildId)
    }

    public func open(channel id: ChannelId) {
        if case .guild(let guildId) = place {
            lastChannels[guildId] = id
        }
        guard messages?.channelId != id else {
            return
        }
        messages = MessageListModel(channelId: id, account: account, store: store)
    }

    /// Disconnects but keeps the session, e.g. before the Mac sleeps.
    public func suspend() {
        account.disconnect()
    }

    public func resume() {
        do {
            try account.connect()
        } catch {
            closed(error as? GatewayError)
        }
    }

    /// Ends the session. Releasing the model without calling it ends the event loop too.
    public func close() {
        ended = true
        account.close()
        subscription?.close()
        loop.cancel()
        connection = .closed(error: nil)
    }

    func apply(_ batch: EventBatch) {
        if let state = batch.connection {
            if case .closed(let error) = state {
                closed(error)
            } else {
                connection = state
            }
        }
        if batch.ready || batch.currentUserChanged {
            currentUser = store.currentUser()
        }
        if case .guild(let id) = place, batch.ready || batch.guildListChanged,
            store.guild(id: id) == nil
        {
            open(.home)
        }
        let open = messages?.channelId
        let wasListed = open.map(isListed) ?? false
        guilds.apply(batch)
        channels?.apply(batch)
        // Threads and DMs aren't in the channel list; they leave only when removed.
        if let open, batch.removedChannels.contains(open) || (wasListed && !isListed(open)) {
            if case .guild(let guildId) = place, let list = channels {
                openListedChannel(in: list, guildId: guildId)
            } else {
                messages = nil
            }
        }
        messages?.apply(batch)
    }

    private func isListed(_ channel: ChannelId) -> Bool {
        channels?.channels.contains { $0.id == channel } == true
    }

    private func openListedChannel(in list: ChannelListModel, guildId: GuildId) {
        let last = lastChannels[guildId].flatMap { last in list.channels.first { $0.id == last } }
        // Forum and media channels hold posts, not a message list.
        let first = list.channels.first { $0.kind == .guildText || $0.kind == .guildNews }
        if let channel = last ?? first {
            open(channel: channel.id)
        } else {
            messages = nil
        }
    }

    private func closed(_ error: GatewayError?) {
        connection = .closed(error: error)
        guard !ended else {
            return
        }
        ended = true
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
