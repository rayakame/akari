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

    // Without observations of their actors, they stay one muted text (B8).
    @Test
    func otherNoticesAreOneText() {
        #expect(message(.userJoin).noticeParts == [.text("Mira joined the server.")])
        #expect(message(.default, content: "hi").noticeParts == nil)
    }

    @Test
    func noticesNameTheAuthor() {
        #expect(notice(.userJoin) == "Mira joined the server.")
        #expect(notice(.channelPinnedMessage) == "Mira pinned a message to this channel.")
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
