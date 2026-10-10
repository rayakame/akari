import Testing

@testable import AkariKit

@MainActor
struct GuildListModelTests {
    let store = FakeStore()

    func loaded() -> GuildListModel {
        let model = GuildListModel(store: store)
        model.reload()
        store.log.forget()
        return model
    }

    @Test
    func readsTheOrderAndRecords() {
        store.update { state in
            state.add(guild(3), guild(1), guild(2))
            state.unavailableGuildIds = [id(4)]
        }
        let model = GuildListModel(store: store)

        model.reload()

        #expect(model.guilds == [guild(3), guild(1), guild(2)])
        #expect(model.unavailableIds == [id(4)])
    }

    @Test
    func anUpdateRereadsOnlyThatGuild() {
        store.update { $0.add(guild(1), guild(2)) }
        let model = loaded()
        store.update { $0.guilds[id(1)] = guild(1, name: "Renamed") }

        model.apply(EventBatch([.guildUpdated(guildId: id(1))]))

        #expect(store.reads == [.guild(id(1))])
        #expect(model.guilds == [guild(1, name: "Renamed"), guild(2)])
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
        #expect(store.reads == [.guildIds, .unavailableGuildIds, .guild(id(4))])
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
        #expect(store.reads.filter { $0 == .guildIds }.count == 1)
        #expect(store.reads.filter { $0 == .guild(id(2)) }.count == 1)
    }
}
