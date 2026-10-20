import Foundation
import Testing

@testable import AkariKit

struct MessageNoticeTests {
    func message(
        _ kind: MessageType, content: String = "", reference: MessageReference? = nil
    ) -> Message {
        let row = row(1, by: user(1, name: "Mira"), at: .now, kind: kind)
        let message = row.message
        return Message(
            id: message.id, channelId: message.channelId, kind: kind, author: message.author,
            fromWebhook: false, content: content, timestamp: message.timestamp,
            editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
            embedCount: 0, stickerNames: [], componentsV2: false, delivery: .sent,
            formatted: formattedText(content: content), mentionsMe: false, reference: reference,
            hasThread: false
        )
    }

    func notice(_ kind: MessageType, content: String = "") -> String? {
        message(kind, content: content).notice
    }

    // Q38: "Ray started a thread: t2. See all threads.", the actor, the name and "threads" strong.
    @Test
    func aNewThreadsLineNamesItsActorItsThreadAndAllThreads() {
        let line = message(
            .threadCreated, content: "plans",
            reference: MessageReference(messageId: nil, channelId: id(20), guildId: id(1)))

        #expect(
            line.noticeParts == [
                .actor("Mira"), .text(" started a thread: "), .thread(name: "plans", id: id(20)),
                .text(". See all "), .threads("threads"), .text("."),
            ])
        #expect(line.notice == "Mira started a thread: plans. See all threads.")
    }

    @Test
    func aNewThreadsLineWithoutItsThreadHasNoLink() {
        #expect(
            message(.threadCreated, content: "plans").noticeParts?.contains(
                .thread(name: "plans", id: nil)) == true)
    }

    // TB3.4: "Ray pinned a message to this channel. See all pinned messages.", the actor, "a
    // message" and "pinned messages" strong.
    @Test
    func aPinsLineNamesItsActorTheMessageAndAllPinnedMessages() {
        let line = message(
            .channelPinnedMessage,
            reference: MessageReference(messageId: id(5), channelId: id(10), guildId: id(1)))

        #expect(
            line.noticeParts == [
                .actor("Mira"), .text(" pinned "), .message("a message", id: id(5)),
                .text(" to this channel. See all "), .pins("pinned messages"), .text("."),
            ])
        #expect(line.notice == "Mira pinned a message to this channel. See all pinned messages.")
    }

    // Discord's string for a pin without its reference has "a message" as plain text.
    @Test
    func aPinsLineWithoutItsMessageSaysSoInText() {
        #expect(
            message(.channelPinnedMessage).noticeParts == [
                .actor("Mira"), .text(" pinned a message to this channel. See all "),
                .pins("pinned messages"), .text("."),
            ])
    }

    // Both lines observed draw their actor as a name (TB3.4), so every kind does.
    @Test(arguments: [
        MessageType.userJoin, .channelPinnedMessage, .premiumGuildSubscription,
        .premiumGuildSubscriptionTier1, .premiumGuildSubscriptionTier2,
        .premiumGuildSubscriptionTier3, .recipientAdd, .recipientRemove, .call,
        .channelNameChange, .channelIconChange, .threadCreated, .channelFollowAdd, .stageStart,
        .stageEnd,
    ])
    func everyNoticeWithAnActorNamesIt(kind: MessageType) {
        let parts = message(kind, content: "lobby").noticeParts

        #expect(parts?.first == .actor("Mira"))
        #expect(parts?.dropFirst().first?.text.hasPrefix(" ") == true)
    }

    @Test
    func aJoinIsItsActorThenOneText() {
        #expect(message(.userJoin).noticeParts == [.actor("Mira"), .text(" joined the server.")])
    }

    @Test
    func noticesWithoutAnActorAreOneText() {
        #expect(message(.pollResult).noticeParts == [.text("A poll has ended.")])
        #expect(message(.guildInviteReminder).noticeParts == [.text("System message")])
        #expect(message(.default, content: "hi").noticeParts == nil)
    }

    @Test
    func noticesNameTheAuthor() {
        #expect(notice(.userJoin) == "Mira joined the server.")
        #expect(
            notice(.channelPinnedMessage)
                == "Mira pinned a message to this channel. See all pinned messages.")
        #expect(
            notice(.premiumGuildSubscriptionTier2) == "Mira boosted the server. It reached level 2."
        )
        #expect(
            notice(.threadCreated, content: "plans")
                == "Mira started a thread: plans. See all threads.")
        #expect(
            notice(.channelNameChange, content: "lobby") == "Mira changed the channel name: lobby")
        #expect(notice(.pollResult) == "A poll has ended.")
    }

    @Test
    func otherSystemMessagesSayWhatTheyAre() {
        #expect(notice(.guildInviteReminder) == "System message")
        #expect(notice(.changelog, content: "New things") == "System message: New things")
    }

    @Test(arguments: [
        MessageType.default, .reply, .chatInputCommand, .contextMenuCommand,
        .threadStarterMessage,
    ])
    func messagesPeopleWriteHaveNoNotice(kind: MessageType) {
        #expect(notice(kind, content: "hi") == nil)
    }

    @Test
    func unknownTypesShowAsMessages() {
        #expect(notice(.unknown(999), content: "hi") == nil)
    }
}
