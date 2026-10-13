import Observation

/// One spoiler of one message, revealed by a click.
public struct SpoilerKey: Hashable, Sendable {
    public let messageId: MessageId
    public let index: UInt16

    public init(messageId: MessageId, index: UInt16) {
        self.messageId = messageId
        self.index = index
    }
}

/// A channel's messages, as the store's window holds them.
@MainActor @Observable
public final class MessageListModel {
    public enum Load: Hashable, Sendable {
        case latest
        case older
        case newer
    }

    /// Where a channel's first rows came from.
    public enum Origin: Sendable {
        /// A window the store already held.
        case window
        case disk
        case network
    }

    public struct Row: Identifiable, Equatable, Sendable {
        /// Stable for the row's life: a sent message keeps its pending ID as its key.
        public let id: MessageId
        public let message: Message

        public init(id: MessageId, message: Message) {
            self.id = id
            self.message = message
        }
    }

    public let channelId: ChannelId
    /// Oldest first, then our pending and failed messages.
    public private(set) var rows: [Row] = []
    public private(set) var reachedOldest = false
    public private(set) var atPresent = true
    public private(set) var isStale = false
    public private(set) var loading: Load?
    /// Each kind of load's last failure, until that kind works; a latest page clears them all.
    public private(set) var failedLoads: [Load: RequestError] = [:]
    /// The channel's composer; created with the model.
    public let composer: ComposerModel
    /// The last finished load; changes with the rows it brought.
    public private(set) var lastLoad: LoadTrace?
    /// When the session opened the channel, and where its first rows came from.
    public let openedAt = ContinuousClock.now
    public private(set) var openedFrom = Origin.network
    /// Revealed for as long as the model lives: through rereads and loads, not a channel switch.
    public private(set) var revealedSpoilers: Set<SpoilerKey> = []

    private static let pageSize: UInt8 = 50

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    // Mentions in this channel are named from its guild.
    @ObservationIgnored private let guildId: GuildId?
    @ObservationIgnored private var hasMessages = false
    // Rows the cache showed while the first page loaded; that page didn't bring them.
    @ObservationIgnored private var fromDisk: Set<MessageId> = []
    // Confirmed message ID → the pending ID its row keeps as key.
    @ObservationIgnored private var pendingKeys: [MessageId: MessageId] = [:]
    @ObservationIgnored private var loads = 0
    @ObservationIgnored private var activeLoads: [Int: Load] = [:]

    init(channelId: ChannelId, account: Account, store: Store, drafts: Drafts = Drafts()) {
        self.channelId = channelId
        self.account = account
        self.store = store
        guildId = store.channel(id: channelId)?.guildId
        composer = ComposerModel(
            channelId: channelId, account: account, store: store, drafts: drafts)
        reload(rereading: [])
        if hasMessages {
            openedFrom = .window
        }
    }

    /// Views the channel; loads the latest page when it has no messages or is stale.
    public func open() async {
        if hasMessages && !isStale {
            account.viewChannel(channelId: channelId)
        } else {
            await load(.latest)
        }
    }

    public func loadOlder() async {
        guard !reachedOldest, loading == nil else {
            return
        }
        await load(.older)
    }

    public func loadNewer() async {
        guard !atPresent, loading == nil else {
            return
        }
        await load(.newer)
    }

    public func jumpToPresent() async {
        await load(.latest)
    }

    public func reveal(_ key: SpoilerKey) {
        revealedSpoilers.insert(key)
    }

    func apply(_ batch: EventBatch) {
        pendingKeys.merge(batch.confirmed[channelId] ?? [:]) { _, confirmed in confirmed }
        let updated = batch.updatedMessages[channelId] ?? []
        let hadRows = hasMessages
        if batch.windowsChanged.contains(channelId) {
            reload(rereading: namesChanged(batch) ? Set(rows.map(\.message.id)) : updated)
        } else if namesChanged(batch) {
            reread(Set(rows.map(\.message.id)))
        } else if !updated.isEmpty {
            reread(updated)
        }
        // The cache's copy comes stale, before the page that refreshes it.
        if !hadRows, hasMessages, isStale, openedFrom == .network, lastLoad == nil {
            openedFrom = .disk
            fromDisk = Set(rows.map(\.message.id))
        }
        composer.apply(batch)
    }

