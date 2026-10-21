import Foundation
import Testing

@testable import AkariKit

// Channel 11 in guild 1, with thread 31 under it; the user is in 11.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ThreadMembershipTests {
    let store = FakeStore()
    let account: FakeAccount

    init() {
        account = FakeAccount(store: store)
    }

    func session(joined: Bool, listed: Bool, archived: Bool = false) -> SessionModel {
        var started = thread(31, in: 11, name: "t1")
        started.thread?.joined = joined
        started.thread?.archived = archived
        store.update { state in
            state.add(guild(1))
            state.list([channel(11)] + (listed ? [started] : []), in: 1)
            state.channels[id(31)] = started
        }
        let session = SessionModel(userId: id(1), account: account) { _ in }
        session.start()
        session.open(.guild(id(1)))
        session.open(channel: id(11))
        return session
    }

    // As akari-core does: the thread's change comes as ChannelUpdated and the list follows.
    func list(_ session: SessionModel, threadListed: Bool) {
        store.update { state in
            state.channelLists[id(1)] = [id(11)] + (threadListed ? [id(31)] : [])
        }
        session.apply(EventBatch([.channelUpdated(channelId: id(31), guildId: id(1))]))
    }

    func shown(_ session: SessionModel) -> [ChannelId] {
        session.channels?.shown(open: session.messages?.channelId).map(\.id) ?? []
    }

    @Test
    func anUnjoinedThreadOpenedFromItsBoxOffersJoining() async {
        let session = session(joined: false, listed: false)
        await session.open(thread: id(31), from: id(11))

        #expect(session.messages?.channelId == id(31))
        #expect(
            session.membershipItem(for: id(31))
                == SessionModel.MembershipItem(joins: true, enabled: true))
        #expect(session.membershipItem(for: id(31))?.title == "Join Thread")
        session.close()
    }

    @Test
    func joiningListsTheThread() async {
        let session = session(joined: false, listed: false)
        await session.open(thread: id(31), from: id(11))

        await session.join(thread: id(31), from: .toolbarOverflow)
        list(session, threadListed: true)

        #expect(store.log.calls.contains(.joinThread(id(31), .toolbarOverflow)))
        #expect(session.channels?.channels.map(\.id) == [id(11), id(31)])
        #expect(session.membershipItem(for: id(31))?.title == "Leave Thread")
        session.close()
    }

    @Test
    func leavingTheOpenThreadKeepsItsRowUntilTheUserOpensSomethingElse() async {
        let session = session(joined: true, listed: true)
        session.open(channel: id(31))

        await session.leave(thread: id(31), from: .toolbarOverflow)
        list(session, threadListed: false)

        #expect(store.log.calls.contains(.leaveThread(id(31), .toolbarOverflow)))
        #expect(session.messages?.channelId == id(31))
        #expect(shown(session) == [id(11), id(31)])
        session.open(channel: id(11))
        #expect(shown(session) == [id(11)])
        session.close()
    }

    @Test
    func leavingFromARowSendsItsLocation() async {
        let session = session(joined: true, listed: true)

        await session.leave(thread: id(31), from: .contextMenu)
        list(session, threadListed: false)

        #expect(store.log.calls.contains(.leaveThread(id(31), .contextMenu)))
        #expect(shown(session) == [id(11)])
        session.close()
    }

    @Test
    func theItemsAreDisabledForArchivedThreads() async {
        let session = session(joined: true, listed: true, archived: true)

        #expect(
            session.membershipItem(for: id(31))
                == SessionModel.MembershipItem(joins: false, enabled: false))
        await session.leave(thread: id(31), from: .contextMenu)
        #expect(!store.log.calls.contains(.leaveThread(id(31), .contextMenu)))
        session.close()
    }

    @Test
    func aChannelHasNoMembershipItem() {
        let session = session(joined: true, listed: true)

        #expect(session.membershipItem(for: id(11)) == nil)
        session.close()
    }

    @Test
    func aFullThreadSaysSoAsDiscordDoes() async {
        let session = session(joined: false, listed: false)
        await session.open(thread: id(31), from: id(11))
        account.membershipError.withLock {
            $0 = .Discord(status: 400, code: 30033, message: "Max number of thread participants")
        }

        await session.join(thread: id(31), from: .toolbarOverflow)

        #expect(
            session.membershipError
                == SessionModel.MembershipError(
                    title: "Cannot join thread",
                    message: "This thread has reached the maximum possible number of members."))
        session.dismissMembershipError()
        #expect(session.membershipError == nil)
        session.close()
    }

    @Test
    func anotherFailureSaysTryAgain() async {
        let session = session(joined: true, listed: true)
        account.membershipError.withLock { $0 = .Network(kind: .connect) }

        await session.leave(thread: id(31), from: .contextMenu)

        #expect(
            session.membershipError
                == SessionModel.MembershipError(
                    title: "Error", message: "An error occurred, please try again."))
        session.close()
    }
}
