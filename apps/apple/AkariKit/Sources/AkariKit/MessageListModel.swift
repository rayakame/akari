import Observation

/// A channel's messages, as the store's window holds them.
@MainActor @Observable
public final class MessageListModel {
    public enum Load: Equatable, Sendable {
        case latest
        case older
        case newer
    }

    public struct LoadFailure: Equatable, Sendable {
        public let load: Load
        public let error: RequestError

        public init(load: Load, error: RequestError) {
            self.load = load
            self.error = error
        }
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
    /// The last load's failure, until a load works.
    public private(set) var loadFailure: LoadFailure?
    /// The channel's composer; created with the model.
    public let composer: ComposerModel

    private static let pageSize: UInt8 = 50

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private var hasMessages = false
    // Confirmed message ID → the pending ID its row keeps as key.
    @ObservationIgnored private var pendingKeys: [MessageId: MessageId] = [:]
    @ObservationIgnored private var loads = 0
    @ObservationIgnored private var activeLoads: [Int: Load] = [:]

    init(channelId: ChannelId, account: Account, store: Store, drafts: Drafts = Drafts()) {
        self.channelId = channelId
        self.account = account
        self.store = store
        composer = ComposerModel(
            channelId: channelId, account: account, store: store, drafts: drafts)
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

    func apply(_ batch: EventBatch) {
        pendingKeys.merge(batch.confirmed[channelId] ?? [:]) { _, confirmed in confirmed }
        let updated = batch.updatedMessages[channelId] ?? []
        if batch.windowsChanged.contains(channelId) {
            reload(rereading: updated)
        } else if !updated.isEmpty {
            reread(updated)
        }
        composer.apply(batch)
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
        do {
            try await account.loadMessages(channelId: channelId, load: request)
            LaunchLog.mark("first message load finished")
            loadFailure = nil
            // The batch with the loaded range may come later; the rows shouldn't lag `loading`.
            reload(rereading: [])
        } catch {
            loadFailure = LoadFailure(
                load: kind, error: error as? RequestError ?? .UnexpectedResponse)
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
