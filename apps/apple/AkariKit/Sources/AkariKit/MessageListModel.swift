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
        /// Around a message, for a jump to it.
        case aroundMessage
    }

    /// Where a channel's first rows came from.
    public enum Origin: Sendable {
        /// A window the store already held.
        case window
        case disk
        case network
    }

    public struct Row: Identifiable, Equatable, Sendable {
        /// A thread's first message when it started from a message in its parent channel.
        public enum Starter: Equatable, Sendable {
            /// Being fetched; the row isn't shown yet.
            case loading
            /// The parent channel's message the thread started from.
            case loaded(Message)
            /// Deleted, or the user can't read it.
            case unavailable
        }

        public static let unavailableStarter =
            "Sorry, we couldn't load the first message in this thread"

        /// Stable for the row's life: a sent message keeps its pending ID as its key.
        public let id: MessageId
        /// As the store holds it.
        public let message: Message
        /// Set for a thread's first message that points at its parent's.
        public let starter: Starter?
        /// Whether a "started a thread" message links its thread: not once Discord refused it or
        /// it's gone.
        public let threadLinked: Bool
        /// What a system message's line is worded for.
        public let noticeContext: Message.NoticeContext

        public init(
            id: MessageId, message: Message, starter: Starter? = nil, threadLinked: Bool = true,
            noticeContext: Message.NoticeContext = .init()
        ) {
            self.id = id
            self.message = message
            self.starter = starter
            self.threadLinked = threadLinked
            self.noticeContext = noticeContext
        }

        /// What the row draws: a loaded starter draws the message the thread started from.
        public var shown: Message {
            if case .loaded(let original) = starter { original } else { message }
        }

        /// What the row says when it draws as a system message.
        public var notice: String? {
            starter == .unavailable
                ? Self.unavailableStarter : shown.notice(in: noticeContext)
        }

        /// `notice` in parts; an unavailable thread isn't linked.
        public var noticeParts: [Message.NoticePart]? {
            if starter == .unavailable {
                return [.text(Self.unavailableStarter)]
            }
            return shown.noticeParts(in: noticeContext)?.map { part in
                if case .thread(let name, _) = part, !threadLinked {
                    return .thread(name: name, id: nil)
                }
                return part
            }
        }

        /// A system message Discord shows no line for; the timeline leaves it out.
        public var isHidden: Bool {
            noticeParts?.isEmpty == true
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

    /// A jump to a message, such as a pin's: the view scrolls it to its top, or the first
    /// message after it when it's gone.
    public struct MessageJump: Equatable, Sendable {
        public let messageId: MessageId
        /// Counts the jumps, so one to the same message again scrolls again.
        public let number: Int

        public init(messageId: MessageId, number: Int) {
            self.messageId = messageId
            self.number = number
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
    /// The row the new messages line goes before: the first loaded message from someone else past
    /// the read position the list opened with, or the one a send, a mark as read or READY gave it.
    public private(set) var newMessagesLine: MessageId?
    /// Set once the view says the new messages line isn't on screen. It goes for good once the
    /// line has been on screen or jumped to, until a line for a later read position.
    public private(set) var unreadBar: UnreadBar?
    /// Counts the jumps to the first unread message; on each the view scrolls the line to its top.
    public private(set) var unreadJumps = 0
    /// The last jump to a message that landed.
    public private(set) var messageJump: MessageJump?
    /// Who started the thread, by name, if the store or the loaded messages know them.
    public private(set) var threadOwner: String?

    private static let pageSize: UInt8 = 50
    // A trackpad's bounce or momentum at the bottom stays below it.
    private static let scrollAwayDistance = 48.0

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
    @ObservationIgnored private let place: Message.NoticeContext.Place
    @ObservationIgnored private let ownerId: UserId?
    @ObservationIgnored private var storedOwner: String?
    @ObservationIgnored private var hasMessages = false
    // Rows the cache showed while the first page loaded; that page didn't bring them.
    @ObservationIgnored private var fromDisk: Set<MessageId> = []
    // Confirmed message ID → the pending ID its row keeps as key.
    @ObservationIgnored private var pendingKeys: [MessageId: MessageId] = [:]
    @ObservationIgnored private var loads = 0
    @ObservationIgnored private var activeLoads: [Int: Load] = [:]
    // For the session's reading signal; `true` when the user scrolled clearly away from the bottom.
    @ObservationIgnored var bottomChanged: ((_ scrolledAway: Bool) -> Void)?
    // The user's own scrolls since the bottom, not the distance to it: a message arriving during
    // a gesture moves the bottom away.
    @ObservationIgnored private var scrolledUp = 0.0
    // The load ticket of the last latest or around load or supersede: only it places the list.
    @ObservationIgnored private var lastPlacing = 0
    // A latest load a jump overtook, until a later page lands: should the jump not land, the
    // list still has to reach the present.
    @ObservationIgnored private var owedPresent: Int?
    @ObservationIgnored private var retired = false
    @ObservationIgnored private var landed = 0
    @ObservationIgnored private var messageJumps = 0
    // The message the last jump that loaded goes to.
    @ObservationIgnored private var jumpingTo: MessageId?

    init(
        channelId: ChannelId, userId: UserId, account: Account, store: Store,
        drafts: Drafts = Drafts()
    ) {
        self.channelId = channelId
        self.userId = userId
        self.account = account
        self.store = store
        let channel = store.channel(id: channelId)
        guildId = channel?.guildId
        place = Self.place(of: channel, in: store)
        ownerId = channel?.thread?.ownerId
        composer = ComposerModel(
            channelId: channelId, account: account, store: store, drafts: drafts)
        readStoredPosition()
        readOwner()
        reload(rereading: [])
        if hasMessages {
            openedFrom = .window
        }
        composer.onSend = { [weak self] in
            self?.endNewMessagesLine()
        }
    }

    /// Views the channel; loads the latest page when it has no messages or is stale. A jump to
    /// unread an earlier list of the channel started doesn't land.
    public func open() async {
        if hasMessages && !isStale {
            account.supersedeAroundLoads(channelId: channelId)
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
        // The core may hold a jump's page the list hasn't shown yet.
        if !atPresent || store.window(channelId: channelId)?.latest == false {
            await load(.latest)
        }
    }

    /// The unread bar and Shift+Page Up, when there is a line: loads the messages around the read
    /// position if the first unread message isn't loaded; a failed load leaves the bar. Escape,
    /// the jump to the present, marking read, sending and a channel switch meanwhile cancel the
    /// jump. Scrolling to the line is the view's part.
    public func jumpToUnread() async {
        guard newMessagesLine != nil, let readUpTo else {
            return
        }
        // A channel never read has no position to load around; its line is among the loaded rows.
        // A jump to a message still loading would land after the scroll.
        if readUpTo.rawValue != 0,
            !readPositionLoaded(readUpTo) || activeLoads[lastPlacing] == .aroundMessage
        {
            // The running load jumps once its messages are in, if nothing placed the list since.
            guard activeLoads[lastPlacing] != .around else {
                return
            }
            // A failed load, or a page the core dropped for a newer request, leaves the bar.
            let (ticket, outcome) = await load(.around)
            guard outcome == .applied, ticket == lastPlacing else {
                // Unless a load placed the list since, nothing else takes it to the present.
                if !retired, owedPresent != nil, activeLoads[lastPlacing] == nil {
                    await load(.latest)
                }
                return
            }
        } else if activeLoads[lastPlacing] == .aroundMessage {
            supersedeJumps()
        }
        seenLineFrom = self.readUpTo
        updateUnreadBar()
        unreadJumps += 1
    }

    /// "a message" in a pin's line: loads the messages around it unless the loaded ones reach
    /// it and nothing else is placing the list; a failed load leaves the list where it is. What
    /// cancels the jump to unread cancels this one too, and so does either jump started after
    /// it. Scrolling is the view's part.
    public func jump(to messageId: MessageId) async {
        let placing = activeLoads[lastPlacing]
        // The running load jumps once its messages are in.
        if placing == .aroundMessage, jumpingTo == messageId {
            return
        }
        // A load still placing the list would replace the rows the jump scrolls.
        if placing != nil || !loadedRange(reaches: messageId) {
            jumpingTo = messageId
            let (ticket, outcome) = await load(.aroundMessage, around: messageId)
            guard outcome == .applied, ticket == lastPlacing else {
                if !retired, owedPresent != nil, activeLoads[lastPlacing] == nil {
                    await load(.latest)
                }
                return
            }
        }
        messageJumps += 1
        messageJump = MessageJump(messageId: messageId, number: messageJumps)
    }

    // A jump landing after the user left would reopen the channel in history, without its scroll.
    func retire() {
        retired = true
        if aroundLoadRunning {
            account.supersedeAroundLoads(channelId: channelId)
        }
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

    /// The view reports whether it's scrolled to the bottom, also after its own scrolls. Away
    /// from it reading pauses, and the user stays engaged.
    public func setAtBottom(_ atBottom: Bool) {
        guard atBottom != self.atBottom else {
            return
        }
        self.atBottom = atBottom
        if atBottom {
            scrolledUp = 0
        }
        bottomChanged?(false)
    }

    /// A scroll by the user, positive toward older messages, in a list `height` points high;
    /// after `setAtBottom` for the same move. One clearly away from the bottom ends engagement.
    public func userScrolled(by distance: Double, height: Double) {
        guard !atBottom else {
            return
        }
        let limit = max(Self.scrollAwayDistance, height / 4)
        let wasAway = scrolledUp > limit
        scrolledUp = max(0, scrolledUp + distance)
        if !wasAway, scrolledUp > limit {
            bottomChanged?(true)
        }
    }

    public func reveal(_ key: SpoilerKey) {
        revealedSpoilers.insert(key)
    }

    // Marked read or a send: only a message newer than what is known now starts the next line.
    func endNewMessagesLine() {
        // A latest load would overtake a running jump too, but at the present none runs.
        if aroundLoadRunning {
            supersedeJumps()
        }
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
        if batch.everythingChanged || batch.currentUserChanged
            || ownerId.map(batch.usersChanged.contains) == true
        {
            readOwner()
        }
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

    // Drops the pages of the jumps still loading; what comes after places the list.
    private func supersedeJumps() {
        account.supersedeAroundLoads(channelId: channelId)
        loads += 1
        lastPlacing = loads
    }

    private var aroundLoadRunning: Bool {
        activeLoads.values.contains { $0 == .around || $0 == .aroundMessage }
    }

    // The outcome is `nil` when it failed or there was nothing to load. `target` is the message
    // an `aroundMessage` load centers on.
    @discardableResult
    private func load(_ kind: Load, around target: MessageId? = nil) async -> (
        ticket: Int, outcome: PageOutcome?
    ) {
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
            case .aroundMessage:
                .around(messageId: target ?? MessageId(rawValue: 0), limit: Self.pageSize)
            }
        // Taken now, in the order the user acted: the load begins later, on Akari's runtime.
        var placing: UInt64?
        if kind.places {
            placing = account.reservePlacing()
            lastPlacing = ticket
        }
        LaunchLog.mark("first message load started")
        let started = ContinuousClock.now
        let held = Set(rows.map(\.id))
        let oldest = rows.first?.id
        do {
            let report = try await account.loadMessages(
                channelId: channelId, load: request, placing: placing)
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
            if kind.places {
                notePlaced(ticket, kind: kind, outcome: report?.outcome)
            }
            return (ticket, report?.outcome)
        } catch {
            let failure = error as? RequestError ?? .UnexpectedResponse
            failedLoads[kind] = failure
            lastLoad = LoadTrace(
                sequence: ticket, channelId: channelId, kind: kind, report: nil,
                failure: failure, read: .zero, started: started,
                duration: started.duration(to: .now))
            return (ticket, nil)
        }
    }

    private func notePlaced(_ ticket: Int, kind: Load, outcome: PageOutcome?) {
        switch outcome {
        case .applied?:
            landed = max(landed, ticket)
            if let owed = owedPresent, owed < ticket {
                owedPresent = nil
            }
        case .overtaken? where kind == .latest && ticket > landed:
            owedPresent = ticket
        default:
            break
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
        var held = Dictionary(rows.map { ($0.message.id, $0) }) { first, _ in first }
        let unread = ids.filter { held[$0] == nil || changed.contains($0) }
        if !unread.isEmpty {
            for message in store.messages(channelId: channelId, ids: unread) {
                held[message.id] = row(message.id, message)
            }
        }
        let shown = Set(ids)
        pendingKeys = pendingKeys.filter { shown.contains($0.key) }
        rows = ids.compactMap { id in
            held[id].map { row(pendingKeys[id] ?? id, $0.message, from: $0) }
        }
        nameOwner()
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

    // So it's among them, or was deleted from between them.
    private func loadedRange(reaches id: MessageId) -> Bool {
        let sent = { (row: Row) in row.message.delivery == .sent }
        guard let oldest = rows.first(where: sent)?.message.id,
            let newest = rows.last(where: sent)?.message.id
        else {
            return false
        }
        return (reachedOldest || oldest <= id) && (atPresent || id <= newest)
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
            fresh[row.message.id].map { self.row(row.id, $0) } ?? row
        }
        nameOwner()
    }

    private func row(_ id: MessageId, _ message: Message) -> Row {
        let thread = message.kind == .threadCreated ? message.reference?.channelId : nil
        let starter = starter(of: message)
        // A loaded starter draws the original, which is worded for its own kind.
        let shown = if case .loaded(let original)? = starter { original } else { message }
        return Row(
            id: id, message: message, starter: starter,
            threadLinked: thread.map {
                store.channel(id: $0) != nil || !store.threadUnavailable(threadId: $0)
            } ?? true,
            noticeContext: noticeContext(for: shown))
    }

    private func row(_ id: MessageId, _ message: Message, from held: Row) -> Row {
        Row(
            id: id, message: message, starter: held.starter, threadLinked: held.threadLinked,
            noticeContext: held.noticeContext)
    }

    // Reads only what the kind's line needs: most rows are people's messages.
    private func noticeContext(for message: Message) -> Message.NoticeContext {
        var context = Message.NoticeContext(currentUserId: userId, place: place)
        context.guildId = guildId
        switch message.kind {
        case .premiumGuildSubscriptionTier1, .premiumGuildSubscriptionTier2,
            .premiumGuildSubscriptionTier3, .guildDiscoveryDisqualified,
            .guildDiscoveryGracePeriodInitialWarning, .guildDiscoveryGracePeriodFinalWarning,
            .roleSubscriptionPurchase, .guildIncidentReportRaid:
            context.guildName = guildId.flatMap { store.guild(id: $0)?.name }
        case .threadCreated:
            context.threadName = message.reference?.channelId.flatMap {
                store.channel(id: $0)?.name
            }
        case .autoModerationAction:
            let field = message.embedFields.first { $0.name == "channel_id" }
            if let alerted = field.flatMap({ UInt64($0.value) }).map(ChannelId.init(rawValue:)) {
                context.alertChannelName = store.channel(id: alerted)?.name
                // Discord links it only while the user can see it.
                if store.permissions(channelId: alerted)?.contains(.viewChannel) == true {
                    context.alertChannelId = alerted
                }
            }
        case .friendRequestAccepted:
            context.recipientName = store.channel(id: channelId)?.recipientIds.first.flatMap {
                store.user(id: $0)?.displayName
            }
        case .voiceSession:
            context.participantNames = (message.call?.participants ?? [])
                .filter { $0 != message.author.id }
                .compactMap { store.user(id: $0)?.displayName }
        default:
            break
        }
        return context
    }

    private func starter(of message: Message) -> Row.Starter? {
        guard message.kind == .threadStarterMessage else {
            return nil
        }
        return switch store.starterMessage(threadId: channelId) {
        case .loaded(let original)?: .loaded(original)
        case .loading?: .loading
        case .unavailable?, nil: .unavailable
        }
    }

    // The store's users are DM recipients; the user may not have posted in their own thread.
    private func readOwner() {
        storedOwner = ownerId.flatMap { owner in
            owner == userId
                ? store.currentUser()?.displayName : store.user(id: owner)?.displayName
        }
        nameOwner()
    }

    private func nameOwner() {
        guard let ownerId else {
            return
        }
        let owner =
            storedOwner
            ?? rows.lazy.flatMap { [$0.shown, $0.message] }
            .first { $0.author.id == ownerId }?.author.displayName
        if owner != threadOwner {
            threadOwner = owner
        }
    }
}

extension MessageListModel {
    fileprivate static func place(of channel: Channel?, in store: Store) -> Message.NoticeContext.Place
    {
        switch channel?.kind {
        case .dm?: return .dm
        case .groupDm?: return .groupDM
        case .publicThread?, .privateThread?, .newsThread?:
            let parent = channel?.parentId.flatMap { store.channel(id: $0)?.kind }
            return parent == .guildForum || parent == .guildMedia ? .forumPost : .thread
        default: return .channel
        }
    }
}

extension MessageListModel.Load {
    // Older and newer loads extend the list; the others put it where they load.
    fileprivate var places: Bool {
        self != .older && self != .newer
    }
}
