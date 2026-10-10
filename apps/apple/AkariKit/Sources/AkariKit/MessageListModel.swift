import Observation

/// A channel's messages, as the store's window holds them.
@MainActor @Observable
public final class MessageListModel {
    public enum Load: Equatable, Sendable {
        case latest
        case older
        case newer
    }

    public struct Row: Identifiable, Equatable, Sendable {
        /// Stable for the row's life: a sent message keeps its pending ID as its key.
        public let id: MessageId
        public let message: Message
    }

    public let channelId: ChannelId
    /// Oldest first, then our pending and failed messages.
    public private(set) var rows: [Row] = []
    public private(set) var reachedOldest = false
    public private(set) var atPresent = true
    public private(set) var isStale = false
    public private(set) var canSend = false
    public private(set) var loading: Load?
    public private(set) var loadError: RequestError?
    public private(set) var sendError: RequestError?

    private static let pageSize: UInt8 = 50

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private var guildId: GuildId?
    @ObservationIgnored private var hasMessages = false
    // Confirmed message ID → the pending ID its row keeps as key.
    @ObservationIgnored private var pendingKeys: [MessageId: MessageId] = [:]
    @ObservationIgnored private var loads = 0

    init(channelId: ChannelId, account: Account, store: Store) {
        self.channelId = channelId
        self.account = account
        self.store = store
        readCanSend()
        reload(rereading: [])
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

    /// The message shows as pending at once; a failure leaves it as failed.
    public func send(_ content: String) async {
        do {
            _ = try await account.sendMessage(channelId: channelId, content: content)
            sendError = nil
        } catch {
            sendError = error as? RequestError
        }
    }

    public func retry(_ id: MessageId) async {
        do {
            _ = try await account.retryMessage(channelId: channelId, pendingId: id)
            sendError = nil
        } catch {
            sendError = error as? RequestError
        }
    }

    public func discard(_ id: MessageId) {
        account.discardMessage(channelId: channelId, pendingId: id)
    }

    func apply(_ batch: EventBatch) {
        let guildChanged = guildId.map {
            batch.guildsChanged.contains($0) || batch.membersChanged.contains($0)
        }
        if batch.ready || guildChanged == true
            || batch.channelsChanged[guildId]?.contains(channelId) == true
        {
            readCanSend()
        }
        pendingKeys.merge(batch.confirmed[channelId] ?? [:]) { _, confirmed in confirmed }
        let updated = batch.updatedMessages[channelId] ?? []
        if batch.windowsChanged.contains(channelId) {
            reload(rereading: updated)
        } else if !updated.isEmpty {
            reread(updated)
        }
    }

    private func load(_ kind: Load) async {
        loads += 1
        let ticket = loads
        loading = kind
        defer {
            if loads == ticket {
                loading = nil
            }
        }
        let request: MessageLoad =
            switch kind {
            case .latest: .latest(limit: Self.pageSize)
            case .older: .older(limit: Self.pageSize)
            case .newer: .newer(limit: Self.pageSize)
            }
        do {
            try await account.loadMessages(channelId: channelId, load: request)
            loadError = nil
            // The batch with the loaded range may come later; the rows shouldn't lag `loading`.
            reload(rereading: [])
        } catch {
            loadError = error as? RequestError
        }
    }

    private func readCanSend() {
        guard let channel = store.channel(id: channelId) else {
            canSend = false
            return
        }
        guildId = channel.guildId
        // DMs have no permissions to check.
        canSend =
            channel.guildId == nil
            || store.permissions(channelId: channelId)?.contains(.sendMessages) == true
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
