import Testing

@testable import AkariKit

@MainActor
struct ServerListModelTests {
    let store = FakeStore()
    let memory = AccountMemory(defaults: MemoryDefaults())

    func model(account: UInt64 = 1) -> ServerListModel {
        ServerListModel(store: store, account: id(account), memory: memory)
    }

    func loaded() -> ServerListModel {
        let model = model()
        model.reload()
        store.log.forget()
        return model
    }

    @Test
    func theRailFollowsTheServerList() {
        store.update { state in
            state.add(guild(1), guild(2), guild(3), guild(4))
            state.serverList = [
                .guild(guildId: id(3)), folder(7, [1, 4], name: "Games"), .guild(guildId: id(2)),
            ]
        }
        let model = model()

        model.reload()

        let games = ServerListModel.FolderItem(
            folder: Folder(id: 7, name: "Games", color: nil, guildIds: [id(1), id(4)]),
            guilds: [guild(1), guild(4)])
        #expect(model.items == [.guild(guild(3)), .folder(games), .guild(guild(2))])
        #expect(model.guilds == [guild(3), guild(1), guild(4), guild(2)])
    }

    @Test
    func readsTheOrderAndRecords() {
        store.update { state in
            state.add(guild(3), guild(1), guild(2))
            state.unavailableGuildIds = [id(4)]
        }
        let model = model()

        model.reload()

        #expect(model.guilds == [guild(3), guild(1), guild(2)])
        #expect(model.items == [.guild(guild(3)), .guild(guild(1)), .guild(guild(2))])
        #expect(model.unavailableIds == [id(4)])
    }

    @Test
    func anUpdateRereadsOnlyThatGuild() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.serverList = [folder(7, [1, 2])]
        }
        let model = loaded()
        store.update { $0.guilds[id(1)] = guild(1, name: "Renamed") }

        model.apply(EventBatch([.guildUpdated(guildId: id(1))]))

        #expect(store.reads == [.guild(id(1))])
        #expect(model.guilds == [guild(1, name: "Renamed"), guild(2)])
        guard case .folder(let item) = model.items.first else {
            Issue.record("expected the folder")
            return
        }
        #expect(item.guilds == [guild(1, name: "Renamed"), guild(2)])
    }

    @Test
    func addedAndRemovedGuildsFollowTheStoreOrder() {
        store.update { $0.add(guild(1), guild(2), guild(3)) }
        let model = loaded()
        store.update { state in
            state.guildIds = [id(4), id(3), id(2)]
            state.guilds[id(1)] = nil
            state.guilds[id(4)] = guild(4)
            state.unavailableGuildIds = [id(5)]
        }

        model.apply(
            EventBatch([
                .guildAdded(guildId: id(4)), .guildRemoved(guildId: id(1)),
                .guildUnavailable(guildId: id(5)),
            ])
        )

        #expect(model.guilds == [guild(4), guild(3), guild(2)])
        #expect(model.unavailableIds == [id(5)])
        #expect(store.reads == [.serverList, .unavailableGuildIds, .guild(id(4))])
    }

    @Test
    func aServerListChangeRereadsTheOrderAlone() {
        store.update { $0.add(guild(1), guild(2)) }
        let model = loaded()
        store.update { $0.serverList = [folder(7, [2, 1])] }

        model.apply(EventBatch([.serverListChanged]))

        #expect(store.reads == [.serverList, .unavailableGuildIds])
        #expect(model.guilds == [guild(2), guild(1)])
    }

    @Test
    func readyRereadsEverything() {
        store.update { $0.add(guild(1), guild(2)) }
        let model = loaded()
        store.update { state in
            state.guildIds = [id(2), id(1)]
            state.guilds[id(1)] = guild(1, name: "Changed while away")
        }

        model.apply(EventBatch([.guildUpdated(guildId: id(2)), .ready]))

        #expect(model.guilds == [guild(2), guild(1, name: "Changed while away")])
        #expect(store.reads.filter { $0 == .serverList }.count == 1)
        #expect(store.reads.filter { $0 == .guild(id(2)) }.count == 1)
    }

    @Test
    func aFolderStartsCollapsedAndRemembersItsState() {
        let first = model()
        #expect(!first.isExpanded(folder: 7))

        first.toggle(folder: 7)

        #expect(first.isExpanded(folder: 7))
        #expect(model().isExpanded(folder: 7), "after a restart")
        #expect(!model(account: 2).isExpanded(folder: 7), "per account")
        first.toggle(folder: 7)
        #expect(!model().isExpanded(folder: 7))
    }

    @Test
    func forgettingAnAccountForgetsItsFolders() {
        model().toggle(folder: 7)

        memory.forget(id(1))

        #expect(!model().isExpanded(folder: 7))
    }

    @Test
    func aServerListChangeKeepsExpandedFolders() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.serverList = [folder(7, [1, 2])]
        }
        let model = loaded()
        model.toggle(folder: 7)
        store.update { $0.serverList = [.guild(guildId: id(2)), folder(7, [1])] }

        model.apply(EventBatch([.serverListChanged]))

        #expect(model.isExpanded(folder: 7))
    }
}
