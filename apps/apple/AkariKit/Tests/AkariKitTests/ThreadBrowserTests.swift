import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1 with threads 31–35 under it, channel 12, a forum 13 with post 36, and a
// voice channel 14; the user is in 11, which can read its history.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ThreadBrowserTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
    }

    // A snowflake from `seconds` after 2015, Discord's epoch.
    func at(_ seconds: UInt64) -> UInt64 {
        (seconds * 1000) << 22
    }

    func time(_ raw: UInt64) -> Date {
        ThreadBox.time(of: raw)
    }

    func world() {
        var threads = (31...35).map { thread(UInt64($0), in: 11, name: "t\($0 - 30)") }
        for index in threads.indices {
            threads[index].thread?.joined = false
        }
        let post = thread(36, in: 13, name: "post")
        store.update { state in
            state.connection = .online
            state.currentUser = user(1, name: "me")
            state.users[id(2)] = user(2, name: "Ray")
            state.add(guild(1))
            state.list(
                [channel(11), channel(12), channel(13, kind: .guildForum), channel(14, kind: .guildVoice)],
                in: 1)
            for thread in threads + [post] {
                state.channels[thread.id] = thread
            }
            state.permissions[id(11)] = [.viewChannel, .readMessageHistory]
            state.windows[id(11)] = window([1])
            state.windows[id(12)] = window([2])
            state.show(message(1, in: 11), message(2, in: 12))
        }
    }

    func edit(_ raw: UInt64, _ change: (inout ThreadInfo) -> Void) {
        store.update { state in
            if var info = state.channels[id(raw)]?.thread {
                change(&info)
                state.channels[id(raw)]?.thread = info
            }
        }
    }

    func browse(
        joined: [UInt64] = [], others: [UInt64] = [], archived: [UInt64] = [],
        older: OlderThreads = .all
    ) {
        store.update { state in
            state.threadBrowsers[id(11)] = ThreadBrowser(
                joined: joined.map(id), others: others.map(id), archived: archived.map(id),
                older: older)
        }
    }

    func inChannel11() -> SessionModel {
        world()
        let session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    func browser(_ session: SessionModel, as presentation: ThreadBrowserModel.Presentation = .popout)
        -> ThreadBrowserModel
    {
        session.openThreadBrowser(of: id(11), as: presentation)
        return session.threadBrowser!
    }

    func rowIds(_ rows: [ThreadBrowserModel.Row]?) -> [ChannelId]? {
        rows?.map(\.id)
    }

    // MARK: Sections

    @Test
    func sectionsFollowTheStoresBrowserWithDiscordsTitles() {
        let session = inChannel11()
        browse(joined: [31], others: [33, 32], archived: [34])

        let browser = browser(session)

        #expect(browser.sections.map(\.title) == [
            "1 joined threads", "2 other active threads", "older threads",
        ])
        #expect(browser.sections.map { $0.rows.map(\.id) } == [[id(31)], [id(33), id(32)], [id(34)]])
        session.close()
    }

    @Test
    func emptySectionsAreLeftOut() {
        let session = inChannel11()
        browse(others: [32])

        let browser = browser(session)

        #expect(browser.sections.map(\.kind) == [.others])
        session.close()
    }

    @Test
    func anActiveRowShowsItsLatestMessage() {
        let session = inChannel11()
        let latest = message(at(500), in: 31, by: user(2, name: "Ray"), content: "test")
        store.update { $0.latestMessages[id(31)] = latest }
        browse(others: [31])

        let row = browser(session).sections.first?.rows.first

        #expect(row?.name == "t1")
        #expect(row?.subtitle == .latest(latest, authorName: "Ray", time: time(at(500))))
        session.close()
    }

    @Test
    func anActiveRowWithoutALatestMessageSaysNoRecentMessagesSinceItsLastActivity() {
        let session = inChannel11()
        edit(31) { $0.lastMessageId = id(at(400)) }
        edit(32) { $0.createdAt = Date(timeIntervalSince1970: 1_800_000_000) }
        browse(others: [31, 32, 33])

        let rows = browser(session).sections.first?.rows

        #expect(rows?.map(\.subtitle) == [
            .noRecentMessages(time: time(at(400))),
            .noRecentMessages(time: Date(timeIntervalSince1970: 1_800_000_000)),
            .noRecentMessages(time: time(33)),
        ])
        session.close()
    }

    @Test
    func anArchivedRowNamesItsOwnerAndWhenItWasLastActive() {
        let session = inChannel11()
        let archivedAt = Date(timeIntervalSince1970: 1_750_000_000)
        edit(34) { info in
            info.archived = true
            info.ownerId = id(2)
            info.lastMessageId = id(at(300))
        }
        edit(35) { info in
            info.archived = true
            info.ownerId = id(9)
            info.archivedAt = archivedAt
        }
        browse(archived: [34, 35])

        let rows = browser(session).sections.first?.rows

        #expect(rows?.map(\.subtitle) == [
            .startedBy(user(2, name: "Ray"), name: "Ray", time: time(at(300))),
            .lastActive(time: archivedAt),
        ])
        session.close()
    }

    @Test
    func theOwnerMayBeTheUser() {
        let session = inChannel11()
        edit(34) { info in
            info.archived = true
            info.ownerId = id(1)
        }
        browse(archived: [34])

        let row = browser(session).sections.first?.rows.first

        guard case .startedBy(_, let name, _)? = row?.subtitle else {
            Issue.record("expected the owner, got \(String(describing: row?.subtitle))")
            return
        }
        #expect(name == "me")
        session.close()
    }

    @Test
    func facesComeFromThePreviewWithTheRestCounted() {
        let session = inChannel11()
        edit(31) { info in
            info.memberIdsPreview = [id(2), id(7)]
            info.memberCount = 5
        }
        edit(32) { info in
            info.memberIdsPreview = [id(2)]
            info.memberCount = 60
        }
        edit(33) { info in
            info.memberIdsPreview = [id(2)]
            info.memberCount = 1
        }
        edit(34) { $0.memberCount = 4 }
        browse(others: [31, 32, 33, 34])

        let rows = browser(session).sections.first?.rows ?? []

        #expect(rows.map { $0.faces.map(\.userId) } == [[id(2), id(7)], [id(2)], [id(2)], []])
        #expect(rows.first?.faces.map(\.user) == [user(2, name: "Ray"), nil])
        #expect(rows.map(\.more) == ["+3", "50+", nil, nil])
        session.close()
    }

    // MARK: Loading

    @Test
    func openingLoadsTheFirstPageWhenTheUserCanReadTheHistory() async {
        let session = inChannel11()
        browse(older: .notLoaded)
        account.browserAfterOlderLoad.withLock {
            $0 = ThreadBrowser(joined: [], others: [], archived: [id(34)], older: .all)
        }
        let browser = browser(session)

        await browser.open()

        #expect(account.log.actions.filter { $0 == .loadOlderThreads(id(11)) }.count == 1)
        #expect(browser.sections.map(\.kind) == [.older])
        #expect(browser.canSearch)
        session.close()
    }

    @Test
    func withoutReadMessageHistoryThereIsNoSearchAndNoOlderThreads() async {
        let session = inChannel11()
        store.update { $0.permissions[id(11)] = [.viewChannel] }
        browse(older: .notLoaded)
        let browser = browser(session)

        await browser.open()
        await browser.loadMore()

        #expect(!browser.canSearch)
        #expect(!account.log.actions.contains(.loadOlderThreads(id(11))))
        #expect(browser.isEmpty)
        #expect(!browser.showsSpinner)
        session.close()
    }

    @Test
    func loadMoreAsksOnlyWhileDiscordHasMore() async {
        let session = inChannel11()
        browse(archived: [34], older: .all)
        let browser = browser(session)

        await browser.loadMore()
        browse(archived: [34], older: .more)
        session.apply(EventBatch([.channelUpdated(channelId: id(34), guildId: id(1))]))
        await browser.loadMore()

        #expect(account.log.actions.filter { $0 == .loadOlderThreads(id(11)) }.count == 1)
        session.close()
    }

    @Test
    func theSpinnerShowsOnlyWhileTheFirstPageLoadsWithNothingElseToShow() {
        let session = inChannel11()
        browse(older: .loading)
        let browser = browser(session)
        #expect(browser.showsSpinner)
        #expect(!browser.isEmpty)

        browse(others: [32], older: .loading)
        session.apply(EventBatch([.channelUpdated(channelId: id(32), guildId: id(1))]))
        #expect(!browser.showsSpinner)

        browse(older: .all)
        session.apply(EventBatch([.channelUpdated(channelId: id(32), guildId: id(1))]))
        #expect(!browser.showsSpinner)
        #expect(browser.isEmpty)
        session.close()
    }

    // MARK: Search

    @Test
    func enterSearchesAndShowsTheResultsInDiscordsOrder() async {
        let session = inChannel11()
        browse(joined: [31], others: [32])
        account.searchReply.withLock { $0 = .success([id(34), id(31)]) }
        let browser = browser(session)

        browser.setQuery("t1")
        #expect(browser.results == nil)
        await browser.submit()

        #expect(account.log.actions.contains(.searchThreads(id(11), "t1")))
        #expect(rowIds(browser.results) == [id(34), id(31)])
        session.close()
    }

    @Test
    func anEmptyQueryDoesntSearch() async {
        let session = inChannel11()
        let browser = browser(session)

        await browser.submit()

        #expect(!account.log.actions.contains { if case .searchThreads = $0 { true } else { false } })
        session.close()
    }

    @Test
    func clearingTheQueryShowsTheSectionsAgainButEditingKeepsTheResults() async {
        let session = inChannel11()
        browse(others: [32])
        account.searchReply.withLock { $0 = .success([id(31)]) }
        let browser = browser(session)
        browser.setQuery("t1")
        await browser.submit()

        browser.setQuery("t")
        #expect(rowIds(browser.results) == [id(31)])

        browser.setQuery("")
        #expect(browser.results == nil)
        #expect(browser.sections.map(\.kind) == [.others])
        session.close()
    }

    @Test
    func aFailedOrUnindexedSearchChangesNothing() async {
        let session = inChannel11()
        account.searchReply.withLock { $0 = .success([id(31)]) }
        let browser = browser(session)
        browser.setQuery("t1")
        await browser.submit()

        account.searchReply.withLock { $0 = .success(nil) }
        browser.setQuery("t2")
        await browser.submit()
        #expect(rowIds(browser.results) == [id(31)])

        account.searchReply.withLock { $0 = .failure(.UnexpectedResponse) }
        await browser.submit()
        #expect(rowIds(browser.results) == [id(31)])
        session.close()
    }

    @Test
    func aSearchWithoutMatchesShowsTheEmptyState() async {
        let session = inChannel11()
        browse(others: [32])
        account.searchReply.withLock { $0 = .success([]) }
        let browser = browser(session)
        browser.setQuery("zzzz")

        await browser.submit()

        #expect(browser.results == [])
        #expect(browser.isEmpty)
        session.close()
    }

    @Test
    func aSecondEnterWithTheSameTextWhileSearchingAsksOnce() async {
        let session = inChannel11()
        account.holdSearches.withLock { $0 = true }
        let browser = browser(session)
        browser.setQuery("t1")

        async let first: Void = browser.submit()
        await account.searchEntries.pulled(1)
        await browser.submit()
        account.searchEntries.send(())
        await first

        #expect(account.log.actions.filter { $0 == .searchThreads(id(11), "t1") }.count == 1)
        session.close()
    }

    // Discord drops an Enter while a search runs; Akari searches the latest text after it.
    @Test
    func theLatestTextWinsOverASearchStillRunning() async {
        let session = inChannel11()
        account.searchReplies.withLock { $0 = ["t1": [id(31)], "t2": [id(32)]] }
        account.holdSearches.withLock { $0 = true }
        let browser = browser(session)
        browser.setQuery("t1")

        async let first: Void = browser.submit()
        await account.searchEntries.pulled(1)
        browser.setQuery("t2")
        await browser.submit()
        account.searchEntries.send(())
        await account.searchEntries.pulled(2)
        account.searchEntries.send(())
        await first

        let searches = account.log.actions.filter {
            if case .searchThreads = $0 { true } else { false }
        }
        #expect(searches == [.searchThreads(id(11), "t1"), .searchThreads(id(11), "t2")])
        #expect(rowIds(browser.results) == [id(32)])
        session.close()
    }

    // MARK: Review fixes

    func loads() -> Int {
        account.log.actions.filter { $0 == .loadOlderThreads(id(11)) }.count
    }

    func until(_ condition: () -> Bool) async {
        for _ in 0..<200 where !condition() {
            await Task.yield()
        }
    }

    // READY forgets the pages; an open browser asks again instead of spinning.
    @Test
    func aNewSessionLoadsTheOlderThreadsAgain() async {
        let session = inChannel11()
        browse(older: .notLoaded)
        account.browserAfterOlderLoad.withLock {
            $0 = ThreadBrowser(joined: [], others: [], archived: [id(34)], older: .all)
        }
        let browser = browser(session)
        await browser.open()
        #expect(loads() == 1)

        browse(older: .notLoaded)
        session.apply(EventBatch([.ready]))
        await until { loads() == 2 }

        #expect(loads() == 2)
        session.close()
    }

    // Opened while another browser's page loads: that page's outcome rereads this one.
    @Test
    func aBrowserFollowsAPageItDidntAskFor() async {
        let session = inChannel11()
        browse(older: .loading)
        let browser = browser(session)
        await browser.open()
        #expect(browser.showsSpinner)

        browse(archived: [34], older: .all)
        session.apply(EventBatch([.olderThreadsChanged(parentId: id(11))]))

        #expect(browser.sections.map(\.kind) == [.older])
        #expect(!browser.showsSpinner)
        session.close()
    }

    @Test
    func clearingTheFieldWhileSearchingDropsTheAnswer() async {
        let session = inChannel11()
        browse(others: [32])
        account.searchReply.withLock { $0 = .success([id(31)]) }
        account.holdSearches.withLock { $0 = true }
        let browser = browser(session)
        browser.setQuery("t1")

        async let searched: Void = browser.submit()
        await account.searchEntries.pulled(1)
        browser.setQuery("")
        account.searchEntries.send(())
        await searched

        #expect(browser.results == nil)
        #expect(browser.sections.map(\.kind) == [.others])
        session.close()
    }

    @Test
    func changesElsewhereDontRereadTheBrowser() {
        let session = inChannel11()
        browse(others: [32])
        _ = browser(session)
        store.log.forget()

        session.apply(
            EventBatch([
                .channelUpdated(channelId: id(12), guildId: id(1)),
                .userUpdated(userId: id(9)),
                .channelRemoved(channelId: id(40), guildId: id(1)),
            ]))

        #expect(!store.reads.contains(.threadBrowser(id(11))))
        session.close()
    }

    @Test
    func aSearchResultsRowOffersJoining() async {
        let session = inChannel11()
        account.searchReply.withLock { $0 = .success([id(34)]) }
        let browser = browser(session)
        browser.setQuery("t4")

        await browser.submit()

        #expect(
            session.membershipItem(for: id(34))
                == SessionModel.MembershipItem(joins: true, enabled: true))
        session.close()
    }

    @Test
    func anUpdateOfSomeoneTheBrowserShowsRereadsIt() {
        let session = inChannel11()
        edit(32) { $0.memberIdsPreview = [id(2)] }
        browse(others: [32])
        let browser = browser(session)

        store.update { $0.users[id(2)] = user(2, name: "Rae") }
        session.apply(EventBatch([.userUpdated(userId: id(2))]))

        #expect(browser.sections.first?.rows.first?.faces.first?.user?.displayName == "Rae")
        session.close()
    }

    @Test
    func aNewThreadOfTheChannelRereadsIt() {
        let session = inChannel11()
        browse(others: [32])
        let browser = browser(session)

        browse(others: [35, 32])
        session.apply(EventBatch([.channelAdded(channelId: id(35), guildId: id(1))]))

        #expect(browser.sections.first?.rows.map(\.id) == [id(35), id(32)])
        session.close()
    }

    @Test
    func clearingTheFieldDropsASearchWaitingItsTurn() async {
        let session = inChannel11()
        account.searchReplies.withLock { $0 = ["t1": [id(31)], "t2": [id(32)]] }
        account.holdSearches.withLock { $0 = true }
        let browser = browser(session)
        browser.setQuery("t1")

        async let first: Void = browser.submit()
        await account.searchEntries.pulled(1)
        browser.setQuery("t2")
        await browser.submit()
        browser.setQuery("")
        account.holdSearches.withLock { $0 = false }
        account.searchEntries.send(())
        await first

        #expect(!account.log.actions.contains(.searchThreads(id(11), "t2")))
        #expect(browser.results == nil)
        session.close()
    }

    // MARK: Second review

    // The pages stay for the connection: opening again asks for nothing.
    @Test
    func reopeningWithAPageLoadedAsksNothing() async {
        let session = inChannel11()
        browse(archived: [34], older: .more)
        let browser = browser(session)

        await browser.open()

        #expect(loads() == 0)
        #expect(browser.sections.map(\.kind) == [.older])
        session.close()
    }

    // MARK: Live

    @Test
    func threadActivityForItsChannelRereadsTheBrowser() {
        let session = inChannel11()
        browse(others: [32, 33])
        let browser = browser(session)

        browse(others: [33, 32])
        session.apply(EventBatch([.threadActivity(guildId: id(1), parentId: id(11), threadId: id(33))]))

        #expect(browser.sections.first?.rows.map(\.id) == [id(33), id(32)])
        session.close()
    }

    @Test
    func activityElsewhereDoesntRereadIt() {
        let session = inChannel11()
        browse(others: [32])
        _ = browser(session)
        store.log.forget()

        session.apply(EventBatch([.threadActivity(guildId: id(1), parentId: id(12), threadId: id(39))]))

        #expect(!store.reads.contains(.threadBrowser(id(11))))
        session.close()
    }

    @Test
    func aSearchsRowsFollowTheirThreads() async {
        let session = inChannel11()
        account.searchReply.withLock { $0 = .success([id(31)]) }
        let browser = browser(session)
        browser.setQuery("t1")
        await browser.submit()

        store.update { $0.channels[id(31)]?.name = "renamed" }
        session.apply(EventBatch([.channelUpdated(channelId: id(31), guildId: id(1))]))

        #expect(browser.results?.first?.name == "renamed")
        session.close()
    }
}