    // Mention names and the mentioned highlight come from these, not from the message.
    private func namesChanged(_ batch: EventBatch) -> Bool {
        if batch.currentUserChanged || !batch.usersChanged.isEmpty {
            return true
        }
        guard let guildId else {
            return false
        }
        return batch.guildsChanged.contains(guildId) || batch.membersChanged.contains(guildId)
            || batch.channelsChanged[guildId] != nil
    }

    private func load(_ kind: Load) async {
        loads += 1
        let ticket = loads
        activeLoads[ticket] = kind
        loading = kind
        defer {
            activeLoads[ticket] = nil
            // Loads can end out of order; show the newest one still running.
            loading = activeLoads.max { $0.key < $1.key }?.value
        }
        let request: MessageLoad =
            switch kind {
            case .latest: .latest(limit: Self.pageSize)
            case .older: .older(limit: Self.pageSize)
            case .newer: .newer(limit: Self.pageSize)
            }
        LaunchLog.mark("first message load started")
        let started = ContinuousClock.now
        let held = Set(rows.map(\.id))
        let oldest = rows.first?.id
        do {
            let report = try await account.loadMessages(channelId: channelId, load: request)
            LaunchLog.mark("first message load finished")
            if kind == .latest {
                failedLoads = [:]
            } else {
                failedLoads[kind] = nil
            }
            let reading = ContinuousClock.now
            // The batch with the loaded range may come later; the rows shouldn't lag `loading`.
            reload(rereading: [])
            let read = reading.duration(to: .now)
            // A live message that arrived meanwhile is new too, but below an older page.
            let page = rows.map(\.id).filter { id in
                !held.contains(id) && !fromDisk.contains(id)
                    && (kind != .older || oldest.map { id < $0 } ?? true)
            }
            lastLoad = LoadTrace(
                sequence: ticket, channelId: channelId, kind: kind, report: report,
                failure: nil, page: Set(page), read: read, started: started,
                duration: started.duration(to: .now))
        } catch {
            let failure = error as? RequestError ?? .UnexpectedResponse
            failedLoads[kind] = failure
            lastLoad = LoadTrace(
                sequence: ticket, channelId: channelId, kind: kind, report: nil,
                failure: failure, read: .zero, started: started,
                duration: started.duration(to: .now))
        }
    }

    private func reload(rereading changed: Set<MessageId>) {
        guard let window = store.window(channelId: channelId) else {
            rows = []
            pendingKeys = [:]
            hasMessages = false
            reachedOldest = false
            atPresent = true
            isStale = false
            return
        }
        let ids = window.messageIds + window.pendingIds
        var held = Dictionary(rows.map { ($0.message.id, $0.message) }) { first, _ in first }
        let unread = ids.filter { held[$0] == nil || changed.contains($0) }
        if !unread.isEmpty {
            for message in store.messages(channelId: channelId, ids: unread) {
                held[message.id] = message
            }
        }
        let shown = Set(ids)
        pendingKeys = pendingKeys.filter { shown.contains($0.key) }
        rows = ids.compactMap { id in
            held[id].map { Row(id: pendingKeys[id] ?? id, message: $0) }
        }
        hasMessages = !window.messageIds.isEmpty
        reachedOldest = window.oldest
        atPresent = window.latest
        isStale = window.stale
    }

    private func reread(_ changed: Set<MessageId>) {
        let ids = rows.map(\.message.id).filter(changed.contains)
        guard !ids.isEmpty else {
            return
        }
        let fresh = Dictionary(
            store.messages(channelId: channelId, ids: ids).map { ($0.id, $0) }
        ) { first, _ in first }
        rows = rows.map { row in
            fresh[row.message.id].map { Row(id: row.id, message: $0) } ?? row
        }
    }
}
