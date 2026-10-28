import Foundation
import Observation

/// A forum channel's page, shown in the message list's place as in Discord's client: its posts
/// as cards, its archived posts page by page under "Older Posts", or the posts a search found,
/// with the tag filter and Sort & View.
@MainActor @Observable
public final class ForumModel {
    /// Sort & View and the tag filter, kept per forum for the session as the official client
    /// keeps them; they start at the forum's defaults.
    public struct Settings: Equatable, Sendable {
        public var sort: ForumSort
        public var layout: ForumLayout
        /// Only posts with these tags; none for every post.
        public var tags: Set<TagId>
        public var matching: TagMatching

        public init(sort: ForumSort, layout: ForumLayout, tags: Set<TagId>, matching: TagMatching) {
            self.sort = sort
            self.layout = layout
            self.tags = tags
            self.matching = matching
        }

        static func defaults(of forum: ForumSettings?) -> Settings {
            Settings(
                sort: forum?.sort ?? .latestActivity, layout: forum?.layout ?? .list, tags: [],
                matching: forum?.tagMatching ?? .some)
        }
    }

    public struct Card: Equatable, Sendable, Identifiable {
        public enum Preview: Equatable, Sendable {
            /// Not here yet; a shown card asks for it.
            case loading
            /// The post's first message, by `authorName`.
            case message(Message, authorName: String)
            /// "Original message was deleted"
            case deleted
        }

        public let id: ChannelId
        public let title: String
        public let pinned: Bool
        /// In the forum's order.
        public let tags: [ForumTag]
        public let preview: Preview
        /// "0", "12", "50+", "100k+"
        public let count: String
        /// Its last activity, or when it was posted under Date Posted.
        public let time: Date
        /// When it was posted: "Posted …".
        public let created: Date
        /// Its first message's images and videos.
        public let media: [Attachment]
    }

    public enum Search: Equatable, Sendable {
        /// No query, or none searched yet.
        case none
        case searching
        /// `posts` are what Discord found, in its order; none when nothing matched or the search
        /// failed.
        case found
    }

    /// The search field takes as much as a post's title.
    public static let queryLimit = 100
    static let searchDelay = Duration.milliseconds(350)

    public let forumId: ChannelId
    /// The forum: its name, its guidelines (`topic`) and its tags.
    public private(set) var channel: Channel?
    public private(set) var settings: Settings
    /// The active posts, pinned first; while a search shows, what it found.
    public private(set) var posts: [Card] = []
    /// The archived posts the loaded pages reach; none while a search shows.
    public private(set) var archived: [Card] = []
    public private(set) var older: OlderThreads = .notLoaded
    /// Others' posts since the list last refreshed: the "N New Posts" pill.
    public private(set) var newPosts: UInt32 = 0
    public private(set) var query = ""
    public private(set) var search: Search = .none
    /// The user can read the forum's history: older posts load and searches run.
    public private(set) var canReadHistory = false

    public var tags: [ForumTag] { channel?.forum?.tags ?? [] }

    /// The status row under the search field; `nil` hides it.
    public var status: String? {
        switch search {
        case .none:
            return nil
        case .searching:
            return "Searching..."
        case .found where posts.isEmpty:
            return "No matching posts"
        case .found:
            let noun = posts.count == 1 ? "post" : "posts"
            return "\(posts.count) \(noun) with \"\(searched?.query ?? query)\""
        }
    }

    /// A query is typed where the user can't search: Discord's box takes the results' place.
    public var searchDenied: Bool {
        !query.isEmpty && !canReadHistory
    }

    /// Placeholder cards under the list: a search or a page of older posts loads.
    public var showsPlaceholders: Bool {
        search == .searching
            || (search == .none && canReadHistory && (older == .loading || older == .notLoaded))
    }

    /// "Be the first to start the conversation!": no post to show and nothing loading.
    public var isEmpty: Bool {
        search == .none && posts.isEmpty && archived.isEmpty && !showsPlaceholders
    }

    @ObservationIgnored var wait: @Sendable (Duration) async throws -> Void = {
        try await Task.sleep(for: $0)
    }
    /// The search waiting for the query to rest, then running.
    @ObservationIgnored private(set) var pendingSearch: Task<Void, Never>?

