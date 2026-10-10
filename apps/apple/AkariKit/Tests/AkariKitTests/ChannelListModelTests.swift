import Testing

@testable import AkariKit

@MainActor
struct ChannelListModelTests {
    let store = FakeStore()

    init() {
        store.update { state in
            state.add(guild(1), guild(2))
            state.list([channel(10, kind: .guildCategory), channel(11), channel(12)], in: 1)
            state.list([channel(20, guild: 2)], in: 2)
        }
    }

    func loaded() -> ChannelListModel {
        let model = ChannelListModel(guildId: id(1), store: store)
        store.log.forget()
        return model
    }

    @Test
    func channelsFollowTheStoreList() {
        let model = ChannelListModel(guildId: id(1), store: store)

        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
        #expect(store.reads == [.channelList(id(1)), .channels([id(10), id(11), id(12)])])
    }

    @Test
    func anUpdatedChannelIsReadAlone() {
        let model = loaded()
        store.update { $0.channels[id(11)] = channel(11, name: "renamed") }

        model.apply(
            EventBatch([
                .channelUpdated(channelId: id(11), guildId: id(1)),
                .channelUpdated(channelId: id(11), guildId: id(1)),
            ])
        )

        #expect(store.reads == [.channelList(id(1)), .channels([id(11)])])
        #expect(model.channels.map(\.name) == ["channel-10", "renamed", "channel-12"])
    }

    @Test
    func eventsForOtherGuildsReadNothing() {
        let model = loaded()

        model.apply(
            EventBatch([
                .channelUpdated(channelId: id(20), guildId: id(2)),
                .currentMemberUpdated(guildId: id(2)),
                .guildUpdated(guildId: id(2)),
                .channelUpdated(channelId: id(30), guildId: nil),
                .messageInserted(channelId: id(11), messageId: id(100)),
            ])
        )

        #expect(store.reads.isEmpty)
        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
    }

    @Test
    func aPermissionChangeRereadsTheList() {
        let model = loaded()
        store.update { state in
            state.list(
                [channel(10, kind: .guildCategory), channel(11), channel(13), channel(12)], in: 1
            )
        }

        model.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))

        #expect(model.channels.map(\.id) == [id(10), id(11), id(13), id(12)])
        #expect(store.reads == [.channelList(id(1)), .channels([id(13)])])

        store.log.forget()
        store.update { $0.channelLists[id(1)] = [id(10), id(11), id(12)] }
        model.apply(EventBatch([.guildUpdated(guildId: id(1))]))

        #expect(model.channels.map(\.id) == [id(10), id(11), id(12)])
        #expect(store.reads == [.channelList(id(1))])
    }
}
