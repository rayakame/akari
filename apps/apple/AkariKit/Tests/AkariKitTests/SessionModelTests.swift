import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
final class SessionModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let closes = Locked<[GatewayError?]>([])
    let suite = "app.akari.tests.\(UUID().uuidString)"
    let memory: AccountMemory

    init() throws {
        account = FakeAccount(store: store)
        memory = AccountMemory(defaults: try #require(UserDefaults(suiteName: suite)))
    }

    deinit {
        UserDefaults.standard.removePersistentDomain(forName: suite)
    }

    var subscription: FakeSubscription { store.subscription }

    func makeSession() -> SessionModel {
        SessionModel(userId: id(1), account: account, memory: memory) { [closes] error in
            closes.withLock { $0.append(error) }
        }
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
        #expect(calls.contains(.read(.guildIds)))
        #expect(session.connection == .connecting)
        #expect(session.currentUser == user(1, name: "me"))
        #expect(session.guilds.guilds == [guild(5)])
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
        #expect(session.guilds.guilds.isEmpty)
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
    func aGoneGuildFallsBackHome() async {
        memory.remember(.init(place: .guild(id(9)), channel: id(90)), of: id(1))
        let session = makeSession()
        session.start()

        subscription.send(.ready)
        await subscription.batches.pulled(2)

        #expect(session.place == .home)
        #expect(session.messages == nil)
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

        let later = AccountMemory(defaults: try #require(UserDefaults(suiteName: suite)))
        #expect(later.lastSpot(of: id(1)) == .init(place: .guild(id(1)), channel: id(12)))
        #expect(later.lastSpot(of: id(2)) == nil)
        session.close()
    }
}
