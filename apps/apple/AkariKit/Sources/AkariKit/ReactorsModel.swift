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

    /// The message has no reactions left, was deleted, or reactions no longer show: the window
    /// closes itself.
    public var isEmpty: Bool { tabs.isEmpty }

    private struct Paging {
        var users: [User] = []
        var after: UserId?
        var done = false

        mutating func add(_ page: ReactorsPage) {
            let known = Set(users.map(\.id))
            users += page.users.filter { !known.contains($0.id) }
            let asked = after
            after = page.users.last?.id ?? after
            // An empty page, or one that ends where the last one did, has no user to ask on from.
            done = !page.hasMore || after == asked
        }
    }

    private enum Work {
        case page
        case refresh
    }

    private struct Removal: Hashable {
        let emoji: ReactionPill.ID
        let kind: ReactionKind
        let user: UserId
    }

    private enum Hidden: Equatable {
        case asking
        // With the refreshes begun so far: one asked before the answer may still bring the user.
        case answered(refreshes: Int)
    }

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let userId: UserId
    @ObservationIgnored private var pages: [ReactionKind: Paging] = [:]
    // Answers for an emoji the window has left are dropped.
    @ObservationIgnored private var generation = 0
    // One request at a time; what is asked for meanwhile is done after it.
    @ObservationIgnored private var work: Work?
    @ObservationIgnored private var refreshOwed = false
    @ObservationIgnored private var loadOwed = false
    @ObservationIgnored private var refreshes = 0
    @ObservationIgnored private var removing: [Removal: Hidden] = [:]
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
        readTabs(deleted: false)
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
        work = nil
        loading = false
        failed = false
        refreshOwed = false
        loadOwed = false
        removing = removing.filter { $0.value == .asking }
        hasMore = pill != nil
    }

    /// The view calls it when the list's end comes near, and for another try after a failure.
    public func loadMore() async {
        switch work {
        case .refresh:
            loadOwed = true
        case .page:
            break
        case nil:
            await page()
        }
    }

    // Goes on while a page shows no one new: nothing would ask for the next one.
    private func page() async {
        let asked = generation
        begin(.page)
        failed = false
        var grown = false
        while !grown, let pill, let kind = nextKind(of: pill) {
            let page = try? await account.reactors(
                channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: kind,
                after: pages[kind]?.after, limit: Self.pageSize)
            guard asked == generation else {
                return
            }
            guard let page else {
                failed = true
                break
            }
            pages[kind, default: Paging()].add(page)
            let shown = reactors.count
            list()
            grown = reactors.count > shown
        }
        await finish()
    }

    private func begin(_ work: Work) {
        self.work = work
        loading = true
    }

    private func finish() async {
        work = nil
        loading = false
        list()
        if refreshOwed {
            refreshOwed = false
            await refresh()
        } else if loadOwed {
            loadOwed = false
            await page()
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
            for user in pages[kind]?.users ?? [] where !hidden(user.id, kind) {
                guard seen.insert(user.id).inserted else {
                    continue
                }
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

    private func hidden(_ user: UserId, _ kind: ReactionKind) -> Bool {
        selected.map { removing[Removal(emoji: $0, kind: kind, user: user)] != nil } ?? false
    }

    /// A row's ✕: the row goes at once and comes back if Discord refuses.
    public func remove(_ reactor: Reactor) async {
        guard reactor.removable, let pill else {
            return
        }
        let removal = Removal(emoji: pill.id, kind: reactor.kind, user: reactor.id)
        guard removing[removal] == nil else {
            return
        }
        removing[removal] = .asking
        list()
        do {
            try await account.removeReaction(
                channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: reactor.kind,
                userId: reactor.id)
            if selected == removal.emoji {
                pages[removal.kind]?.users.removeAll { $0.id == removal.user }
            }
            if selected == removal.emoji, work != nil {
                removing[removal] = .answered(refreshes: refreshes)
                refreshOwed = true
            } else {
                removing[removal] = nil
            }
        } catch {
            removing[removal] = nil
            if let error = error as? ReactionError, error == .Forbidden || error == .Blocked {
                refused()
            }
        }
        list()
    }

    func apply(_ batch: EventBatch) {
        let names = { (messages: [ChannelId: Set<MessageId>]) in
            messages.values.contains { $0.contains(self.messageId) }
        }
        let reacted = names(batch.reactionsChanged)
        let deleted = names(batch.deletedMessages)
        let touched =
            batch.everythingChanged || batch.displaySettingsChanged || reacted || deleted
            || names(batch.updatedMessages) || batch.windowsChanged.contains(channelId)
        guard touched else {
            return
        }
        let before = tabs
        let chosen = pill
        readTabs(deleted: deleted)
        guard let now = pill else {
            // The chosen emoji left the message: the first one takes its place.
            selected = tabs.first?.id
            restart()
            if selected != nil {
                Task { await loadMore() }
            }
            return
        }
        // Reaction events name no user: one who left and one who came leave every count.
        if now != chosen || (reacted && tabs == before) {
            Task { await refresh() }
        }
    }

    private func readTabs(deleted: Bool) {
        var read: [ReactionPill] = []
        if !deleted, store.displaySettings().renderReactions {
            let reactions = store.reactions(channelId: channelId, messageId: messageId)
            // A reload after a reconnect may let go of the message: what shows stays.
            if reactions.isEmpty, store.messages(channelId: channelId, ids: [messageId]).isEmpty {
                return
            }
            read = ReactionPill.byCount(ReactionPill.merged(reactions))
        }
        if read != tabs {
            tabs = read
        }
    }

    // The loaded users stay while every page that was loaded is asked again: a new first page
    // says nothing of who is past it.
    private func refresh() async {
        guard work == nil else {
            refreshOwed = true
            return
        }
        guard let pill else {
            return
        }
        let asked = generation
        refreshes += 1
        let number = refreshes
        begin(.refresh)
        var whole = true
        for kind in [ReactionKind.normal, .burst] {
            guard (kind == .normal ? pill.normal : pill.burst) > 0 else {
                pages[kind] = nil
                continue
            }
            guard let held = pages[kind] else {
                continue
            }
            var fresh: Paging? = Paging()
            while let paging = fresh, !paging.done, paging.users.count < held.users.count {
                let page = try? await account.reactors(
                    channelId: channelId, messageId: messageId, emoji: pill.emoji, kind: kind,
                    after: paging.after, limit: Self.pageSize)
                guard asked == generation else {
                    return
                }
                fresh = page.map { page in
                    var paging = paging
                    paging.add(page)
                    return paging
                }
            }
            if let fresh {
                pages[kind] = fresh
            } else {
                whole = false
            }
        }
        if whole {
            removing = removing.filter { removal in
                if case .answered(let begun) = removal.value, removal.key.emoji == pill.id {
                    return begun >= number
                }
                return true
            }
        }
        await finish()
    }
}
