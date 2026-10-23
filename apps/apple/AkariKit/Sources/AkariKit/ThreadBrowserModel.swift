import Foundation
import Observation

/// A channel's Threads browser, as Discord's client shows it in the popout under the header's
/// Threads button and in the modal "See all threads" opens: its joined and other active threads
/// and, loaded page by page, its older ones; or the threads a search found.
@MainActor @Observable
public final class ThreadBrowserModel {
    public enum Presentation: Equatable, Sendable {
        case popout
        case modal
    }

    public struct Section: Equatable, Sendable, Identifiable {
        public enum Kind: Equatable, Sendable {
            case joined
            case others
            case older
        }

        public let kind: Kind
        /// "9 joined threads", "2 other active threads", "older threads": Discord's words.
        public let title: String
        public let rows: [Row]

        public var id: Kind { kind }
    }

    public struct Row: Equatable, Sendable, Identifiable {
        /// The second line.
        public enum Subtitle: Equatable, Sendable {
            /// An active thread's newest message: "Ray: test • 6h ago".
            case latest(Message, authorName: String, time: Date)
            /// "No recent messages • 1d ago": an active thread whose newest message isn't known.
            case noRecentMessages(time: Date)
            /// "Started by Ray • Last active 3d ago": an archived thread.
            case startedBy(User, name: String, time: Date)
            /// "Last active 3d ago": an archived thread whose owner isn't known.
            case lastActive(time: Date)
        }

        public struct Face: Equatable, Sendable {
            public let userId: UserId
            /// `nil` while the store doesn't know the user.
            public let user: User?
        }

        public let id: ChannelId
        public let name: String
        public let subtitle: Subtitle
        /// Some of its members, from Discord's preview.
        public let faces: [Face]
        /// "+3" for members the faces leave out, "50+" from 50 members; `nil` otherwise.
        public let more: String?
    }

    public let parentId: ChannelId
    public let presentation: Presentation
    public private(set) var sections: [Section] = []
    /// A search's threads in Discord's order; `nil` shows the sections.
    public private(set) var results: [Row]?
    public private(set) var query = ""
    /// The user can read the channel's history: the search field shows and older threads load.
    public private(set) var canSearch = false
    public private(set) var older: OlderThreads = .notLoaded

    /// The first page of older threads loads and nothing else shows yet.
    public var showsSpinner: Bool {
        results == nil && sections.isEmpty && canSearch
            && (older == .loading || older == .notLoaded)
    }

    /// "There are no threads.": nothing to show and nothing loading.
    public var isEmpty: Bool {
        results?.isEmpty ?? (sections.isEmpty && !showsSpinner)
    }

    @ObservationIgnored private let store: Store
    @ObservationIgnored private let account: Account
    @ObservationIgnored private let userId: UserId
    @ObservationIgnored private var resultIds: [ChannelId]?
    @ObservationIgnored private var searchingFor: String?
    @ObservationIgnored private var nextSearch: String?
    // Clearing the field makes a search still on its way stale.
    @ObservationIgnored private var searchGeneration = 0
    @ObservationIgnored private var opened = false
    @ObservationIgnored private let guildId: GuildId?
    @ObservationIgnored private var browsedIds: Set<ChannelId> = []
    @ObservationIgnored private var shownUsers: Set<UserId> = []
    // Who the rows being built show or name, an owner the store doesn't know yet too.
    @ObservationIgnored private var rowUsers: Set<UserId> = []

    init(
        parentId: ChannelId, presentation: Presentation, userId: UserId, store: Store,
        account: Account
    ) {
        self.parentId = parentId
        self.presentation = presentation
        self.userId = userId
        self.store = store
        self.account = account
        guildId = store.channel(id: parentId)?.guildId
        reread()
    }

    /// Loads the first page of older threads, as the official client does on opening, and again
    /// whenever the pages are forgotten while the browser shows (a new session). The pages stay
    /// for the connection, so opening again asks nothing.
    public func open() async {
        opened = true
        if older == .notLoaded {
            await loadMore()
        }
    }

    /// The view calls it when the list's bottom comes within 200 points.
    public func loadMore() async {
        guard canSearch, older == .notLoaded || older == .more else {
            return
        }
        older = .loading
        try? await account.loadOlderThreads(parentId: parentId)
        reread()
    }

    /// The field's text. Clearing it shows the sections again; other edits keep the results
    /// until Enter.
    public func setQuery(_ text: String) {
        query = text
        if text.isEmpty {
            resultIds = nil
            results = nil
            nextSearch = nil
            searchGeneration += 1
        }
    }

    /// Enter: searches by name, one search at a time. Enter with other text while one runs
    /// searches that text once it's answered, and only the last answer shows. A failed search, or
    /// one Discord can't answer yet, changes nothing.
    public func submit() async {
        guard !query.isEmpty else {
            return
        }
        if let running = searchingFor {
            nextSearch = query == running ? nil : query
            return
        }
        var text = query
        while true {
            let generation = searchGeneration
            searchingFor = text
            let found = try? await account.searchThreads(parentId: parentId, name: text)
            if let next = nextSearch {
                nextSearch = nil
                text = next
                continue
            }
            searchingFor = nil
            guard let found, generation == searchGeneration else {
                return
            }
            resultIds = found
            reread()
            return
        }
    }

