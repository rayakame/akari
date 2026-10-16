import Foundation
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
        /// Around the read position, for the jump to the first unread message.
        case around
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

    /// Over the list's top while the new messages line hasn't been on screen.
    public struct UnreadBar: Equatable, Sendable {
        /// The loaded messages from others from the line on.
        public let count: Int
        /// More are unread than are loaded: the read position is older than the loaded messages,
        /// or the list isn't at the present.
        public let more: Bool
        /// The first unread message's time, or the read position's when that isn't loaded.
        public let since: Date

        /// "3 new messages since 17:13", with `time` as the app shows message times.
        public func text(time: String) -> String {
            let number = count.formatted() + (more ? "+" : "")
            let messages = count == 1 && !more ? "message" : "messages"
            return "\(number) new \(messages) since \(time)"
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
    /// Whether the view is scrolled to the bottom of its rows, as it reports; a list opens there.
    public private(set) var atBottom = true
    /// At the present, not stale, and scrolled to its bottom: where the user reads new messages.
    public var atPresentBottom: Bool { atPresent && atBottom && !isStale }
    /// The row the new messages line goes before: the first loaded message from someone else
    /// that is newer than what the user had read when the list opened. Acks, scrolling and
    /// reconnects keep it; a send, Escape and "Mark As Read" end it, and only a later message
    /// starts a new one.
    public private(set) var newMessagesLine: MessageId?
    /// Set once the view says the new messages line isn't on screen. It goes for good once the
    /// line has been on screen or jumped to, until a line for a later read position.
    public private(set) var unreadBar: UnreadBar?
    /// Counts the jumps to the first unread message; on each the view scrolls the line to its top.
    public private(set) var unreadJumps = 0

    private static let pageSize: UInt8 = 50

    @ObservationIgnored private let account: Account
    @ObservationIgnored private let store: Store
    @ObservationIgnored private let userId: UserId
    // Taken when the list opens: the core acks the open channel at once, which would leave
    // nothing new to mark.
    @ObservationIgnored private var readUpTo: MessageId?
    // Whether `readUpTo` came from the store or a mark as read, not from the first rows shown.
    @ObservationIgnored private var readUpToKnown = false
    // The read positions whose line the view said was out of sight, and on screen; the line of a
    // later position starts unplaced.
    @ObservationIgnored private var hiddenLineFrom: MessageId?
    @ObservationIgnored private var seenLineFrom: MessageId?
    // Set once the session has had the core read the channel: a READY then brings our own ack.
    @ObservationIgnored var readHere = false
    // Mentions in this channel are named from its guild.
    @ObservationIgnored private let guildId: GuildId?
    @ObservationIgnored private var hasMessages = false
    // Rows the cache showed while the first page loaded; that page didn't bring them.
    @ObservationIgnored private var fromDisk: Set<MessageId> = []
    // Confirmed message ID → the pending ID its row keeps as key.
    @ObservationIgnored private var pendingKeys: [MessageId: MessageId] = [:]
    @ObservationIgnored private var loads = 0
    @ObservationIgnored private var activeLoads: [Int: Load] = [:]
    // For the session's reading signal; `true` when the user scrolled away from the bottom.
    @ObservationIgnored var bottomChanged: ((_ scrolledAway: Bool) -> Void)?

    init(
        channelId: ChannelId, userId: UserId, account: Account, store: Store,
        drafts: Drafts = Drafts()
    ) {
        self.channelId = channelId
        self.userId = userId
        self.account = account
        self.store = store
        guildId = store.channel(id: channelId)?.guildId
        composer = ComposerModel(
            channelId: channelId, account: account, store: store, drafts: drafts)
        readStoredPosition()
        reload(rereading: [])
        if hasMessages {
            openedFrom = .window
        }
        composer.onSend = { [weak self] in
            self?.endNewMessagesLine()
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

    /// Escape: marks the channel read, ends the new messages line, and loads the present when
    /// the list shows older messages. Scrolling to the bottom is the view's part.
    public func escape() async {
        account.markRead(channelId: channelId)
        endNewMessagesLine()
        if !atPresent {
            await load(.latest)
        }
    }

    /// The unread bar and Shift+Page Up: loads the messages around the read position when the
    /// first unread message isn't loaded. Scrolling to the line is the view's part. Does nothing
    /// without a line.
    public func jumpToUnread() async {
        guard newMessagesLine != nil, let readUpTo else {
            return
        }
        seenLineFrom = readUpTo
        updateUnreadBar()
        if !readPositionLoaded(readUpTo) {
            await load(.around)
        }
        unreadJumps += 1
    }

    /// The view reports whether the new messages line is on screen.
    public func setNewMessagesLineOnScreen(_ onScreen: Bool) {
        guard newMessagesLine != nil, let readUpTo,
            readUpTo != (onScreen ? seenLineFrom : hiddenLineFrom)
        else {
            return
        }
        if onScreen {
            seenLineFrom = readUpTo
        } else {
            hiddenLineFrom = readUpTo
        }
        updateUnreadBar()
    }

    /// The view reports whether it's scrolled to the bottom.
    public func setAtBottom(_ atBottom: Bool) {
        guard atBottom != self.atBottom else {
            return
        }
        self.atBottom = atBottom
        bottomChanged?(!atBottom)
    }

    public func reveal(_ key: SpoilerKey) {
        revealedSpoilers.insert(key)
    }

    // Marked read or a send: only a message newer than what is known now starts the next line.
    func endNewMessagesLine() {
        let newest = rows.last { $0.message.delivery == .sent }?.message.id
        // Marking read acks up to the channel's newest message, also one not loaded here.
        let acked = store.readState(channelId: channelId)?.acked
        if let position = [readUpTo, newest, acked].compactMap({ $0 }).max() {
            readUpTo = position
            readUpToKnown = true
        }
        placeNewMessagesLine()
    }

    // READY's position is Discord's, newer when the user read on another device meanwhile.
    func newSession() {
        guard !readHere, let state = store.readState(channelId: channelId) else {
            return
        }
        let position = state.acked ?? MessageId(rawValue: 0)
        if let readUpTo, readUpTo >= position {
            return
        }
        readUpTo = position
        readUpToKnown = true
        placeNewMessagesLine()
    }

    func apply(_ batch: EventBatch) {
        // A cache restored after the list opened still has the read position from before this
        // session's acks, which only start with READY.
        if batch.restored, !readUpToKnown {
            readStoredPosition()
            placeNewMessagesLine()
        }
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
            case .around:
                .around(messageId: readUpTo ?? MessageId(rawValue: 0), limit: Self.pageSize)
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
        let wasAtPresentBottom = atPresentBottom
        defer {
            if atPresentBottom != wasAtPresentBottom {
                bottomChanged?(false)
            }
        }
        guard let window = store.window(channelId: channelId) else {
            rows = []
            pendingKeys = [:]
            hasMessages = false
            reachedOldest = false
            atPresent = true
            isStale = false
            placeNewMessagesLine()
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
        placeNewMessagesLine()
    }

    // Without a read state the channel counts as read; `acked` is nil while nothing is.
    private func readStoredPosition() {
        guard let state = store.readState(channelId: channelId) else {
            return
        }
        readUpTo = state.acked ?? MessageId(rawValue: 0)
        readUpToKnown = true
    }

    private func placeNewMessagesLine() {
        if readUpTo == nil {
            readUpTo = rows.last { $0.message.delivery == .sent }?.message.id
        }
        let line = readUpTo.flatMap { readUpTo in
            rows.first { row in
                row.message.id > readUpTo && row.message.author.id != userId
                    && row.message.delivery == .sent
            }?.id
        }
        if line != newMessagesLine {
            newMessagesLine = line
        }
        updateUnreadBar()
    }

    private func updateUnreadBar() {
        let bar = currentUnreadBar()
        if bar != unreadBar {
            unreadBar = bar
        }
    }

    private func currentUnreadBar() -> UnreadBar? {
        guard let line = newMessagesLine, let readUpTo, readUpTo == hiddenLineFrom,
            readUpTo != seenLineFrom, let start = rows.firstIndex(where: { $0.id == line })
        else {
            return nil
        }
        let count = rows[start...].count { row in
            row.message.author.id != userId && row.message.delivery == .sent
        }
        let loaded = readPositionLoaded(readUpTo)
        // Nothing read yet has no time of its own.
        let since =
            loaded || readUpTo.rawValue == 0
            ? rows[start].message.timestamp : Self.time(of: readUpTo)
        return UnreadBar(count: count, more: !loaded || !atPresent, since: since)
    }

    // Then the line stands right before the first unread message.
    private func readPositionLoaded(_ position: MessageId) -> Bool {
        guard !reachedOldest else {
            return true
        }
        let oldest = rows.first { $0.message.delivery == .sent }?.message.id
        return oldest.map { $0 <= position } ?? false
    }

    // Discord's IDs carry their time: milliseconds since 2015, above the low 22 bits.
    private static func time(of id: MessageId) -> Date {
        Date(timeIntervalSince1970: Double((id.rawValue >> 22) + 1_420_070_400_000) / 1000)
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
