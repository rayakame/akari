import Foundation
import Testing

@testable import AkariKit

// A list words its system lines for where they are and from what the store knows.
@MainActor
struct NoticeContextTests {
    let store = FakeStore()
    let account: FakeAccount
    let ray = user(2, name: "Ray")
    let mira = user(3, name: "Mira")

    init() {
        account = FakeAccount(store: store)
        store.update { state in
            state.currentUser = user(1, name: "Me")
            state.add(guild(1, name: "Lab"))
            state.channels[id(10)] = channel(10, guild: 1, name: "general")
        }
    }

    // The list of `channel`, which shows `message`.
    func row(in channel: ChannelId, _ message: Message) -> MessageListModel.Row? {
        store.update { state in
            state.show(message)
            state.windows[channel] = window([message.id.rawValue])
        }
        return MessageListModel(channelId: channel, userId: id(1), account: account, store: store)
            .rows.first
    }

    var ended: MessageCall {
        MessageCall(
            participants: [ray.id, mira.id, id(9)],
            endedTimestamp: Date(timeIntervalSince1970: 1_700_000_000 + 3600))
    }

    @Test
    func aThreadsLineIsWordedForTheThread() {
        store.update { $0.channels[id(20)] = thread(20, in: 10) }

        let added = row(in: id(20), message(1, in: 20, by: ray, kind: .recipientAdd, mentions: [mira]))

        #expect(added?.notice == "Ray added Mira to the thread.")
    }

    @Test
    func aGroupsRenameOffersToEditTheGroup() {
        store.update { $0.channels[id(30)] = group(30, with: [2]) }

        let renamed = row(in: id(30), message(1, in: 30, by: ray, content: "pals", kind: .channelNameChange))

        #expect(renamed?.notice == "Ray changed the group name: pals. Edit Group")
    }

    @Test
    func aForumPostsRenameIsItsTitle() {
        store.update { state in
            state.channels[id(40)] = channel(40, guild: 1, kind: .guildForum)
            state.channels[id(41)] = thread(41, in: 40)
        }

        let renamed = row(in: id(41), message(1, in: 41, by: ray, content: "Q3", kind: .channelNameChange))

        #expect(renamed?.notice == "Ray changed the post title: Q3")
    }

    @Test
    func aBoostNamesTheServer() {
        let boost = row(in: id(10), message(1, by: ray, content: "", kind: .premiumGuildSubscriptionTier2))

        #expect(boost?.notice == "Ray just boosted the server! Lab has achieved Level 2!")
    }

    @Test
    func aNewThreadsLineTakesTheThreadsCurrentName() {
        store.update { $0.channels[id(20)] = thread(20, in: 10, name: "launch") }

        let line = row(
            in: id(10),
            message(
                1, by: ray, content: "plans", kind: .threadCreated,
                reference: MessageReference(messageId: nil, channelId: id(20), guildId: id(1))))

        #expect(line?.notice == "Ray started a thread: launch. See all threads.")
    }

    // Discord links the channel only while the user can see it.
    @Test
    func anAlertLinksAChannelTheUserCanOpen() {
        store.update { state in
            state.channels[id(11)] = channel(11, guild: 1, name: "secret")
            state.channels[id(12)] = channel(12, guild: 1, name: "lobby")
            state.permissions[id(12)] = [.viewChannel]
        }
        func alert(_ channel: UInt64) -> [Message.NoticePart]? {
            row(
                in: id(10),
                message(
                    channel, kind: .autoModerationAction,
                    fields: [EmbedField(name: "channel_id", value: String(channel))])
            )?.noticeParts
        }

        #expect(alert(12)?.last == .channel("lobby", id: id(12)))
        #expect(alert(11)?.last == .strong("secret"))
        #expect(alert(13)?.last == .strong("unknown-channel"))
    }

    @Test
    func anAcceptedFriendRequestNamesTheDMsOtherPerson() {
        store.update { state in
            state.channels[id(50)] = dm(50, with: 2)
            state.users[ray.id] = ray
        }

        let accepted = row(in: id(50), message(1, in: 50, by: user(1, name: "Me"), content: "", kind: .friendRequestAccepted))

        #expect(accepted?.notice == "You accepted Ray's friend request.")
    }

    // Like Discord's client, only the people the store knows; Akari's store knows DM recipients.
    @Test
    func aVoiceSessionNamesTheParticipantsTheStoreKnows() {
        store.update { $0.users[mira.id] = mira }

        let session = row(in: id(10), message(1, by: ray, kind: .voiceSession, call: ended))

        #expect(session?.notice == "Ray and Mira were hanging out for an hour.")
    }

