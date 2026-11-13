import AkariFFI
import Foundation
import Testing

@testable import AkariKit

// Guild 1 with text channel 11 and forum 13, whose posts are 41 and 42, each with a window at
// the present. The session is online, active, key and visible, on the forum's page.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ForumSplitTests {
    let store = FakeStore()
    let account: FakeAccount
    let memory = AccountMemory(defaults: MemoryDefaults())
    let forumId: ChannelId = id(13)

    init() {
        account = FakeAccount(store: store)
        let posts = [41, 42].map { raw -> Channel in
            var post = thread(UInt64(raw), in: 13, name: "post \(raw)")
            post.thread?.joined = false
            return post
        }
        let listed = ForumPosts(active: posts.map(\.id), archived: [], older: .all, newPosts: 0)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(11), channel(13, kind: .guildForum, name: "ideas")], in: 1)
            for post in posts {
                state.channels[post.id] = post
            }
            state.permissions[id(13)] = [.viewChannel, .readMessageHistory]
            state.forumPages[id(13)] = { _ in listed }
            state.windows[id(11)] = window([1])
            state.windows[id(41)] = window([4])
            state.windows[id(42)] = window([5])
            state.show(message(1, in: 11), message(4, in: 41), message(5, in: 42))
        }
    }

    var readings: [[ChannelId]] { account.readingCalls.current }

    func makeSession() -> SessionModel {
        SessionModel(userId: id(1), account: account, memory: memory) { _ in }
    }

    func inForum() -> SessionModel {
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: forumId)
        session.setAppActive(true)
        session.setWindowKey(true)
        session.setWindowVisible(true)
        return session
    }

    @Test
    func aCardOpensItsPostBesideTheForum() async {
        let session = inForum()

        await session.openBeside(thread: id(41), from: forumId)

        #expect(session.forum?.forumId == forumId, "the page stays")
        #expect(session.messages == nil)
        #expect(session.split?.channelId == id(41))
        #expect(session.splitChannel?.id == id(41))
        #expect(session.focusSplit, "its composer takes the focus")
        session.close()
    }

    // A forum's page is an open list too, with the post beside it.
    @Test
    func theOpenListsNameTheForumAndItsPost() async {
        let session = inForum()
        #expect(account.openCalls.current.last == [forumId])

        await session.openBeside(thread: id(41), from: forumId)
        #expect(account.openCalls.current.last == [forumId, id(41)])

        session.open(channel: id(11))
        #expect(account.openCalls.current.last == [id(11)])
        session.close()
        let opens = account.openCalls.current
        #expect(opens.last == [])
        #expect(zip(opens, opens.dropFirst()).allSatisfy { $0 != $1 }, "told once per change")
    }

    // The core ends a mark's hold on a channel a report leaves out.
    @Test
    func aPostMarkedUnreadBesideTheForumStaysHeldInFullView() async throws {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)
        _ = try await account.markUnread(channelId: id(41), messageId: id(4))
        #expect(account.held.current == [id(41)])
        let told = account.openCalls.current.count

        session.openInFullView()

        #expect(session.messages?.channelId == id(41) && session.forum == nil && session.split == nil)
        #expect(Array(account.openCalls.current.dropFirst(told)) == [[id(41)]])
        #expect(account.held.current == [id(41)])
        session.close()
    }

    // The core stops reading a channel that leaves the open ones: it hears of those first.
    @Test
    func aNavigationNamesTheOpenListsBeforeTheOnesBeingRead() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)
        let told = account.listCalls.current.count

        session.openInFullView()

        #expect(readings.last == [id(41)])
        #expect(Array(account.listCalls.current.dropFirst(told)) == ["open", "reading"])
        session.close()
    }

    enum Navigation: CaseIterable {
        case fullView, splitView, anotherChannel, goToAnotherChannel, closeSplit, openBeside
        case anotherPost, forumAgain, home, backToTheServer, backToTheForum, goFromHome
        case goToTheForumFromHome, relaunch, parentRemoved, postRemoved, close
    }

    // Whatever a navigation does on the way, the core hears of the open lists once, as they
    // are after it.
    @Test(arguments: Navigation.allCases)
    func aNavigationNamesTheOpenListsOnce(_ navigation: Navigation) async {
        var session = inForum()
        await session.openBeside(thread: id(41), from: forumId)
        switch navigation {
        case .splitView: session.openInFullView()
        case .openBeside: session.closeSplit()
        case .backToTheServer, .goFromHome, .goToTheForumFromHome: session.open(.home)
        case .backToTheForum: session.open(channel: id(11))
        case .relaunch: session.close()
        default: break
        }
        let told = account.openCalls.current.count

        let after: [ChannelId]
        switch navigation {
        case .fullView:
            session.openInFullView()
            after = [id(41)]
        case .splitView:
            session.openInSplitView()
            after = [forumId, id(41)]
        case .anotherChannel:
            session.open(channel: id(11))
            after = [id(11)]
        case .goToAnotherChannel:
            session.go(to: id(11))
            after = [id(11)]
        case .closeSplit:
            session.closeSplit()
            after = [forumId]
        case .openBeside:
            await session.openBeside(thread: id(41), from: forumId)
            after = [forumId, id(41)]
        case .anotherPost:
            await session.openBeside(thread: id(42), from: forumId)
            after = [forumId, id(42)]
        case .forumAgain:
            session.open(channel: forumId)
            after = [forumId]
        case .home:
            session.open(.home)
            after = []
        case .backToTheServer:
            session.open(.guild(id(1)))
            after = [forumId, id(41)]
        case .backToTheForum:
            session.go(to: forumId)
            after = [forumId, id(41)]
        case .goFromHome:
            session.go(to: id(42))
            after = [id(42)]
        case .goToTheForumFromHome:
            session.go(to: forumId)
            after = [forumId, id(41)]
        case .relaunch:
            session = makeSession()
            session.start()
            after = [forumId, id(41)]
        case .parentRemoved:
            store.update { state in
                state.channels[id(13)] = nil
                state.list([channel(11)], in: 1)
            }
            session.apply(EventBatch([.channelRemoved(channelId: forumId, guildId: id(1))]))
            after = [id(11)]
        case .postRemoved:
            store.update { $0.channels[id(41)] = nil }
            session.apply(EventBatch([.channelRemoved(channelId: id(41), guildId: id(1))]))
            after = [forumId]
        case .close:
            session.close()
            after = []
        }

        #expect(Array(account.openCalls.current.dropFirst(told)) == [after], "\(navigation)")
        #expect([session.openChannelId, session.split?.channelId].compactMap { $0 } == after || navigation == .close)
        session.close()
    }

    @Test
    func anotherCardReplacesThePost() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        await session.openBeside(thread: id(42), from: forumId)

        #expect(session.split?.channelId == id(42))
        #expect(session.forum?.forumId == forumId)
        session.close()
    }

    @Test
    func theForumKeepsItsPostWhileTheUserIsElsewhere() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        session.open(channel: id(11))
        #expect(session.split == nil)
        session.go(to: forumId)

        #expect(session.forum?.forumId == forumId)
        #expect(session.split?.channelId == id(41))
        session.close()
    }

    @Test
    func aRelaunchOnTheForumShowsItsPostAgain() async {
        let first = inForum()
        await first.openBeside(thread: id(41), from: forumId)
        first.close()

        let session = makeSession()
        session.start()

        #expect(session.forum?.forumId == forumId)
        #expect(session.split?.channelId == id(41))
        session.close()
    }

    // The store has the forum before it has the user: only the forum's page says a list opened.
    @Test
    func aForumRestoredBeforeTheUserIsKnownNamesItsPage() {
        inForum().close()
        store.update { $0.currentUser = nil }
        let told = account.openCalls.current.count

        let session = makeSession()
        session.start()

        #expect(session.forum?.forumId == forumId && session.split == nil)
        #expect(Array(account.openCalls.current.dropFirst(told)) == [[forumId]])
        session.close()
    }

    // As in the official client: clicking the selected forum again forgets its post.
    @Test
    func theOpenForumsRowClosesItsPost() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        session.open(channel: forumId)

        #expect(session.split == nil)
        #expect(session.forum?.forumId == forumId)
        session.open(channel: id(11))
        session.open(channel: forumId)
        #expect(session.split == nil, "forgotten")
        session.close()
    }

    @Test
    func theXClosesThePostAndTheForumForgetsIt() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        session.closeSplit()

        #expect(session.split == nil)
        #expect(!session.focusChannel, "a forum's page has no composer to focus")
        session.open(channel: id(11))
        session.go(to: forumId)
        #expect(session.split == nil)
        session.close()
    }

    @Test
    func openInFullViewShowsThePostAloneAndTheForumKeepsIt() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        session.openInFullView()

        #expect(session.messages?.channelId == id(41))
        #expect(session.forum == nil)
        #expect(session.split == nil)
        session.go(to: forumId)
        #expect(session.split?.channelId == id(41))
        session.close()
    }

    @Test
    func openInSplitViewOpensTheForumWithThePostBesideIt() {
        let session = inForum()
        session.go(to: id(41))
        #expect(session.messages?.channelId == id(41))

        session.openInSplitView()

        #expect(session.forum?.forumId == forumId)
        #expect(session.messages == nil)
        #expect(session.split?.channelId == id(41))
        #expect(session.focusSplit)
        session.close()
    }

    @Test
    func escapeWithNothingLeftToDoClosesThePost() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        #expect(!session.escape())

        #expect(session.split == nil)
        #expect(session.forum?.forumId == forumId)
        session.close()
    }

    // The forum is acked at "now" and the post like a thread: both are read.
    @Test
    func theForumAndThePostBesideItAreBothRead() async {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)
        let split = session.split
        await split?.open()
        split?.setAtBottom(true)
        await finished(session.pendingReading)

        #expect(session.reading == [forumId, id(41)])
        #expect(Set(readings.last ?? []) == [forumId, id(41)])
        session.close()
    }

    @Test
    func aPostOfAnotherForumDoesNotOpenBesideThisOne() async {
        store.update { state in
            var other = thread(51, in: 14, name: "elsewhere")
            other.thread?.joined = false
            state.channels[id(51)] = other
        }
        let session = inForum()

        await session.openBeside(thread: id(51), from: forumId)

        #expect(session.split == nil)
        session.close()
    }

    // Discord's client names the bar as the place: "Forum Toolbar".
    @Test
    func followingFromTheBarJoinsAndLeavesThePost() async throws {
        let session = inForum()
        await session.openBeside(thread: id(41), from: forumId)

        await session.setFollowing(true, post: id(41))
        #expect(account.log.actions.last == .joinThread(id(41), .forumToolbar))
        store.subscription.send(.channelUpdated(channelId: id(41), guildId: id(1)))
        try await eventually { session.split?.postBar?.following == true }

        await session.setFollowing(false, post: id(41))
        #expect(account.log.actions.last == .leaveThread(id(41), .forumToolbar))
        session.close()
    }

    // Review: nothing is remembered for a post that can't open here.
    @Test
    func aPostOfAnotherForumIsNotRemembered() async {
        store.update { state in
            var other = thread(51, in: 14, name: "elsewhere")
            other.thread?.joined = false
            state.channels[id(51)] = other
        }
        let session = inForum()

        await session.openBeside(thread: id(51), from: forumId)

        #expect(memory.splits(of: id(1)).isEmpty)
        session.close()
    }

    // A card's post the store doesn't have is fetched first: the answer opens nothing once the
    // user left the forum, also when they are back on it.
    @Test
    func aFetchedPostOpensNothingOnALaterVisit() async throws {
        var archived = thread(43, in: 13, name: "old post")
        archived.thread?.joined = false
        account.fetchedThreads.withLock { $0[id(43)] = archived }
        account.holdThreadFetches.withLock { $0 = true }
        let session = inForum()

        let opening = Task { await session.openBeside(thread: id(43), from: self.forumId) }
        try await eventually { self.account.log.actions.contains(.fetchThread(self.forumId, id(43))) }
        session.open(channel: id(11))
        session.open(channel: forumId)
        account.threadFetches.send(())
        await opening.value

        #expect(session.split == nil)
        session.close()
    }
}
