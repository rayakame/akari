import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// Guild 1 with text channel 11 and forum 13, whose tags are Bug (501) and Staff (502) and whose
// posts are 41–47; the user can read the forum's history.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ForumModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let forumId: ChannelId = id(13)
    let bug: TagId = id(501)
    let staff: TagId = id(502)
    // The search delays asked for; each ends at once unless its search was superseded.
    let waited = Locked<[Duration]>([])
    // While set, each delay waits for its gate.
    let holdDelays = Locked(false)
    let gates = Locked<[AsyncQueue<Void>]>([])
    // Delays that ran to their end.
    let ended = Locked(0)

    init() {
        account = FakeAccount(store: store)
        var forum = channel(13, kind: .guildForum, name: "ideas")
        forum.topic = "Be kind."
        forum.forum = ForumSettings(
            tags: [
                ForumTag(id: bug, name: "Bug", moderated: false, emoji: .unicode(emoji: "🐛")),
                ForumTag(id: staff, name: "Staff", moderated: true, emoji: nil),
            ],
            defaultReaction: nil, sort: .creationDate, layout: .gallery, tagMatching: .all,
            postSlowmode: 0, template: nil)
        let posts = (41...47).map { post(UInt64($0)) }
        store.update { state in
            state.connection = .online
            state.currentUser = user(1, name: "me")
            state.users[id(2)] = user(2, name: "Ray")
            state.add(guild(1))
            state.list([channel(11), forum], in: 1)
            for post in posts {
                state.channels[post.id] = post
            }
            state.permissions[id(13)] = [.viewChannel, .readMessageHistory]
            state.windows[id(11)] = window([1])
            state.show(message(1, in: 11))
        }
    }

    func post(_ raw: UInt64, tags: [TagId] = [], pinned: Bool = false) -> Channel {
        var post = thread(raw, in: 13, name: "post \(raw)")
        post.thread?.joined = false
        post.thread?.appliedTags = tags
        post.thread?.pinned = pinned
        return post
    }

    // A snowflake from `seconds` after 2015, Discord's epoch.
    func at(_ seconds: UInt64) -> UInt64 {
        (seconds * 1000) << 22
    }

    func list(
        _ active: [UInt64], archived: [UInt64] = [], older: OlderThreads = .all, newPosts: UInt32 = 0
    ) {
        let posts = ForumPosts(
            active: active.map(id), archived: archived.map(id), older: older, newPosts: newPosts)
        store.update { $0.forumPages[id(13)] = { _ in posts } }
    }

    func inForum() -> SessionModel {
        let session = SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: forumId)
        session.forum?.wait = { [waited, holdDelays, gates, ended] duration in
            waited.withLock { $0.append(duration) }
            if holdDelays.current {
                let gate = AsyncQueue<Void>()
                gates.withLock { $0.append(gate) }
                guard await gate.next() != nil else {
                    throw CancellationError()
                }
            }
            try Task.checkCancellation()
            ended.withLock { $0 += 1 }
        }
        return session
    }

    func forum(_ session: SessionModel) throws -> ForumModel {
        try #require(session.forum)
    }

    var searches: [String] {
        account.log.actions.compactMap { call in
            if case .searchPosts(_, let name, _) = call { name } else { nil }
        }
    }

    var olderLoads: [ForumView] {
        account.log.actions.compactMap { call in
            if case .loadOlderPosts(_, let view) = call { view } else { nil }
        }
    }

    var previewRequests: [[ChannelId]] {
        account.log.actions.compactMap { call in
            if case .requestPostPreviews(_, let ids) = call { ids } else { nil }
        }
    }

    // The pending search's delay ends and its search, if any, is answered.
    func settle(_ forum: ForumModel) async {
        await finished(forum.pendingSearch)
    }

    // MARK: The page

    @Test
    func aForumsRowOpensItsPageInTheMessageListsPlace() throws {
        list([41])
        let session = inForum()

        let forum = try forum(session)
        #expect(forum.forumId == forumId)
        #expect(session.messages == nil)
        #expect(session.openChannelId == forumId)
        #expect(account.log.actions.contains(.openForum(forumId)))
        #expect(account.log.actions.contains(.view(forumId)), "its guild is subscribed")
        session.close()
    }

    @Test
    func leavingTheForumClosesItsPage() throws {
        list([41])
        let session = inForum()

        session.open(channel: id(11))

        #expect(session.forum == nil)
        #expect(session.messages?.channelId == id(11))
        #expect(account.log.actions.last { if case .openForum = $0 { true } else { false } } == .openForum(nil))
        session.close()
    }

    @Test
    func goingToAForumOpensItsPage() throws {
        list([41])
        let session = inForum()
        session.open(channel: id(11))

        session.go(to: forumId)

        #expect(session.forum?.forumId == forumId)
        session.close()
    }

    @Test
    func theServerReopensItsForum() throws {
        list([41])
        let session = inForum()

        session.open(.home)
        session.open(.guild(id(1)))

        #expect(session.forum?.forumId == forumId)
        session.close()
    }

    // A relaunch on the forum opens its page, whether the cache knew the forum or READY brings it.
    @Test
    func aRelaunchOnTheForumOpensItsPage() async throws {
        let memory = AccountMemory(defaults: MemoryDefaults())
        memory.remember(.init(place: .guild(id(1)), channel: forumId), of: id(1))
        var known = FakeStore.State()
        store.update { state in
            known = state
            state.currentUser = nil
            state.channels[id(13)] = nil
        }
        list([41])
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }
        session.start()

        store.update { $0 = known }
        store.subscription.send(.ready)
        await store.subscription.batches.pulled(2)

        #expect(session.forum?.forumId == forumId)
        #expect(session.messages == nil)
        session.close()
    }

    @Test
    func aRelaunchOnACachedForumOpensItsPage() throws {
        let memory = AccountMemory(defaults: MemoryDefaults())
        memory.remember(.init(place: .guild(id(1)), channel: forumId), of: id(1))
        list([41])
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }

        session.start()

        #expect(session.forum?.forumId == forumId)
        #expect(session.messages == nil)
        session.close()
    }

    // MARK: Sort & View

    @Test
    func theSettingsStartAtTheForumsDefaults() throws {
        let session = inForum()

        let forum = try forum(session)
        #expect(forum.settings == ForumModel.Settings(
            sort: .creationDate, layout: .gallery, tags: [], matching: .all))
        #expect(forum.tags.map(\.name) == ["Bug", "Staff"])
        #expect(account.log.reads.contains(.forumPosts(forumId, ForumView(
            sort: .creationDate, tags: [], matching: .all))))
        session.close()
    }

    // Per forum for the session, as the official client's; Reset keeps the tag filter.
    @Test
    func settingsStayWithTheForumForTheSessionAndResetKeepsTheTags() throws {
        let session = inForum()
        let forum = try forum(session)

        forum.setSort(.latestActivity)
        forum.setLayout(.list)
        forum.toggle(tag: bug)
        forum.setMatching(.some)
        session.open(channel: id(11))
        session.open(channel: forumId)

        let again = try self.forum(session)
        #expect(again !== forum)
        #expect(again.settings == ForumModel.Settings(
            sort: .latestActivity, layout: .list, tags: [bug], matching: .some))
        again.resetToDefault()
        #expect(again.settings == ForumModel.Settings(
            sort: .creationDate, layout: .gallery, tags: [bug], matching: .all))
        session.close()
    }

    @Test
    func theTagFilterAsksTheStoreForItsPosts() throws {
        let session = inForum()
        let forum = try forum(session)
        account.log.forget()

        forum.toggle(tag: staff)
        forum.toggle(tag: bug)

        #expect(account.log.reads.contains(.forumPosts(forumId, ForumView(
            sort: .creationDate, tags: [bug, staff], matching: .all))))
        forum.toggle(tag: staff)
        #expect(forum.settings.tags == [bug])
        forum.clearTags()
        #expect(forum.settings.tags.isEmpty)
        session.close()
    }

    // MARK: Cards

    @Test
    func cardsFollowTheStoresListWithTheirDetails() throws {
        store.update { state in
            state.channels[id(41)] = post(41, tags: [staff, bug], pinned: true)
            state.channels[id(41)]?.thread?.messageCount = 12
            state.channels[id(41)]?.thread?.lastMessageId = id(at(500))
            state.channels[id(42)]?.thread?.messageCount = nil
        }
        list([41, 42], archived: [43])
        let session = inForum()

        let forum = try forum(session)
        #expect(forum.posts.map(\.id) == [id(41), id(42)])
        #expect(forum.archived.map(\.id) == [id(43)])
        let card = try #require(forum.posts.first)
        #expect(card.title == "post 41")
        #expect(card.pinned)
        #expect(card.tags.map(\.name) == ["Bug", "Staff"], "in the forum's order")
        #expect(card.count == "12")
        #expect(forum.posts[1].count == "0")
        session.close()
    }

    // Date Posted shows when each post was created; Recently Active its last activity.
    @Test
    func aCardsTimeFollowsTheSort() throws {
        store.update { state in
            state.channels[id(at(100))] = post(at(100))
            state.channels[id(at(100))]?.thread?.lastMessageId = id(at(500))
        }
        list([at(100)])
        let session = inForum()
        let forum = try forum(session)

        #expect(forum.posts.first?.time == ThreadBox.time(of: at(100)))
        forum.setSort(.latestActivity)
        #expect(forum.posts.first?.time == ThreadBox.time(of: at(500)))
        #expect(forum.posts.first?.created == ThreadBox.time(of: at(100)))
        session.close()
    }

    @Test
    func aCardShowsItsFirstMessageOnceAkariHasIt() throws {
        let first = message(41, in: 41, by: user(2, name: "Ray"), content: "hello")
        store.update { state in
            state.postPreviews[id(41)] = .loaded(message: first)
            state.postPreviews[id(42)] = .deleted
        }
        list([41, 42, 43])
        let session = inForum()

        let forum = try forum(session)
        #expect(forum.posts.map(\.preview) == [
            .message(first, authorName: "Ray"), .deleted, .loading,
        ])
        session.close()
    }

    @Test
    func aCardsMediaIsItsFirstMessagesMedia() throws {
        var first = message(41, in: 41, content: "")
        let picture = attachment(1, kind: .image, display: .media)
        first.attachments = [attachment(2, kind: .file, display: .file), picture]
        store.update { $0.postPreviews[id(41)] = .loaded(message: first) }
        list([41])
        let session = inForum()

        #expect(try forum(session).posts.first?.media == [picture])
        session.close()
    }

    // As the official client's card: itself and the next 4 of the active list, unfiltered.
    @Test
    func aShownCardWithoutItsFirstMessageAsksForItAndTheNextFour() throws {
        store.update { $0.postPreviews[id(41)] = .loaded(message: message(41, in: 41)) }
        let all = ForumPosts(
            active: (41...47).map(id), archived: [], older: .all, newPosts: 0)
        let tagged = ForumPosts(active: [id(44)], archived: [], older: .all, newPosts: 0)
        store.update { $0.forumPages[id(13)] = { view in view.tags.isEmpty ? all : tagged } }
        let session = inForum()
        let forum = try forum(session)
        forum.toggle(tag: bug)
        account.log.forget()

        forum.cardShown(id(44))
        forum.cardShown(id(41))

        #expect(previewRequests == [[id(44), id(45), id(46), id(47)]])
        session.close()
    }

    @Test
    func anArchivedCardAsksNothing() throws {
        list([41], archived: [43])
        let session = inForum()
        let forum = try forum(session)

        forum.cardShown(id(43))

        #expect(previewRequests.isEmpty)
        session.close()
    }

    @Test
    func aFirstMessageThatArrivesUpdatesItsCard() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)
        let first = message(41, in: 41, content: "hello")

        store.update { $0.postPreviews[id(41)] = .loaded(message: first) }
        session.apply(EventBatch([.postPreviewChanged(postId: id(41))]))

        #expect(forum.posts.first?.preview == .message(first, authorName: "someone"))
        session.close()
    }

    // A card still without its message asks again as the forum changes; the core spaces the
    // requests out.
    @Test
    func aShownCardStillWithoutItsMessageAsksAgainOnChanges() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)
        forum.cardShown(id(41))
        account.log.forget()

        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))
        forum.cardHidden(id(41))
        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))

        #expect(previewRequests == [[id(41)]])
        session.close()
    }

    @Test
    func theForumsChangesRereadTheList() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)

        list([42, 41], newPosts: 2)
        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))
        #expect(forum.posts.map(\.id) == [id(42), id(41)])
        #expect(forum.newPosts == 2)

        store.update { $0.channels[id(42)]?.name = "renamed" }
        session.apply(EventBatch([.channelUpdated(channelId: id(42), guildId: id(1))]))
        #expect(forum.posts.first?.title == "renamed")
        session.close()
    }

    // MARK: The pill

    @Test
    func thePillRefreshesTheList() throws {
        list([41], newPosts: 1)
        let session = inForum()
        let forum = try forum(session)

        forum.refresh()

        #expect(account.log.actions.contains(.refreshForum(forumId)))
        session.close()
    }

    // MARK: Older posts

    @Test
    func openingLoadsTheFirstPageOfOlderPosts() async throws {
        list([41], older: .notLoaded)
        let session = inForum()
        let forum = try forum(session)

        await forum.open()

        #expect(olderLoads == [ForumView(sort: .creationDate, tags: [], matching: .all)])
        session.close()
    }

    @Test
    func withoutReadMessageHistoryNothingOlderLoads() async throws {
        store.update { $0.permissions[id(13)] = [.viewChannel] }
        list([41], older: .notLoaded)
        let session = inForum()
        let forum = try forum(session)

        await forum.open()
        await forum.loadMore()

        #expect(!forum.canReadHistory)
        #expect(olderLoads.isEmpty)
        session.close()
    }

    @Test
    func loadMoreAsksOnlyWhenThereIsMore() async throws {
        list([41], older: .all)
        let session = inForum()
        let forum = try forum(session)

        await forum.loadMore()
        #expect(olderLoads.isEmpty)
        list([41], older: .more)
        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))
        await forum.loadMore()
        #expect(olderLoads.count == 1)
        session.close()
    }

    // Each tag filter pages on its own: a new one loads its first page.
    @Test
    func aNewTagFilterLoadsItsFirstPage() async throws {
        let loaded = ForumPosts(active: [id(41)], archived: [], older: .all, newPosts: 0)
        let fresh = ForumPosts(active: [], archived: [], older: .notLoaded, newPosts: 0)
        store.update { $0.forumPages[id(13)] = { view in view.tags.isEmpty ? loaded : fresh } }
        let session = inForum()
        let forum = try forum(session)
        await forum.open()

        forum.toggle(tag: bug)

        try await eventually { self.olderLoads.count == 1 }
        #expect(olderLoads == [ForumView(sort: .creationDate, tags: [bug], matching: .all)])
        session.close()
    }

    // A new connection forgets the pages: the first comes again while the page shows.
    @Test
    func aNewSessionLoadsTheFirstPageAgain() async throws {
        list([41], older: .all)
        let session = inForum()
        let forum = try forum(session)
        await forum.open()

        list([41], older: .notLoaded)
        session.apply(EventBatch([.ready]))

        try await eventually { self.olderLoads.count == 1 }
        session.close()
    }

    @Test
    func placeholdersShowWhileOlderPostsLoad() throws {
        list([], older: .loading)
        let session = inForum()
        let forum = try forum(session)
        #expect(forum.showsPlaceholders)
        #expect(!forum.isEmpty)

        list([], older: .all)
        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))
        #expect(!forum.showsPlaceholders)
        #expect(forum.isEmpty)
        session.close()
    }

    // MARK: Search

    // 350 ms after the query last changed, as the official client.
    @Test
    func aSearchRunsOnceTheQueryRests() async throws {
        account.postSearchReply.withLock { $0 = .success([id(42)]) }
        list([41, 42], archived: [43])
        let session = inForum()
        let forum = try forum(session)

        forum.setQuery("b")
        forum.setQuery("bu")
        await settle(forum)

        #expect(waited.current == [.milliseconds(350), .milliseconds(350)])
        #expect(searches == ["bu"])
        #expect(forum.search == .found)
        #expect(forum.posts.map(\.id) == [id(42)])
        #expect(forum.archived.isEmpty)
        #expect(forum.status == "1 post with \"bu\"")
        session.close()
    }

    // A delay a later change superseded ends without its search.
    @Test
    func onlyTheLastChangesDelayEnds() async throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)
        holdDelays.withLock { $0 = true }

        forum.setQuery("b")
        try await eventually { self.gates.current.count == 1 }
        forum.setQuery("bu")
        try await eventually { self.gates.current.count == 2 }
        for gate in gates.current {
            gate.send(())
        }
        await settle(forum)

        #expect(ended.current == 1)
        #expect(searches == ["bu"])
        session.close()
    }

    @Test
    func theStatusRowCountsAndSays() async throws {
        account.postSearchReply.withLock { $0 = .success([id(41), id(42)]) }
        list([41, 42])
        let session = inForum()
        let forum = try forum(session)
        account.holdPostSearches.withLock { $0 = true }

        forum.setQuery("post")
        try await eventually { forum.search == .searching }
        #expect(forum.status == "Searching...")
        #expect(forum.showsPlaceholders)
        account.postSearchEntries.send(())
        try await eventually { forum.search == .found }

        #expect(forum.status == "2 posts with \"post\"")
        session.close()
    }

    @Test
    func aFailedSearchFindsNoMatchingPosts() async throws {
        account.postSearchReply.withLock { $0 = .failure(.UnexpectedResponse) }
        list([41])
        let session = inForum()
        let forum = try forum(session)

        forum.setQuery("x")
        await settle(forum)

        #expect(forum.posts.isEmpty)
        #expect(forum.status == "No matching posts")
        session.close()
    }

    // The same query and tags aren't searched again; new tags are.
    @Test
    func onlyANewQueryOrNewTagsSearchAgain() async throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("bu")
        await settle(forum)

        forum.setQuery("b")
        forum.setQuery("bu")
        await settle(forum)
        #expect(searches == ["bu"])

        forum.toggle(tag: bug)
        await settle(forum)
        #expect(searches == ["bu", "bu"])
        #expect(account.log.actions.last {
            if case .searchPosts = $0 { true } else { false }
        } == .searchPosts(forumId, "bu", ForumView(sort: .creationDate, tags: [bug], matching: .all)))
        session.close()
    }

    @Test
    func clearingTheSearchShowsTheListAgain() async throws {
        account.postSearchReply.withLock { $0 = .success([id(42)]) }
        list([41, 42])
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("bu")
        await settle(forum)

        forum.clearSearch()

        #expect(forum.query.isEmpty)
        #expect(forum.search == .none)
        #expect(forum.status == nil)
        #expect(forum.posts.map(\.id) == [id(41), id(42)])
        session.close()
    }

    @Test
    func emptyingTheFieldClearsTheSearch() async throws {
        account.postSearchReply.withLock { $0 = .success([id(42)]) }
        list([41, 42])
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("bu")
        await settle(forum)

        forum.setQuery("")

        #expect(forum.search == .none)
        #expect(forum.posts.map(\.id) == [id(41), id(42)])
        session.close()
    }

    // Typed line breaks become spaces, and the field takes 100 characters, as the title's.
    @Test
    func theQueryIsOneLineOfAtMost100() throws {
        let session = inForum()
        let forum = try forum(session)

        forum.setQuery("a\nb\r\nc")
        #expect(forum.query == "a b c")
        forum.setQuery(String(repeating: "x", count: 120))
        #expect(forum.query.count == 100)
        session.close()
    }

    @Test
    func withoutReadMessageHistoryNothingIsSearched() async throws {
        store.update { $0.permissions[id(13)] = [.viewChannel] }
        let session = inForum()
        let forum = try forum(session)

        forum.setQuery("bu")

        #expect(forum.searchDenied)
        #expect(waited.current.isEmpty)
        #expect(searches.isEmpty)
        session.close()
    }

    // MARK: Review

    // Clearing a search shows the list again with the filter's first page, also for tags chosen
    // while the results showed.
    @Test
    func clearingASearchLoadsTheNewFiltersFirstPage() async throws {
        let loaded = ForumPosts(active: [id(41)], archived: [], older: .all, newPosts: 0)
        let fresh = ForumPosts(active: [], archived: [], older: .notLoaded, newPosts: 0)
        store.update { $0.forumPages[id(13)] = { view in view.tags.isEmpty ? loaded : fresh } }
        let session = inForum()
        let forum = try forum(session)
        await forum.open()
        forum.setQuery("x")
        await settle(forum)
        forum.toggle(tag: bug)
        await settle(forum)

        forum.clearSearch()

        try await eventually { self.olderLoads.count == 1 }
        #expect(olderLoads == [ForumView(sort: .creationDate, tags: [bug], matching: .all)])
    }

    @Test
    func aSearchClearedWhileItRunsShowsNothing() async throws {
        account.postSearchReply.withLock { $0 = .success([id(42)]) }
        account.holdPostSearches.withLock { $0 = true }
        list([41, 42])
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("x")
        try await eventually { forum.search == .searching }

        forum.clearSearch()
        account.postSearchEntries.send(())
        try await Task.sleep(for: .milliseconds(50))

        #expect(forum.search == .none)
        #expect(forum.posts.map(\.id) == [id(41), id(42)])
        session.close()
    }

    // A query typed while a search runs is searched once it's answered.
    @Test
    func aQueryTypedWhileASearchRunsIsSearchedAfter() async throws {
        account.holdPostSearches.withLock { $0 = true }
        list([41])
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("a")
        try await eventually { forum.search == .searching }

        forum.setQuery("ab")
        await settle(forum)
        account.postSearchEntries.send(())
        try await eventually { self.searches.count == 2 }
        account.postSearchEntries.send(())
        try await eventually { forum.search == .found }

        #expect(searches == ["a", "ab"])
        session.close()
    }

    @Test
    func nothingOlderLoadsWhileResultsShow() async throws {
        list([41], older: .more)
        let session = inForum()
        let forum = try forum(session)
        forum.setQuery("x")
        await settle(forum)

        await forum.loadMore()

        #expect(olderLoads.isEmpty)
        session.close()
    }

    // A post's newest message or count changes without the post's own event.
    @Test
    func aPostsActivityRereadsItsCard() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)

        store.update { $0.channels[id(41)]?.thread?.messageCount = 9 }
        session.apply(EventBatch([.threadActivity(guildId: id(1), parentId: forumId, threadId: id(41))]))

        #expect(forum.posts.first?.count == "9")
        session.close()
    }

    // A new connection or the display settings (spoilers, sensitive media) read the cards'
    // first messages again.
    @Test(arguments: [StoreEvent.ready, .displaySettingsChanged])
    func firstMessagesAreReadAgain(after event: StoreEvent) throws {
        let first = message(41, in: 41, content: "old")
        store.update { $0.postPreviews[id(41)] = .loaded(message: first) }
        list([41])
        let session = inForum()
        let forum = try forum(session)
        let next = message(41, in: 41, content: "new")

        store.update { $0.postPreviews[id(41)] = .loaded(message: next) }
        session.apply(EventBatch([event]))

        #expect(forum.posts.first?.preview == .message(next, authorName: "someone"))
        session.close()
    }

    @Test
    func losingReadMessageHistoryStopsTheSearches() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)

        store.update { $0.permissions[id(13)] = [.viewChannel] }
        session.apply(EventBatch([.guildUpdated(guildId: id(1))]))

        #expect(!forum.canReadHistory)
        session.close()
    }

    // Each forum keeps its own Sort & View.
    @Test
    func eachForumKeepsItsOwnSettings() throws {
        var other = channel(15, kind: .guildForum, name: "help")
        other.forum = ForumSettings(
            tags: [], defaultReaction: nil, sort: .latestActivity, layout: .list,
            tagMatching: .some, postSlowmode: 0, template: nil)
        store.update { state in
            state.channels[id(15)] = other
            state.channelLists[id(1)]?.append(id(15))
        }
        let session = inForum()
        try forum(session).setLayout(.list)

        session.open(channel: id(15))
        #expect(try forum(session).settings.layout == .list)
        try forum(session).setLayout(.gallery)
        session.open(channel: forumId)

        #expect(try forum(session).settings.layout == .list)
        session.open(channel: id(15))
        #expect(try forum(session).settings.layout == .gallery)
        session.close()
    }

    // A card that shows twice for a moment (a section change) still counts as shown.
    @Test
    func aCardShownTwiceStaysShownUntilBothGo() throws {
        list([41])
        let session = inForum()
        let forum = try forum(session)
        forum.cardShown(id(41))
        forum.cardShown(id(41))
        forum.cardHidden(id(41))
        account.log.forget()

        session.apply(EventBatch([.forumPostsChanged(forumId: forumId)]))

        #expect(previewRequests == [[id(41)]])
        session.close()
    }

    // Behind the age gate the forum isn't opened, and nothing is asked for it.
    @Test
    func aGatedForumOpensOnceTheUserAgrees() throws {
        store.update { $0.ageGates[id(13)] = .ask(server: false) }
        list([41])
        let session = inForum()
        #expect(!account.log.actions.contains(.openForum(forumId)))
        #expect(!account.log.actions.contains(.view(forumId)))

        store.update { $0.ageGates[id(13)] = .open }
        session.apply(EventBatch([.ageGateChanged(guildId: id(1))]))

        #expect(account.log.actions.contains(.openForum(forumId)))
        #expect(account.log.actions.contains(.view(forumId)))
        session.close()
    }

    // MARK: Reading

    // The forum's page is read like a channel, so the core acks it at "now".
    @Test
    func theForumsPageIsRead() throws {
        list([41])
        let session = inForum()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)

        #expect(session.reading == [forumId])
        #expect(account.readingCalls.current.last == [forumId])
        session.setWindowKey(false)
        #expect(session.reading.isEmpty)
        session.close()
    }

    @Test
    func anAgeGatedForumIsNotRead() throws {
        store.update { $0.ageGates[id(13)] = .ask(server: false) }
        list([41])
        let session = inForum()
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)

        #expect(session.reading.isEmpty)
        session.close()
    }

    func attachment(_ raw: UInt64, kind: AttachmentKind, display: AttachmentDisplay) -> AkariFFI.Attachment {
        AkariFFI.Attachment(
            id: id(raw), filename: "a.png", title: nil, description: nil, contentType: nil, size: 1,
            kind: kind, width: 4, height: 4, placeholder: nil, spoiler: false, animated: false,
            key: .attachment(channelId: id(41), messageId: id(41), attachmentId: id(raw)),
            display: display, cover: nil)
    }
}
