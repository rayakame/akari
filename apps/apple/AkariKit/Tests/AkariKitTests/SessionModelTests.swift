import Foundation
import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
struct SessionModelTests {
    let store = FakeStore()
    let account: FakeAccount
    let closes = Locked<[GatewayError?]>([])

    init() {
        account = FakeAccount(store: store)
    }

    var subscription: FakeSubscription { store.subscription }

    func makeSession() -> SessionModel {
        SessionModel(userId: id(1), account: account) { [closes] error in
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
        await session.loop.running?.value

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
        await session.loop.running?.value

        #expect(closes.current == [.AuthenticationFailed])
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
}
