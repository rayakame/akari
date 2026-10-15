import Testing

@testable import AkariKit

@MainActor @Suite(.timeLimit(.minutes(1)))
final class MarkReadTests {
    let store = FakeStore()
    let account: FakeAccount
    let general: ChannelId = id(11)

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.connection = .online
            state.currentUser = user(1)
            state.add(guild(1), guild(2), guild(3), guild(4))
            state.serverList = [.guild(guildId: id(1)), folder(7, [3, 2]), folder(8, [4])]
            state.list([channel(11), channel(12)], in: 1)
            state.windows[id(11)] = window([1, 2])
            state.show(message(1, in: 11), message(2, in: 11))
            state.talk(dm(20, with: 5), dm(21, with: 6), with: user(5), user(6))
        }
    }

    func makeSession() -> SessionModel {
        SessionModel(
            userId: id(1), account: account, memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
    }

    func folders(_ session: SessionModel) -> [ServerListModel.FolderItem] {
        session.servers.items.compactMap { item in
            if case .folder(let folder) = item { folder } else { nil }
        }
    }

    @Test
    func escapeMarksReadAndJumpsToThePresent() async throws {
        store.update { $0.windows[id(11)] = window([1, 2], latest: false) }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        let list = try #require(session.messages)
        store.log.forget()

        await list.escape()

        #expect(account.markedRead.current == [general])
        #expect(store.log.actions == [.load(general, .latest(limit: 50))])
        session.close()
    }

    @Test
    func escapeAtThePresentOnlyMarksRead() async throws {
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))
        let list = try #require(session.messages)
        store.log.forget()

        await list.escape()

        #expect(account.markedRead.current == [general])
        #expect(store.log.actions.isEmpty)
        session.close()
    }

    @Test
    func shiftEscapeMarksTheServerRead() {
        let session = makeSession()
        session.start()
        session.open(.guild(id(3)))

        session.markServerRead()

        #expect(account.markedGuildsRead.current == [[id(3)]])
        session.close()
    }

    @Test
    func shiftEscapeAtHomeDoesNothing() {
        let session = makeSession()
        session.start()
        session.open(channel: id(20))
        #expect(session.place == .home)

        session.markServerRead()

        #expect(account.markedGuildsRead.current.isEmpty)
        #expect(account.markedRead.current.isEmpty)
        session.close()
    }

    @Test
    func markFolderReadMarksItsGuildsInOneCall() throws {
        let session = makeSession()
        session.start()
        let folder = try #require(folders(session).first)

        session.markRead(folder: folder)

        #expect(account.markedGuildsRead.current == [[id(3), id(2)]])
        session.close()
    }

    @Test
    func theMenusMarkTheirServerOrChannelRead() {
        let session = makeSession()
        session.start()

        session.markRead(guild: id(4))
        session.markRead(channel: id(21))
        session.markRead(channel: general)

        #expect(account.markedGuildsRead.current == [[id(4)]])
        #expect(account.markedRead.current == [id(21), general])
        session.close()
    }

    @Test
    func theReadItemIsDisabledWhenNothingIsUnread() throws {
        store.update { state in
            state.guildBadges = [id(1): badge(), id(3): badge(unread: false, mentions: 1)]
            state.channelBadges = [id(11): badge(unread: false, mentions: 2), id(20): badge()]
        }
        let session = makeSession()
        session.start()
        session.open(.guild(id(1)))

        #expect(session.canMarkRead(guild: id(1)))
        #expect(!session.canMarkRead(guild: id(2)))
        #expect(session.canMarkRead(guild: id(3)), "mentions without the dot")
        #expect(folders(session).map(session.canMarkRead(folder:)) == [true, false])
        session.servers.toggle(folder: 7)
        let expanded = try #require(folders(session).first)
        #expect(session.canMarkRead(folder: expanded), "its servers show the badges now")
        #expect(session.canMarkRead(channel: general))
        #expect(!session.canMarkRead(channel: id(12)))
        #expect(session.canMarkRead(channel: id(20)), "a conversation, from anywhere")
        #expect(!session.canMarkRead(channel: id(21)))
        session.close()
    }
}
