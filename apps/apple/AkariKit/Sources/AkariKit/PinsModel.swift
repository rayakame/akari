import Foundation
import Observation

/// A channel's pinned messages as the popout under the header's pins button shows them: the
/// newest pin first, 25 a page, kept current by the core.
@MainActor @Observable
public final class PinsModel {
    /// What the popout shows.
    public enum Phase: Equatable, Sendable {
        /// The first page is on its way.
        case loading
        /// *Akari:* the first page didn't come; `retry()` asks again. The official client shows
        /// a failed first page as an empty channel.
        case failed
        /// The channel has no pins.
        case empty
        case list
    }

    public static let title = "Pinned Messages"
    public static let jump = "Jump"
    public static let unpin = "Unpin Message"
    public static let loadMore = "Load more messages"
    public static let protipLabel = "Protip:"
    public static let failedText = "Akari couldn't load the pinned messages."
    public static let retry = "Try Again"

    public let channelId: ChannelId
    /// A DM or a group DM, whose words speak of a direct message.
    public let isPrivate: Bool
    /// The pinned messages, the newest pin first; each row's actions and reactions read by its
    /// IDs, as a list's do.
    public private(set) var rows: [MessageListModel.Row] = []
    public private(set) var phase = Phase.loading
    /// Discord has pins after `rows`.
    public private(set) var hasMore = false
    /// A later page is on its way: the spinner under the rows.
    public private(set) var loadingMore = false
    /// Messages whose unpin waits for Discord's answer: their ✕ is busy.
    public private(set) var unpinning: Set<MessageId> = []

    /// "Load more messages" under the rows: while Discord has more and nothing loads, also after a
    /// later page failed (*Akari:* the official client offers nothing then).
    public var offersMore: Bool {
        phase == .list && hasMore && !loadingMore
    }

    /// The empty state's words, with the official client's line break.
    public var emptyText: String {
        isPrivate
            ? "This direct message doesn't have\nany pinned messages... yet."
            : "This channel doesn't have any\npinned messages... yet."
    }

    /// The protip under the empty state, after `protipLabel`.
    public var protip: String {
        isPrivate
            ? "You can pin a message from its context menu."
            : "Users with the 'Pin Messages' permission can pin a message from its context menu."
    }

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let jumpTo: (MessageId, Bool) -> Void
    @ObservationIgnored private let ask: (MessageActionsModel.PinQuestion) -> Void
    @ObservationIgnored private let confirm: (MessageActionsModel.Confirmation) -> Void
    @ObservationIgnored private let failed: (MessageActionsModel.Failure) -> Void
    @ObservationIgnored private var state = PinsState.notLoaded
    @ObservationIgnored private var opened = false
    @ObservationIgnored private var asking = false

    /// `jump` jumps to a message of the channel and closes the popout unless Shift (its second
    /// argument) is held; `ask` asks before an unpin, `confirm` offers to try a failed one again
    /// and `failed` tells of a refusal.
    init(
        channelId: ChannelId, store: Store, account: Account,
        jump: @escaping (MessageId, Bool) -> Void,
        ask: @escaping (MessageActionsModel.PinQuestion) -> Void,
        confirm: @escaping (MessageActionsModel.Confirmation) -> Void,
        failed: @escaping (MessageActionsModel.Failure) -> Void
    ) {
        self.channelId = channelId
        self.store = store
        self.account = account
        jumpTo = jump
        self.ask = ask
        self.confirm = confirm
        self.failed = failed
        let kind = store.channel(id: channelId)?.kind
        isPrivate = kind == .dm || kind == .groupDm
        reread()
    }

    /// The popout shows: the core asks Discord for the first page when its list needs one, and
    /// the user has seen the channel's pins.
    public func open() async {
        opened = true
        account.ackPins(channelId: channelId)
        await load()
    }

    /// The list's end came near: the next page, while Discord has more and the last page came.
    public func reachedEnd() async {
        guard state == .loaded else {
            return
        }
        await loadMore()
    }

    /// "Load more messages".
    public func loadMore() async {
        guard hasMore, state != .loading else {
            return
        }
        state = .loading
        loadingMore = true
        try? await account.loadMorePins(channelId: channelId)
        reread()
    }

    /// The failed state's button.
    public func retry() async {
        await load()
    }

    /// The row's ✕: where the core allows the unpin.
    public func canUnpin(_ row: MessageListModel.Row) -> Bool {
        store.messageActions(channelId: row.message.channelId, messageId: row.message.id).unpin
    }

    /// "Jump": to the message in the channel's list; the popout closes unless Shift is held.
    public func jump(to row: MessageListModel.Row, shift: Bool) {
        jumpTo(row.message.id, shift)
    }

    /// The row's ✕: unpins after the official question, at once with Shift.
    public func unpin(_ row: MessageListModel.Row, shift: Bool) {
        let message = row.message.id
        guard canUnpin(row), !unpinning.contains(message) else {
            return
        }
        MessageActionsModel.pin(
            row, pins: false, shift: shift, store: store, account: account, ask: ask,
            confirm: confirm, failed: failed
        ) { [weak self] busy in
            if busy {
                self?.unpinning.insert(message)
            } else {
                self?.unpinning.remove(message)
            }
        }
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged || batch.pinsChanged.contains(channelId)
            || batch.replyTargetsChanged[channelId]?.isEmpty == false
        {
            reread()
        }
        guard opened else {
            return
        }
        if batch.everythingChanged || batch.pinsUnreadChanged.contains(channelId) {
            account.ackPins(channelId: channelId)
        }
        // A new session leaves the list shown but not loaded.
        if state == .notLoaded {
            Task { await load() }
        }
    }

    private func load() async {
        guard !asking else {
            return
        }
        asking = true
        defer { asking = false }
        try? await account.loadPins(channelId: channelId)
        reread()
    }

    private func reread() {
        let page = store.pins(channelId: channelId)
        state = page.state
        let rows = page.items.map { item in
            MessageListModel.Row(
                id: item.message.id, message: item.message, reply: ReplyLine.of(item.message, in: store))
        }
        if rows != self.rows {
            self.rows = rows
        }
        if page.hasMore != hasMore {
            hasMore = page.hasMore
        }
        let loadingMore = page.state == .loading && !rows.isEmpty
        if loadingMore != self.loadingMore {
            self.loadingMore = loadingMore
        }
        let phase: Phase =
            if !rows.isEmpty || (page.hasMore && page.state == .loaded) {
                .list
            } else {
                switch page.state {
                case .loaded: .empty
                case .failed: .failed
                case .notLoaded, .loading: .loading
                }
            }
        if phase != self.phase {
            self.phase = phase
        }
    }
}