    private struct SearchKey: Equatable {
        let query: String
        let tags: Set<TagId>
    }

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let remember: (Settings) -> Void
    @ObservationIgnored private let guildId: GuildId?
    @ObservationIgnored private var opened = false
    @ObservationIgnored private var resultIds: [ChannelId]?
    @ObservationIgnored private var searched: SearchKey?
    @ObservationIgnored private var searching = false
    @ObservationIgnored private var searchAgain = false
    // Clearing the search makes one still on its way stale.
    @ObservationIgnored private var searchGeneration = 0
    @ObservationIgnored private var listedIds: Set<ChannelId> = []
    @ObservationIgnored private var shownCards: Set<ChannelId> = []
    // `nil` inside: the store has no first message for the post.
    @ObservationIgnored private var previews: [ChannelId: PostPreview?] = [:]

    init(
        forumId: ChannelId, settings: Settings?, store: Store, account: Account,
        remember: @escaping (Settings) -> Void
    ) {
        self.forumId = forumId
        self.store = store
        self.account = account
        self.remember = remember
        let channel = store.channel(id: forumId)
        self.channel = channel
        guildId = channel?.guildId
        self.settings = settings ?? .defaults(of: channel?.forum)
        reread()
    }

    /// Loads the first page of older posts for the tag filter, as the official client does when
    /// the page shows, and again whenever the pages are forgotten while it shows (a new session).
    public func open() async {
        opened = true
        if older == .notLoaded {
            await loadMore()
        }
    }

    /// The view calls it when the list's bottom comes within 200 points. Nothing loads while a
    /// search shows.
    public func loadMore() async {
        guard canReadHistory, resultIds == nil, older == .notLoaded || older == .more else {
            return
        }
        older = .loading
        try? await account.loadOlderPosts(forumId: forumId, view: view)
        reread()
    }

    /// The "N New Posts" pill: the list takes in what came since it last refreshed.
    public func refresh() {
        account.refreshForum(forumId: forumId)
    }

    // MARK: Sort & View

    public func setSort(_ sort: ForumSort) {
        change { $0.sort = sort }
    }

    public func setLayout(_ layout: ForumLayout) {
        change { $0.layout = layout }
    }

    public func setMatching(_ matching: TagMatching) {
        change { $0.matching = matching }
    }

    /// A tag's pill: in or out of the filter.
    public func toggle(tag: TagId) {
        change { settings in
            if settings.tags.remove(tag) == nil {
                settings.tags.insert(tag)
            }
        }
    }

    public func clearTags() {
        change { $0.tags = [] }
    }

    /// "Reset to default": the forum's sort, view and tag matching; the tag filter stays.
    public func resetToDefault() {
        let defaults = Settings.defaults(of: channel?.forum)
        change { settings in
            settings.sort = defaults.sort
            settings.layout = defaults.layout
            settings.matching = defaults.matching
        }
    }

    private func change(_ edit: (inout Settings) -> Void) {
        var next = settings
        edit(&next)
        guard next != settings else {
            return
        }
        let newTags = next.tags != settings.tags
        settings = next
        remember(next)
        reread()
        if newTags, !query.isEmpty {
            scheduleSearch()
        }
        loadFirstPageIfForgotten()
    }

    // MARK: Search

    /// The field's text, on one line and at most `queryLimit` long. The search runs once it rests
    /// for 350 ms; emptying the field shows the list again.
    public func setQuery(_ text: String) {
        let line = String(String(text.map { $0.isNewline ? " " : $0 }).prefix(Self.queryLimit))
        guard line != query else {
            return
        }
        query = line
        if line.isEmpty {
            clearSearch()
        } else {
            scheduleSearch()
        }
    }

    /// The status row's "Clear".
    public func clearSearch() {
        query = ""
        pendingSearch?.cancel()
        pendingSearch = nil
        searchGeneration += 1
        searched = nil
        resultIds = nil
        search = .none
        reread()
    }

    private func scheduleSearch() {
        pendingSearch?.cancel()
        pendingSearch = nil
        guard canReadHistory else {
            return
        }
        pendingSearch = Task { [weak self, wait] in
            do {
                try await wait(Self.searchDelay)
            } catch {
                return
            }
            await self?.runSearch()
        }
    }

    // As the official client: not again for the same query and tags, and not while one runs;
    // one asked meanwhile runs after it.
    private func runSearch() async {
        guard !query.isEmpty, canReadHistory else {
            return
        }
        let key = SearchKey(query: query, tags: settings.tags)
        guard key != searched else {
            return
        }
        if searching {
            searchAgain = true
            return
        }
        searching = true
        searched = key
        search = .searching
        resultIds = []
        reread()
        let generation = searchGeneration
        let found = (try? await account.searchPosts(forumId: forumId, name: key.query, view: view))
        searching = false
        if generation == searchGeneration {
            resultIds = (found ?? nil) ?? []
            search = .found
            reread()
        }
        if searchAgain {
            searchAgain = false
            await runSearch()
        }
    }

