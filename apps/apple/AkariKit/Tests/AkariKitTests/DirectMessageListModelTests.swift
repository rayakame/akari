import Testing

@testable import AkariKit

@MainActor
struct DirectMessageListModelTests {
    let store = FakeStore()

    func loaded() -> DirectMessageListModel {
        let model = DirectMessageListModel(store: store)
        model.reload()
        store.log.forget()
        return model
    }

    @Test
    func conversationsFollowTheStoreOrderWithTheirRecipients() {
        store.update { state in
            state.talk(
                dm(2, with: 20), group(3, name: "Trip", with: [20, 30]),
                with: user(20, name: "Ann"), user(30, name: "Bo")
            )
        }

        let model = loaded()

        #expect(model.conversations.map(\.id) == [id(2), id(3)])
        #expect(model.conversations.map(\.name) == ["Ann", "Trip"])
        #expect(model.conversations[1].recipients == [user(20, name: "Ann"), user(30, name: "Bo")])
    }

    @Test
    func aNewMessageMovesAConversationUpAndReadsOnlyIt() {
        store.update { state in
            state.talk(dm(1, with: 10), dm(2, with: 20), with: user(10), user(20))
        }
        let model = loaded()
        store.update { $0.privateChannels = [id(2), id(1)] }

        model.apply(EventBatch([.channelUpdated(channelId: id(2), guildId: nil)]))

        #expect(store.reads == [.privateChannelList, .channels([id(2)])])
        #expect(model.conversations.map(\.id) == [id(2), id(1)])
    }

    @Test
    func groupNamesFallBackToTheRecipients() {
        store.update { state in
            state.talk(
                group(1, name: "Trip", with: [20]), group(2, with: [20, 30]),
                group(3, name: "", with: [30]), group(4, with: []), dm(5, with: 99),
                with: user(20, name: "Ann"), user(30, name: "Bo")
            )
        }

        let model = loaded()

        #expect(
            model.conversations.map(\.name)
                == ["Trip", "Ann, Bo", "Bo", "Unnamed group", "Unknown user"]
        )
        #expect(model.conversations[4].recipients.isEmpty)
    }

    @Test
    func aRenamedRecipientRenamesTheirConversations() {
        store.update { state in
            state.talk(
                dm(1, with: 20), group(2, with: [20, 30]),
                with: user(20, name: "Ann"), user(30, name: "Bo")
            )
        }
        let model = loaded()
        store.update { $0.users[id(20)] = user(20, name: "Anna") }

        model.apply(EventBatch([.userUpdated(userId: id(20)), .userUpdated(userId: id(77))]))

        #expect(store.reads == [.user(id(20))])
        #expect(model.conversations.map(\.name) == ["Anna", "Anna, Bo"])
    }

    @Test
    func guildEventsReadNothing() {
        store.update { $0.talk(dm(1, with: 10), with: user(10)) }
        let model = loaded()

        model.apply(
            EventBatch([
                .channelUpdated(channelId: id(5), guildId: id(9)),
                .messageInserted(channelId: id(5), messageId: id(50)),
                .guildUpdated(guildId: id(9)),
            ]))

        #expect(store.reads.isEmpty)
    }

    @Test
    func readyRereadsEverything() {
        store.update { $0.talk(dm(1, with: 10), with: user(10, name: "Old")) }
        let model = loaded()
        store.update { $0.users[id(10)] = user(10, name: "New") }

        model.apply(EventBatch([.ready]))

        #expect(store.reads == [.privateChannelList, .channels([id(1)]), .user(id(10))])
        #expect(model.conversations.map(\.name) == ["New"])
    }

    @Test
    func aRemovedConversationLeaves() {
        store.update { $0.talk(dm(1, with: 10), dm(2, with: 20), with: user(10), user(20)) }
        let model = loaded()
        store.update { $0.privateChannels = [id(2)] }

        model.apply(EventBatch([.channelRemoved(channelId: id(1), guildId: nil)]))

        #expect(model.conversations.map(\.id) == [id(2)])
        #expect(store.reads == [.privateChannelList])
    }
}