    /// A thread this browser shows, for its row's menu.
    func channel(_ id: ChannelId) -> Channel? {
        browsedIds.contains(id) ? store.channel(id: id) : nil
    }

    func apply(_ batch: EventBatch) {
        if batch.everythingChanged || batch.threadBrowsersChanged.contains(parentId)
            || touches(batch)
        {
            reread()
        }
        if opened, older == .notLoaded {
            Task { await loadMore() }
        }
    }

    // A change to the channel, a thread it shows or a new one of the channel's, someone it
    // shows, or the user's permissions.
    private func touches(_ batch: EventBatch) -> Bool {
        guard let guildId else {
            return false
        }
        if batch.membersChanged.contains(guildId) || batch.guildsChanged.contains(guildId)
            || !batch.removedChannels.isDisjoint(with: browsedIds)
            || !batch.usersChanged.isDisjoint(with: shownUsers)
            || (batch.currentUserChanged && shownUsers.contains(userId))
        {
            return true
        }
        let changed = batch.channelsChanged[guildId] ?? []
        if changed.contains(parentId) || !changed.isDisjoint(with: browsedIds) {
            return true
        }
        let unknown = changed.subtracting(browsedIds)
        return !unknown.isEmpty
            && store.channels(ids: Array(unknown)).contains { $0.parentId == parentId }
    }

    private func reread() {
        canSearch = store.permissions(channelId: parentId)?.contains(.readMessageHistory) ?? false
        let browser = store.threadBrowser(parentId: parentId)
        older = browser.older
        var sections: [Section] = []
        if !browser.joined.isEmpty {
            sections.append(
                Section(
                    kind: .joined, title: "\(browser.joined.count) joined threads",
                    rows: rows(browser.joined)))
        }
        if !browser.others.isEmpty {
            sections.append(
                Section(
                    kind: .others, title: "\(browser.others.count) other active threads",
                    rows: rows(browser.others)))
        }
        if !browser.archived.isEmpty {
            sections.append(Section(kind: .older, title: "older threads", rows: rows(browser.archived)))
        }
        if sections != self.sections {
            self.sections = sections
        }
        if let resultIds {
            let results = rows(resultIds)
            if results != self.results {
                self.results = results
            }
        }
        browsedIds = Set(browser.joined + browser.others + browser.archived + (resultIds ?? []))
        shownUsers = rowUsers
        rowUsers = []
    }

    private func rows(_ ids: [ChannelId]) -> [Row] {
        store.channels(ids: ids).compactMap(row)
    }

    private func row(_ channel: Channel) -> Row? {
        guard let info = channel.thread else {
            return nil
        }
        rowUsers.formUnion(info.memberIdsPreview)
        if let owner = info.ownerId {
            rowUsers.insert(owner)
        }
        let subtitle: Row.Subtitle
        if !info.archived {
            if let latest = store.latestMessage(threadId: channel.id) {
                rowUsers.insert(latest.author.id)
                subtitle = .latest(
                    latest, authorName: latest.author.displayName,
                    time: ThreadBox.time(of: latest.id.rawValue))
            } else {
                subtitle = .noRecentMessages(time: Self.lastActivity(channel, info))
            }
        } else {
            let time = info.lastMessageId.map { ThreadBox.time(of: $0.rawValue) } ?? info.archivedAt
            if let owner = info.ownerId.flatMap(user) {
                subtitle = .startedBy(owner, name: owner.displayName, time: time)
            } else {
                subtitle = .lastActive(time: time)
            }
        }
        return Row(
            id: channel.id, name: channel.name ?? "", subtitle: subtitle,
            faces: info.memberIdsPreview.prefix(Self.facesShown).map {
                Row.Face(userId: $0, user: user($0))
            },
            more: Self.more(shown: min(info.memberIdsPreview.count, Self.facesShown), of: info))
    }

    private func user(_ id: UserId) -> User? {
        id == userId ? store.currentUser() : store.user(id: id)
    }

    // Discord's facepile counts up to 50, and shows 49 faces with a count beside them.
    private static let facesMax = 50
    private static let facesShown = facesMax - 1

    private static func more(shown: Int, of info: ThreadInfo) -> String? {
        guard !info.memberIdsPreview.isEmpty, let count = info.memberCount.map(Int.init) else {
            return nil
        }
        if count >= facesMax {
            return "\(facesMax)+"
        }
        return count > shown ? "+\(count - shown)" : nil
    }

    // Its newest message, else when it was created, else its ID's time.
    static func lastActivity(_ channel: Channel, _ info: ThreadInfo) -> Date {
        info.lastMessageId.map { ThreadBox.time(of: $0.rawValue) } ?? info.createdAt
            ?? ThreadBox.time(of: channel.id.rawValue)
    }
}

/// A row of the channel list's "More Active Threads" popout: an active thread the list doesn't
/// show.
public struct ThreadPeek: Equatable, Sendable, Identifiable {
    public let id: ChannelId
    public let name: String
    public let ownerId: UserId?
    /// `nil` while the store doesn't know the owner.
    public let owner: User?
    /// Its newest message, else when it was created.
    public let time: Date

    public init(id: ChannelId, name: String, ownerId: UserId?, owner: User?, time: Date) {
        self.id = id
        self.name = name
        self.ownerId = ownerId
        self.owner = owner
        self.time = time
    }
}