    // MARK: Cards

    /// A card came on screen: one without its first message asks for it and the next 4 of the
    /// active list, as the official client's card does.
    public func cardShown(_ id: ChannelId) {
        shownCards.insert(id)
        askPreviews(for: [id])
    }

    public func cardHidden(_ id: ChannelId) {
        shownCards.remove(id)
    }

    private func askPreviews(for shown: Set<ChannelId>) {
        let missing = shown.filter { preview(of: $0) == nil }
        guard !missing.isEmpty else {
            return
        }
        let active = store.forumPosts(
            forumId: forumId,
            view: ForumView(sort: settings.sort, tags: [], matching: settings.matching)
        ).active
        var asked: [ChannelId] = []
        for index in active.indices where missing.contains(active[index]) {
            for next in active[index..<min(index + 5, active.endIndex)]
            where !asked.contains(next) && preview(of: next) == nil {
                asked.append(next)
            }
        }
        if !asked.isEmpty {
            account.requestPostPreviews(forumId: forumId, postIds: asked)
        }
    }

    private func preview(of post: ChannelId) -> PostPreview? {
        if let known = previews[post] {
            return known
        }
        let read = store.postPreview(postId: post)
        previews[post] = .some(read)
        return read
    }

    // MARK: Changes

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged {
            previews = [:]
        } else {
            for post in batch.previewsChanged {
                previews[post] = nil
            }
        }
        if batch.everythingChanged || batch.forumsChanged.contains(forumId)
            || batch.threadBrowsersChanged.contains(forumId)
            || !batch.previewsChanged.isDisjoint(with: listedIds) || touches(batch)
        {
            reread()
            askPreviews(for: shownCards)
        }
        loadFirstPageIfForgotten()
    }

    private func loadFirstPageIfForgotten() {
        if opened, older == .notLoaded, canReadHistory, resultIds == nil {
            Task { await loadMore() }
        }
    }

    // The forum, a post it shows, or the user's permissions.
    private func touches(_ batch: EventBatch) -> Bool {
        guard let guildId else {
            return false
        }
        if batch.membersChanged.contains(guildId) || batch.guildsChanged.contains(guildId)
            || !batch.removedChannels.isDisjoint(with: listedIds)
        {
            return true
        }
        let changed = batch.channelsChanged[guildId] ?? []
        return changed.contains(forumId) || !changed.isDisjoint(with: listedIds)
    }

    private var view: ForumView {
        ForumView(
            sort: settings.sort, tags: settings.tags.sorted { $0.rawValue < $1.rawValue },
            matching: settings.matching)
    }

    private func reread() {
        let channel = store.channel(id: forumId)
        if channel != self.channel {
            self.channel = channel
        }
        canReadHistory =
            store.permissions(channelId: forumId)?.contains(.readMessageHistory) ?? false
        let page = store.forumPosts(forumId: forumId, view: view)
        older = page.older
        newPosts = page.newPosts
        let (posts, archived) =
            if let resultIds {
                (cards(resultIds), [Card]())
            } else {
                (cards(page.active), cards(page.archived))
            }
        if posts != self.posts {
            self.posts = posts
        }
        if archived != self.archived {
            self.archived = archived
        }
        listedIds = Set(posts.map(\.id) + archived.map(\.id))
    }

    private func cards(_ ids: [ChannelId]) -> [Card] {
        store.channels(ids: ids).compactMap(card)
    }

    private func card(_ post: Channel) -> Card? {
        guard let info = post.thread else {
            return nil
        }
        let posted = ThreadBox.time(of: post.id.rawValue)
        let first: Message? =
            if case .loaded(let message) = preview(of: post.id) { message } else { nil }
        let shown: Card.Preview =
            switch preview(of: post.id) {
            case .loaded(let message): .message(message, authorName: message.author.displayName)
            case .deleted: .deleted
            case nil: .loading
            }
        return Card(
            id: post.id, title: post.name ?? "", pinned: info.pinned,
            tags: tags.filter { info.appliedTags.contains($0.id) }, preview: shown,
            count: ThreadBox.number(count: info.messageCount ?? 0, threadId: post.id),
            time: settings.sort == .creationDate
                ? posted : ThreadBrowserModel.lastActivity(post, info),
            created: info.createdAt ?? posted,
            media: first?.attachments.filter { $0.display == .media } ?? [])
    }
}