    @Test
    func aCallTheUserWasntInIsMissed() {
        store.update { $0.channels[id(50)] = dm(50, with: 2) }

        let call = row(in: id(50), message(1, in: 50, by: ray, kind: .call, call: ended))

        #expect(call?.notice == "You missed a call from Ray that lasted an hour.")
    }

    // A thread started from a system message words it from the original's needs.
    @Test
    func aThreadsStarterIsWordedForTheOriginal() {
        let original = message(41, by: ray, content: "", kind: .premiumGuildSubscriptionTier2)
        store.update { state in
            state.channels[id(20)] = thread(20, in: 10)
            state.starters[id(20)] = .loaded(message: original)
        }

        let starter = row(
            in: id(20),
            message(
                1, in: 20, by: ray, kind: .threadStarterMessage,
                reference: MessageReference(messageId: id(41), channelId: id(10), guildId: id(1))))

        #expect(starter?.notice == "Ray just boosted the server! Lab has achieved Level 2!")
    }

    @Test
    func aRenamedThreadRewordsItsLine() {
        store.update { $0.channels[id(20)] = thread(20, in: 10, name: "plans") }
        let line = message(
            1, by: ray, content: "plans", kind: .threadCreated,
            reference: MessageReference(messageId: nil, channelId: id(20), guildId: id(1)))
        _ = row(in: id(10), line)
        let model = MessageListModel(channelId: id(10), userId: id(1), account: account, store: store)

        store.update { $0.channels[id(20)] = thread(20, in: 10, name: "launch") }
        model.apply(EventBatch([.channelUpdated(channelId: id(20), guildId: id(1))]))

        #expect(model.rows.first?.notice == "Ray started a thread: launch. See all threads.")
    }

    // As Discord's client names people: the member's server nickname where Akari has it, then
    // the display name.
    @Test
    func noticesAndHeadersUseTheServerNickname() {
        let me = user(1, name: "Me")
        store.update { state in
            state.channels[id(20)] = thread(20, in: 10)
            state.memberNicks[id(1)] = [id(1): "Boss", mira.id: "Mimi"]
        }

        let added = row(in: id(20), message(1, in: 20, by: me, kind: .recipientAdd, mentions: [mira]))

        #expect(added?.notice == "Boss added Mimi to the thread.")
        #expect(added?.authorName == "Boss")
        #expect(row(in: id(10), message(2, by: ray))?.authorName == "Ray")
    }

    @Test
    func aNicknameChangeRenamesTheRows() {
        store.update { state in
            state.memberNicks[id(1)] = [id(1): "Boss"]
            state.show(message(1, by: user(1, name: "Me")))
            state.windows[id(10)] = window([1])
        }
        let model = MessageListModel(channelId: id(10), userId: id(1), account: account, store: store)
        #expect(model.rows.first?.authorName == "Boss")

        store.update { $0.memberNicks[id(1)] = [id(1): "Chief"] }
        model.apply(EventBatch([.currentMemberUpdated(guildId: id(1))]))

        #expect(model.rows.first?.authorName == "Chief")
    }

    // A DM has no server; a webhook's author isn't a member.
    @Test
    func aDMAndAWebhookGoByTheirDisplayNames() {
        store.update { state in
            state.channels[id(50)] = dm(50, with: 2)
            state.memberNicks[id(1)] = [ray.id: "Rayo"]
        }
        let webhook = Message(
            id: id(3), channelId: id(10), kind: .default, author: ray, fromWebhook: true,
            content: "hook", timestamp: .now, editedTimestamp: nil, pinned: false,
            mentionEveryone: false, attachments: [], embedCount: 0, stickerNames: [],
            componentsV2: false, delivery: .sent, formatted: formattedText(content: "hook"),
            mentionsMe: false, reference: nil, hasThread: false, mentions: [], call: nil,
            roleSubscription: nil, purchasedProduct: nil, actor: nil, applicationName: nil,
            embedFields: [])

        #expect(row(in: id(50), message(1, in: 50, by: ray))?.authorName == "Ray")
        #expect(row(in: id(10), webhook)?.authorName == "Ray")
        #expect(row(in: id(10), message(2, by: ray))?.authorName == "Rayo")
    }

    @Test
    func aKindWithoutALineIsHidden() {
        let reminder = row(in: id(10), message(1, by: ray, kind: .guildInviteReminder))

        #expect(reminder?.noticeParts == [])
        #expect(reminder?.isHidden == true)
        #expect(row(in: id(10), message(2, by: ray))?.isHidden == false)
    }
}