// The session's part: which channel the header's button browses, the two containers, a row's
// click, and the channel list's hover popout.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ThreadBrowserSessionTests {
    let store = FakeStore()
    let account: FakeAccount
    let memory = AccountMemory(defaults: MemoryDefaults())

    init() {
        account = FakeAccount(store: store)
    }

    func world() {
        var threads = (31...35).map { thread(UInt64($0), in: 11, name: "t\($0 - 30)") }
        for index in threads.indices {
            threads[index].thread?.joined = false
            threads[index].thread?.lastMessageId = id(100 + UInt64(index))
        }
        // The listed thread is the newest, so leaving it out shows.
        threads[0].thread?.joined = true
        threads[0].thread?.lastMessageId = id(200)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list(
                [channel(11), threads[0], channel(12), channel(13, kind: .guildForum),
                 channel(14, kind: .guildVoice), channel(15, kind: .guildNews)],
                in: 1)
            for thread in threads {
                state.channels[thread.id] = thread
            }
            state.channels[id(36)] = thread(36, in: 13, name: "post")
            state.channels[id(37)] = thread(37, in: 15, name: "news thread")
            for raw in [11, 12, 15, 31, 32, 33, 34, 35, 37] as [UInt64] {
                state.windows[id(raw)] = window([raw + 1000])
                state.show(message(raw + 1000, in: raw))
            }
            state.permissions[id(11)] = [.viewChannel, .readMessageHistory]
            state.threadBrowsers[id(11)] = ThreadBrowser(
                joined: [id(31)], others: [id(35), id(34), id(33), id(32)], archived: [],
                older: .notLoaded)
        }
    }

    func inChannel11() -> SessionModel {
        world()
        let session = SessionModel(userId: id(1), account: account, memory: memory) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    @Test
    func theHeadersButtonBrowsesTextAndAnnouncementChannelsAndAThreadsParent() {
        let session = inChannel11()
        #expect(session.threadsButtonParent == id(11))

        session.open(channel: id(15))
        #expect(session.threadsButtonParent == id(15))

        session.go(to: id(32))
        #expect(session.threadsButtonParent == id(11))

        session.go(to: id(36))
        #expect(session.threadsButtonParent == nil)
        session.close()
    }

    @Test
    func theBrowserOpensAsAskedAndCloses() {
        let session = inChannel11()

        session.openThreadBrowser(of: id(11), as: .modal)
        #expect(session.threadBrowser?.presentation == .modal)
        #expect(session.threadBrowser?.parentId == id(11))

        session.closeThreadBrowser()
        #expect(session.threadBrowser == nil)
        session.close()
    }

    @Test
    func goingElsewhereClosesThePopout() {
        let session = inChannel11()
        session.openThreadBrowser(of: id(11), as: .popout)

        session.open(channel: id(12))

        #expect(session.threadBrowser == nil)
        session.close()
    }

    @Test
    func aPlainClickOpensTheThreadFullWidthAndClosesTheParentsSplit() async {
        let session = inChannel11()
        await session.openBeside(thread: id(31), from: id(11))
        #expect(session.split?.channelId == id(31))
        session.openThreadBrowser(of: id(11), as: .popout)

        await session.open(browsed: id(33), beside: false)

        #expect(session.threadBrowser == nil)
        #expect(session.messages?.channelId == id(33))
        session.open(channel: id(11))
        #expect(session.split == nil)
        session.close()
    }

    @Test
    func aShiftClickOpensTheThreadBeside() async {
        let session = inChannel11()
        session.openThreadBrowser(of: id(11), as: .modal)

        await session.open(browsed: id(33), beside: true)

        #expect(session.threadBrowser == nil)
        #expect(session.messages?.channelId == id(11))
        #expect(session.split?.channelId == id(33))
        session.close()
    }

    @Test
    func aShiftClickInAThreadsFullViewGoesToTheParentFirst() async {
        let session = inChannel11()
        session.go(to: id(32))
        session.openThreadBrowser(of: id(11), as: .popout)

        await session.open(browsed: id(34), beside: true)

        #expect(session.messages?.channelId == id(11))
        #expect(session.split?.channelId == id(34))
        session.close()
    }

    @Test
    func aBrowsedRowsMenuOffersJoiningAThreadTheSidebarDoesntList() {
        let session = inChannel11()
        session.openThreadBrowser(of: id(11), as: .popout)

        #expect(
            session.membershipItem(for: id(33))
                == SessionModel.MembershipItem(joins: true, enabled: true))
        session.close()
    }

    @Test
    func theCoreHearsWhichChannelsBrowserIsOpen() async {
        let session = inChannel11()

        session.openThreadBrowser(of: id(11), as: .popout)
        session.openThreadBrowser(of: id(11), as: .modal)
        session.closeThreadBrowser()
        session.openThreadBrowser(of: id(11), as: .popout)
        await session.open(browsed: id(33), beside: false)

        let browsing = account.log.actions.filter {
            if case .setBrowsing = $0 { true } else { false }
        }
        #expect(browsing == [
            .setBrowsing(id(11)), .setBrowsing(nil), .setBrowsing(id(11)), .setBrowsing(nil),
        ])
        session.close()
    }

    // Shown, the popout follows new messages as the browser does.
    @Test
    func thePeekedThreadsFollowTheStore() {
        let session = inChannel11()
        session.peek(at: id(11))
        #expect(session.peekedThreads.map(\.id) == [id(35), id(34), id(33)])

        store.update { state in
            state.channels[id(32)]?.thread?.lastMessageId = id(300)
        }
        session.apply(EventBatch([.threadActivity(guildId: id(1), parentId: id(11), threadId: id(32))]))
        #expect(session.peekedThreads.map(\.id) == [id(32), id(35), id(34)])

        session.peek(at: nil)
        #expect(session.peekedThreads.isEmpty)
        session.close()
    }

    @Test
    func moreActiveThreadsAreThoseTheSidebarDoesntShowNewestFirstAtMostThree() {
        let session = inChannel11()

        let peeks = session.moreActiveThreads(of: id(11))

        #expect(peeks.map(\.id) == [id(35), id(34), id(33)])
        #expect(peeks.first?.name == "t5")
        #expect(peeks.first?.time == ThreadBox.time(of: 104))
        #expect(session.moreActiveThreads(of: id(12)).isEmpty)
        session.close()
    }
}
