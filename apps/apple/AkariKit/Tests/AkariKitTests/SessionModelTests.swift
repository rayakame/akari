import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let closes = Locked<[GatewayError?]>([])
    let defaults = MemoryDefaults()
    let memory: AccountMemory

    init() {
        account = FakeAccount(store: store)
        memory = AccountMemory(defaults: defaults)
    }

    var subscription: FakeSubscription { store.subscription }

    func makeSession() -> SessionModel {
        SessionModel(userId: id(1), account: account, memory: memory) { [closes] error in
            closes.withLock { $0.append(error) }
        }
    }

    // Where the memory that forgets the account keeps them, not the shared defaults.
    @Test
    func theSessionsTrustedLinksLiveWithItsMemory() throws {
        let session = makeSession()
        let url = try #require(URL(string: "https://example.com/"))

        session.trustedLinks().trust(url)

        #expect(TrustedLinks(account: session.userId, defaults: defaults).isTrusted(url))
        memory.forget(session.userId)
        #expect(!session.trustedLinks().isTrusted(url))
    }

    // Discord's clients collapse a category only when it's muted in that same client.
    @Test
    func aCategoryMutedElsewhereStaysAsTheUserLeftIt() {
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1))
            state.list([channel(10, kind: .guildCategory), channel(11, parent: 10)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        store.update { $0.mutedChannels = [id(10)] }

        session.apply(EventBatch([.guildSettingsChanged(guildId: id(1))]))

        #expect(!session.collapsed.isCollapsed(id(10), in: id(1)))
        #expect(session.channels?.muted == [id(10)])
        session.close()
    }

    @Test
    func startSubscribesBeforeReadingThenConnects() {
        store.update { state in
            state.connection = .connecting
            state.currentUser = user(1, name: "me")
            state.add(guild(5))
        }
        let session = makeSession()

        session.start()

        let calls = store.log.calls
        #expect(calls.first == .subscribe)
        #expect(calls.last == .connect)
        #expect(calls.contains(.read(.connection)))
        #expect(calls.contains(.read(.currentUser)))
        #expect(calls.contains(.read(.serverList)))
        #expect(session.connection == .connecting)
        #expect(session.currentUser == user(1, name: "me"))
        #expect(session.servers.guilds == [guild(5)])
        session.close()
    }

    @Test
    func batchesApplyInOrderOnTheMainActor() async {
        store.update { state in
            for raw in 1...50 {
                state.add(guild(UInt64(raw)))
            }
        }
        let session = makeSession()
        session.start()
        store.log.forget()

        for raw in 1...50 {
            subscription.send(
                .guildUpdated(guildId: id(UInt64(raw))),
                .connection(state: raw == 50 ? .online : .connecting)
            )
        }
        await subscription.batches.pulled(51)

        #expect(store.reads == (1...50).map { .guild(id(UInt64($0))) })
        #expect(store.log.readsOffTheMainThread == 0)
        #expect(session.connection == .online)
        session.close()
    }

    // A bulk "mark as read" elsewhere: hundreds of acks at once.
    @Test
    func aBurstOfBadgeEventsIsOneReadPerList() async {
        let channels = (1...254).map { channel(UInt64(1000 + $0)) }
        store.update { state in
            state.add(guild(1))
            state.list(channels, in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        store.log.forget()

        subscription.batches.send(
            channels.map { .badgeChanged(channelId: $0.id, guildId: id(1)) }
                + [.guildBadgeChanged(guildId: id(1))])
        await subscription.batches.pulled(2)

        let badgeReads = store.reads.filter { read in
            switch read {
            case .channelBadges, .guildBadges: true
            default: false
            }
        }
        #expect(badgeReads == [.guildBadges([id(1)]), .channelBadges(channels.map(\.id))])
        session.close()
    }

    @Test
    func cancellingTheSessionClosesTheSubscription() async {
        let session = makeSession()
        session.start()
        await subscription.batches.pulled(1)

        session.close()
        await finished(session.loop.running)

        #expect(store.log.calls.contains(.close))
        #expect(subscription.closed.fired)
        #expect(closes.current.isEmpty)
    }

    @Test
    func releasingTheSessionEndsItsLoop() async {
        var session: SessionModel? = makeSession()
        session?.start()
        await subscription.batches.pulled(1)

        session = nil

        await subscription.closed.wait()
    }

    @Test
    func aClosedAccountReportsOnce() async {
        let session = makeSession()
        session.start()

        subscription.send(.connection(state: .closed(error: .AuthenticationFailed)))
        subscription.send(.connection(state: .closed(error: .AuthenticationFailed)))
        subscription.end()
        await finished(session.loop.running)

        #expect(closes.current == [.AuthenticationFailed])
        #expect(session.connection == .closed(error: .AuthenticationFailed))
    }

    @Test
    func closingAfterAnAuthenticationFailureKeepsTheError() async {
        let session = makeSession()
        session.start()
        subscription.send(.connection(state: .closed(error: .AuthenticationFailed)))
        await subscription.batches.pulled(2)

        session.close()
        await finished(session.loop.running)

        #expect(session.connection == .closed(error: .AuthenticationFailed))
    }

    @Test
    func aFailedConnectClosesTheSession() {
        account.connectError.withLock { $0 = .Closed }
        let session = makeSession()

        session.start()

        #expect(session.connection == .closed(error: .Closed))
        #expect(closes.current == [.Closed])
        #expect(subscription.closed.fired)
    }

    @Test
    func openingAGuildOpensItsLastChannelOrItsFirstTextChannel() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list(
                [
                    channel(10, kind: .guildCategory), channel(11, kind: .guildForum),
                    channel(12, kind: .guildVoice), channel(13), channel(14, kind: .guildNews),
                ],
                in: 1
            )
            state.list([channel(20, guild: 2, kind: .guildNews), channel(21, guild: 2)], in: 2)
        }
        let session = makeSession()
        session.start()

        session.open(.guild(id(1)))
        #expect(session.place == .guild(id(1)))
        #expect(session.channels?.guildId == id(1))
        #expect(session.messages?.channelId == id(13))

        session.open(channel: id(14))
        session.open(.guild(id(2)))
        #expect(session.messages?.channelId == id(20))

        session.open(.guild(id(1)))
        #expect(session.messages?.channelId == id(14))

        let channels = session.channels
        let messages = session.messages
        session.open(.guild(id(1)))
        session.open(channel: id(14))
        #expect(session.channels === channels)
        #expect(session.messages === messages)

        session.open(.home)
        #expect(session.place == .home)
        #expect(session.channels == nil)
        #expect(session.messages == nil)
        session.close()
    }

    @Test
    func suspendAndResumeKeepTheSession() {
        let session = makeSession()
        session.start()
        store.log.forget()

        session.suspend()
        session.resume()

        #expect(store.log.calls == [.disconnect, .connect])
        #expect(closes.current.isEmpty)
        session.close()
    }

    @Test
    func theNoticeFollowsTheConnection() async {
        let session = makeSession()
        #expect(session.notice == .connecting)
        session.start()

        subscription.send(.connection(state: .connecting))
        await subscription.batches.pulled(2)
        #expect(session.notice == .connecting)
        subscription.send(.connection(state: .online))
        await subscription.batches.pulled(3)
        #expect(session.notice == nil)
        subscription.send(.connection(state: .connecting))
        await subscription.batches.pulled(4)
        #expect(session.notice == .reconnecting)
        session.suspend()
        subscription.send(.connection(state: .offline))
        await subscription.batches.pulled(5)
        #expect(session.notice == .offline)
        session.resume()
        subscription.send(.connection(state: .connecting))
        await subscription.batches.pulled(6)
        #expect(session.notice == .reconnecting)
        subscription.send(.connection(state: .closed(error: .Stopped)))
        await subscription.batches.pulled(7)
        #expect(session.notice == .closed(.Stopped))
        session.close()
    }

    @Test
    func aDeletedOrHiddenOpenChannelOpensAnother() async {
        store.update { state in
            state.add(guild(1))
            state.list([channel(13), channel(14), channel(15)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        store.update { state in
            state.channelLists[id(1)] = [id(14), id(15)]
            state.channels[id(13)] = nil
        }
        subscription.send(.channelRemoved(channelId: id(13), guildId: id(1)))
        await subscription.batches.pulled(2)
        #expect(session.messages?.channelId == id(14))

        session.open(channel: id(15))
        store.update { $0.channelLists[id(1)] = [id(14)] }
        subscription.send(.currentMemberUpdated(guildId: id(1)))
        await subscription.batches.pulled(3)
        #expect(session.messages?.channelId == id(14))

        store.update { $0.channelLists[id(1)] = [] }
        subscription.send(.channelUpdated(channelId: id(14), guildId: id(1)))
        await subscription.batches.pulled(4)
        #expect(session.messages == nil)
        session.close()
    }

    // A channel link or mention can name a channel of another guild, or a conversation.
    @Test
    func goingToAChannelOpensItWhereItLives() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(13)], in: 1)
            state.list([channel(20, guild: 2), channel(21, guild: 2)], in: 2)
            state.channels[id(50)] = dm(50, with: 7)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        session.go(to: id(21))
        #expect(session.place == .guild(id(2)))
        #expect(session.channels?.guildId == id(2))
        #expect(session.messages?.channelId == id(21))

        session.go(to: id(50))
        #expect(session.place == .home)
        #expect(session.messages?.channelId == id(50))

        session.open(.guild(id(2)))
        #expect(session.messages?.channelId == id(21))
        session.close()
    }

    // As in the sidebar: forums, voice channels and categories have no message list yet.
    @Test
    func goingToAChannelWithoutAMessageListDoesNothing() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(13)], in: 1)
            state.list([channel(20, guild: 2, kind: .guildForum), channel(21, guild: 2)], in: 2)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        session.go(to: id(20))

        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(13))
        session.close()
    }

    @Test
    func goingToAChannelTheStoreDoesntHaveStaysInPlace() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(13)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        session.go(to: id(99))

        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(99))
        session.close()
    }

    @Test
    func enteringAGuildOpensItsFirstChannelNotAThread() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(10, kind: .guildForum), thread(31, in: 10), channel(11)], in: 1)
        }
        let session = makeSession()
        session.start()

        session.open(.guild(id(1)))

        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    @Test
    func anOpenThreadThatStopsBeingListedStaysOpen() async {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), thread(31, in: 11)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(31))

        store.update { $0.channelLists[id(1)] = [id(11)] }
        subscription.send(.threadListingChanged(guildId: id(1), threadId: id(31)))
        await subscription.batches.pulled(2)

        #expect(session.messages?.channelId == id(31))
        #expect(session.channels?.shown(open: id(31)).map(\.id) == [id(11), id(31)])
        session.open(channel: id(11))
        #expect(session.channels?.shown(open: id(11)).map(\.id) == [id(11)])
        session.close()
    }

    @Test
    func theParentInTheHeaderOpensIt() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), thread(31, in: 11)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(31))

        session.openParent()

        #expect(session.messages?.channelId == id(11))
        session.openParent()
        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    // A "started a thread" line in channel 11 names thread 32, which the store doesn't have.
    func inChannel11(besides others: [Channel] = []) -> SessionModel {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)] + others, in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    @Test
    func aLineOpensAThreadTheStoreHasWithoutFetchingIt() async {
        let session = inChannel11(besides: [thread(31, in: 11)])

        await session.open(thread: id(31), from: id(11))

        #expect(session.messages?.channelId == id(31))
        #expect(!store.log.calls.contains(.fetchThread(id(11), id(31))))
        session.close()
    }

    @Test
    func aLineFetchesAThreadTheStoreDoesntHaveAndOpensIt() async {
        let session = inChannel11()
        account.fetchedThreads.withLock { $0[id(32)] = thread(32, in: 11) }

        await session.open(thread: id(32), from: id(11))

        #expect(store.log.calls.contains(.fetchThread(id(11), id(32))))
        #expect(session.messages?.channelId == id(32))
        session.close()
    }

    @Test
    func aThreadDiscordRefusesDoesntOpen() async {
        let session = inChannel11()

        await session.open(thread: id(32), from: id(11))

        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    @Test
    func aThreadThatFailsToFetchDoesntOpen() async {
        let session = inChannel11()
        account.fetchedThreads.withLock { $0[id(32)] = thread(32, in: 11) }
        account.threadFetchError.withLock { $0 = .Network(kind: .connect) }

        await session.open(thread: id(32), from: id(11))

        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    @Test
    func aThreadFetchedAfterTheUserMovedOnDoesntOpen() async {
        let session = inChannel11(besides: [channel(12)])
        account.fetchedThreads.withLock { $0[id(32)] = thread(32, in: 11) }
        account.holdThreadFetches.withLock { $0 = true }

        let opening = Task { await session.open(thread: id(32), from: id(11)) }
        await account.threadFetches.pulled(1)
        session.open(channel: id(12))
        account.threadFetches.send(())
        await opening.value

        #expect(session.messages?.channelId == id(12))
        session.close()
    }

    @Test
    func aSecondClickWhileTheThreadIsFetchedAsksOnce() async {
        let session = inChannel11()
        account.fetchedThreads.withLock { $0[id(32)] = thread(32, in: 11) }
        account.holdThreadFetches.withLock { $0 = true }

        let first = Task { await session.open(thread: id(32), from: id(11)) }
        await account.threadFetches.pulled(1)
        await session.open(thread: id(32), from: id(11))
        account.threadFetches.send(())
        await first.value

        #expect(store.log.calls.filter { $0 == .fetchThread(id(11), id(32)) }.count == 1)
        #expect(session.messages?.channelId == id(32))
        session.close()
    }

    @Test
    func aRemovedOpenThreadCloses() async {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), thread(31, in: 11)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(31))

        store.update { state in
            state.channelLists[id(1)] = [id(11)]
            state.channels[id(31)] = nil
        }
        subscription.send(.channelRemoved(channelId: id(31), guildId: id(1)))
        await subscription.batches.pulled(2)

        #expect(session.messages?.channelId == id(11))
        session.close()
    }

    @Test
    func aChannelOutsideTheListStaysOpenUntilRemoved() async {
        store.update { state in
            state.add(guild(1))
            state.list([channel(13)], in: 1)
            state.channels[id(50)] = channel(50, guild: nil, kind: .dm)
            state.channels[id(90)] = channel(90, kind: .publicThread)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(90))

        subscription.send(.channelUpdated(channelId: id(13), guildId: id(1)))
        await subscription.batches.pulled(2)
        #expect(session.messages?.channelId == id(90))

        session.open(.home)
        session.open(channel: id(50))
        subscription.send(.channelRemoved(channelId: id(50), guildId: nil))
        await subscription.batches.pulled(3)
        #expect(session.messages == nil)
        session.close()
    }

    @Test
    func aRemovedGuildFallsBackHome() async {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(13)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        store.update { state in
            state.guildIds = [id(1)]
            state.guilds[id(2)] = nil
        }
        subscription.send(.guildRemoved(guildId: id(2)))
        await subscription.batches.pulled(2)
        #expect(session.place == .guild(id(1)))

        store.update { state in
            state.guildIds = []
            state.guilds[id(1)] = nil
        }
        subscription.send(.guildUnavailable(guildId: id(1)))
        await subscription.batches.pulled(3)

        #expect(session.place == .home)
        #expect(session.channels == nil)
        #expect(session.messages == nil)
        #expect(session.servers.guilds.isEmpty)
        session.close()
    }

    @Test
    func draftsSurviveChannelSwitches() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(12), channel(13)], in: 1)
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(12))

        session.messages?.composer.draft = "half"
        session.open(channel: id(13))
        #expect(session.messages?.composer.draft == "")
        session.open(channel: id(12))

        #expect(session.messages?.composer.draft == "half")
        let next = SessionModel(
            userId: id(1), account: FakeAccount(store: store), memory: memory,
            drafts: session.drafts
        ) { _ in }
        next.open(.guild(id(1)))
        next.open(channel: id(12))
        #expect(next.messages?.composer.draft == "half")
        session.close()
    }

    @Test
    func homeReopensTheLastConversation() {
        store.update { state in
            state.add(guild(1))
            state.list([channel(13)], in: 1)
            state.talk(dm(5, with: 50), dm(6, with: 60), with: user(50), user(60))
        }
        let session = makeSession()
        session.start()

        session.open(channel: id(5))
        session.open(.guild(id(1)))
        #expect(session.messages?.channelId == id(13))
        session.open(.home)

        #expect(session.messages?.channelId == id(5))
        #expect(session.directMessages.conversations.map(\.id) == [id(5), id(6)])
        session.close()
    }

    @Test
    func aRemovedConversationIsntReopened() async {
        store.update { state in
            state.add(guild(1))
            state.list([channel(13)], in: 1)
            state.talk(dm(5, with: 50), with: user(50))
        }
        let session = makeSession()
        session.start()
        session.open(channel: id(5))
        session.open(.guild(id(1)))

        store.update { state in
            state.privateChannels = []
            state.channels[id(5)] = nil
        }
        subscription.send(.channelRemoved(channelId: id(5), guildId: nil))
        await subscription.batches.pulled(2)
        session.open(.home)

        #expect(session.messages == nil)
        session.close()
    }

    @Test
    func theLastPlaceReopensAfterTheFirstReady() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(11), channel(12)], in: 1)
            state.list([channel(21, guild: 2)], in: 2)
        }
        let session = makeSession()
        session.start()
        #expect(session.place == .home)

        subscription.send(.ready)
        await subscription.batches.pulled(2)
        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(12))

        session.open(.guild(id(2)))
        subscription.send(.ready)
        await subscription.batches.pulled(3)
        #expect(session.place == .guild(id(2)))
        #expect(session.messages?.channelId == id(21))
        session.close()
    }

    @Test
    func aConversationReopensAtHomeAfterTheFirstReady() async {
        memory.remember(.init(place: .home, channel: id(5)), of: id(1))
        store.update { $0.talk(dm(5, with: 50), with: user(50)) }
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages?.channelId == id(5))
        session.close()
    }

    @Test
    func theLastChannelStartsLoadingBeforeReady() async throws {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
        }
        let session = makeSession()
        session.start()

        let early = try #require(session.messages)
        #expect(early.channelId == id(12))
        #expect(session.place == .home)
        await early.open()
        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .guild(id(1)))
        #expect(session.messages === early)
        let loads = account.log.actions.filter { if case .load = $0 { true } else { false } }
        #expect(loads.count == 1)
        session.close()
    }

    @Test
    func aPlaceOpenedBeforeReadyWinsOverTheLastOne() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(11), channel(12)], in: 1)
            state.list([channel(21, guild: 2), channel(22, guild: 2)], in: 2)
        }
        let session = makeSession()
        session.start()

        session.open(.guild(id(2)))
        session.open(channel: id(22))
        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .guild(id(2)))
        #expect(session.messages?.channelId == id(22))
        session.close()
    }

    @Test
    func homeChosenBeforeReadyWinsOverALastServer() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
        }
        let session = makeSession()
        session.start()

        session.open(.home)
        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages == nil)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: nil))
        session.close()
    }

    @Test
    func aGoneGuildFallsBackHome() async {
        memory.remember(.init(place: .guild(id(9)), channel: id(90)), of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages == nil)
        // The next launch mustn't load the gone channel again.
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: nil))
        session.close()
    }

    @Test
    func aGoneConversationLeavesNothingOpen() async {
        memory.remember(.init(place: .home, channel: id(5)), of: id(1))
        let session = makeSession()
        session.start()
        #expect(session.messages?.channelId == id(5))

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages == nil)
        // The next launch mustn't load the gone channel again.
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: nil))
        session.close()
    }

    @Test
    func aGoneChannelFallsBackToTheFirstTextChannel() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(99)), of: id(1))
        store.update { state in
            state.add(guild(1))
            state.list([channel(11, kind: .guildVoice), channel(12)], in: 1)
        }
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(12))
        session.close()
    }

    @Test
    func openingRemembersThePlace() throws {
        store.update { state in
            state.add(guild(1))
            state.list([channel(11), channel(12)], in: 1)
        }
        let session = makeSession()
        session.start()

        session.open(.guild(id(1)))
        session.open(channel: id(12))

        let later = AccountMemory(defaults: defaults)
        #expect(later.lastSpot(of: id(1)) == .init(place: .guild(id(1)), channel: id(12)))
        #expect(later.lastSpot(of: id(2)) == nil)
        session.close()
    }

    func restoreGuilds() {
        store.update { state in
            state.currentUser = user(1, name: "me")
            state.add(guild(1), guild(2))
            state.list([channel(11), channel(12)], in: 1)
            state.list([channel(21, guild: 2), channel(22, guild: 2)], in: 2)
            state.talk(dm(5, with: 50), with: user(50))
        }
    }

    @Test
    func aRestoredSessionShowsTheCacheBeforeReady() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        let session = makeSession()
        session.start()
        #expect(session.place == .home)

        restoreGuilds()
        subscription.send(.restored)
        await subscription.batches.pulled(2)

        #expect(session.currentUser == user(1, name: "me"))
        #expect(session.servers.guilds == [guild(1), guild(2)])
        #expect(session.directMessages.conversations.map(\.id) == [id(5)])
        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(12))
        session.close()
    }

    @Test
    func aSessionStartingAfterTheRestoreOpensTheCachedPlace() {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        restoreGuilds()
        let session = makeSession()

        session.start()

        #expect(session.place == .guild(id(1)))
        #expect(session.messages?.channelId == id(12))
        #expect(store.log.calls.last == .connect)
        session.close()
    }

    @Test
    func aGoneCachedPlaceFallsBackAtReady() async {
        memory.remember(.init(place: .guild(id(2)), channel: id(22)), of: id(1))
        restoreGuilds()
        let session = makeSession()
        session.start()
        #expect(session.messages?.channelId == id(22))

        store.update { state in
            state.guildIds.removeAll { $0 == id(2) }
            state.guilds[id(2)] = nil
            state.channelLists[id(2)] = nil
        }
        subscription.send(.guildRemoved(guildId: id(2)), .ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages == nil)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: nil))
        session.close()
    }

    @Test
    func aGoneCachedConversationIsClosedAtReady() async {
        memory.remember(.init(place: .home, channel: id(5)), of: id(1))
        restoreGuilds()
        let session = makeSession()
        session.start()
        #expect(session.messages?.channelId == id(5))

        store.update { $0.privateChannels = [] }
        subscription.send(.channelRemoved(channelId: id(5), guildId: nil), .ready)
        await subscription.batches.pulled(2)

        #expect(session.messages == nil)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: nil))
        session.close()
    }

    @Test
    func aGuildMissingFromTheCacheWaitsForReady() async {
        memory.remember(.init(place: .guild(id(9)), channel: id(90)), of: id(1))
        let session = makeSession()
        session.start()
        let messages = session.messages
        #expect(messages?.channelId == id(90))

        restoreGuilds()
        subscription.send(.restored)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages === messages)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .guild(id(9)), channel: id(90)))

        store.update { state in
            state.add(guild(9))
            state.list([channel(90, guild: 9)], in: 9)
        }
        subscription.send(.ready)
        await subscription.batches.pulled(3)

        #expect(session.place == .guild(id(9)))
        #expect(session.messages === messages)
        session.close()
    }

    @Test
    func aConversationMissingFromTheCacheWaitsForReady() async {
        memory.remember(.init(place: .home, channel: id(7)), of: id(1))
        let session = makeSession()
        session.start()
        let messages = session.messages
        #expect(messages?.channelId == id(7))

        restoreGuilds()
        subscription.send(.restored)
        await subscription.batches.pulled(2)

        #expect(session.messages === messages)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: id(7)))

        store.update { $0.talk(dm(7, with: 70), with: user(70)) }
        subscription.send(.ready)
        await subscription.batches.pulled(3)

        #expect(session.place == .home)
        #expect(session.messages === messages)
        #expect(memory.lastSpot(of: id(1)) == .init(place: .home, channel: id(7)))
        session.close()
    }

    @Test
    func aCachedPlaceThatSurvivesReadyStaysOpen() async {
        memory.remember(.init(place: .guild(id(1)), channel: id(12)), of: id(1))
        restoreGuilds()
        let session = makeSession()
        session.start()
        let channels = session.channels
        let messages = session.messages

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .guild(id(1)))
        #expect(session.channels === channels)
        #expect(session.messages === messages)
        session.close()
    }
}
