import Foundation
import Observation

/// The reactions window of one message: its emoji by count on the left, the users who reacted
/// with the chosen one on the right, a hundred at a time. Akari shows one pill per emoji, so an
/// emoji's users are its plain reactions' and then its super reactions'.
@MainActor @Observable
public final class ReactorsModel: Identifiable {
    public struct Reactor: Equatable, Sendable, Identifiable {
        public let user: User
        /// Which of the emoji's two counts the user is in: the removal names it.
        public let kind: ReactionKind
        /// The row has its ✕: the user's own, or any for those who may remove reactions.
        public let removable: Bool

        public var id: UserId { user.id }
    }

    public static let pageSize: UInt8 = 100

    /// The row's own IDs, as the list asked the store with.
    public let channelId: ChannelId
    public let messageId: MessageId
    /// One per emoji, the most reactions first.
    public private(set) var tabs: [ReactionPill] = []
    public private(set) var selected: ReactionPill.ID?
    public private(set) var reactors: [Reactor] = []
    /// A page is on its way.
    public private(set) var loading = false
    /// Discord has more users of the chosen emoji than are listed.
    public private(set) var hasMore = false
    /// The last page didn't come; `loadMore` asks again.
    public private(set) var failed = false

    /// The message has no reactions left: the window closes itself.
    public var isEmpty: Bool { tabs.isEmpty }

    private struct Paging {
        var users: [User] = []
        var after: UserId?
        var done = false
    }

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let userId: UserId
    @ObservationIgnored private var pages: [ReactionKind: Paging] = [:]
    // Answers for an emoji the window has left, or from before a refresh, are dropped.
    @ObservationIgnored private var generation = 0
    @ObservationIgnored private var refreshOwed = false
    @ObservationIgnored private var removing: Set<UserId> = []
    @ObservationIgnored var refused: () -> Void = {}

    init(
        channelId: ChannelId, messageId: MessageId, emoji: ReactionPill.ID? = nil, userId: UserId,
        store: Store, account: Account
    ) {
        self.channelId = channelId
        self.messageId = messageId
        self.userId = userId
        self.store = store
        self.account = account
        readTabs()
        selected = tabs.contains { $0.id == emoji } ? emoji : tabs.first?.id
    }

    private var pill: ReactionPill? {
        tabs.first { $0.id == selected }
    }

    /// Loads the chosen emoji's first users.
    public func open() async {
        if reactors.isEmpty, !loading {
            await loadMore()
        }
    }

    public func select(_ id: ReactionPill.ID) async {
        guard id != selected, tabs.contains(where: { $0.id == id }) else {
            return
        }
        selected = id
        restart()
        await loadMore()
    }

    private func restart() {
        generation += 1
        pages = [:]
        reactors = []
        loading = false
        failed = false
        refreshOwed = false
        hasMore = pill != nil
    }

    /// The view calls it when the list's end comes near, and for another try after a failure.
    public func loadMore() async {
        guard !loading, let pill, let kind = nextKind(of: pill) else {
            return
        }
        let asked = generation
        loading = true
        failed = false
        let page = try? await account.reactors(
            channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: kind,
            after: pages[kind]?.after, limit: Self.pageSize)
        guard asked == generation else {
            return
        }
        loading = false
        if let page {
            var paging = pages[kind] ?? Paging()
            let known = Set(paging.users.map(\.id))
            paging.users += page.users.filter { !known.contains($0.id) }
            paging.after = page.users.last?.id ?? paging.after
            paging.done = !page.hasMore
            pages[kind] = paging
        } else {
            failed = true
        }
        list()
        if refreshOwed {
            refreshOwed = false
            await refresh()
        }
    }

    private func nextKind(of pill: ReactionPill) -> ReactionKind? {
        [ReactionKind.normal, .burst].first { kind in
            (kind == .normal ? pill.normal : pill.burst) > 0 && pages[kind]?.done != true
        }
    }

    private func list() {
        let moderates = store.messageActions(channelId: channelId, messageId: messageId).removeReactions
        var seen: Set<UserId> = []
        var listed: [Reactor] = []
        for kind in [ReactionKind.normal, .burst] {
            for user in pages[kind]?.users ?? []
            where !removing.contains(user.id) && seen.insert(user.id).inserted {
                listed.append(
                    Reactor(user: user, kind: kind, removable: moderates || user.id == userId))
            }
        }
        if listed != reactors {
            reactors = listed
        }
        let more = pill.flatMap(nextKind) != nil
        if more != hasMore {
            hasMore = more
        }
    }

    /// A row's ✕: the row goes at once and comes back if Discord refuses.
    public func remove(_ reactor: Reactor) async {
        guard reactor.removable, let pill, removing.insert(reactor.id).inserted else {
            return
        }
        list()
        do {
            try await account.removeReaction(
                channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: reactor.kind,
                userId: reactor.id)
            for kind in pages.keys {
                pages[kind]?.users.removeAll { $0.id == reactor.id }
            }
            removing.remove(reactor.id)
        } catch {
            removing.remove(reactor.id)
            if let error = error as? ReactionError, error == .Forbidden || error == .Blocked {
                refused()
            }
        }
        list()
    }

    func apply(_ batch: EventBatch) {
        let touched =
            batch.everythingChanged
            || [batch.reactionsChanged, batch.updatedMessages, batch.deletedMessages]
                .contains { $0.values.contains { $0.contains(messageId) } }
        guard touched else {
            return
        }
        let before = pill
        readTabs()
        guard let now = pill else {
            // The chosen emoji left the message: the first one takes its place.
            selected = tabs.first?.id
            restart()
            if selected != nil {
                Task { await loadMore() }
            }
            return
        }
        if now != before {
            Task { await refresh() }
        }
    }

    private func readTabs() {
        let read = store.displaySettings().renderReactions
            ? ReactionPill.byCount(
                ReactionPill.merged(store.reactions(channelId: channelId, messageId: messageId)))
            : []
        if read != tabs {
            tabs = read
        }
    }

    // The loaded users stay while each kind's first page is asked again: the new page takes
    // the place of the users up to its last one, and what was loaded past it stays.
    private func refresh() async {
        guard !loading else {
            refreshOwed = true
            return
        }
        guard let pill else {
            return
        }
        let asked = generation
        loading = true
        for kind in [ReactionKind.normal, .burst] {
            let count = kind == .normal ? pill.normal : pill.burst
            guard count > 0 else {
                pages[kind] = nil
                continue
            }
            guard let held = pages[kind] else {
                continue
            }
            let page = try? await account.reactors(
                channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: kind,
                after: nil, limit: Self.pageSize)
            guard asked == generation else {
                return
            }
            guard let page else {
                continue
            }
            var paging = Paging(users: page.users, after: page.users.last?.id, done: !page.hasMore)
            if page.hasMore, let last = page.users.last?.id {
                let fresh = Set(page.users.map(\.id))
                let tail = held.users.filter { $0.id > last && !fresh.contains($0.id) }
                if !tail.isEmpty {
                    paging.users += tail
                    paging.after = held.after
                    paging.done = held.done
                }
            }
            pages[kind] = paging
        }
        loading = false
        list()
        if refreshOwed {
            refreshOwed = false
            await refresh()
        }
    }
}
