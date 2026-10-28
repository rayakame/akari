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
            embeds: [], stickers: [], componentsV2: false, delivery: .sent,
            formatted: formattedText(content: content), mentionsMe: false, reference: reference,
            hasThread: false, mentions: [], call: nil,
            roleSubscription: nil, purchasedProduct: nil, actor: nil, applicationName: nil,
            embedFields: []
        )
    }

    func notice(_ kind: MessageType, content: String = "") -> String? {
        message(kind, content: content).notice(in: .init())
    }

    // Q38: "Ray started a thread: t2. See all threads.", the actor, the name and "threads" strong.
    @Test
    func aNewThreadsLineNamesItsActorItsThreadAndAllThreads() {
        let line = message(
            .threadCreated, content: "plans",
            reference: MessageReference(messageId: nil, channelId: id(20), guildId: id(1)))

        #expect(
            line.noticeParts(in: .init()) == [
                .actor("Mira"), .text(" started a thread: "), .thread(name: "plans", id: id(20)),
                .text(". See all "), .threads("threads"), .text("."),
            ])
        #expect(line.notice(in: .init()) == "Mira started a thread: plans. See all threads.")
    }

    @Test
    func aNewThreadsLineWithoutItsThreadHasNoLink() {
        #expect(
            message(.threadCreated, content: "plans").noticeParts(in: .init())?.contains(
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
            line.noticeParts(in: .init()) == [
                .actor("Mira"), .text(" pinned "), .message("a message", id: id(5)),
                .text(" to this channel. See all "), .pins("pinned messages"), .text("."),
            ])
        #expect(line.notice(in: .init()) == "Mira pinned a message to this channel. See all pinned messages.")
    }

    // Discord's string for a pin without its reference has "a message" as plain text.
    @Test
    func aPinsLineWithoutItsMessageSaysSoInText() {
        #expect(
            message(.channelPinnedMessage).noticeParts(in: .init()) == [
                .actor("Mira"), .text(" pinned a message to this channel. See all "),
                .pins("pinned messages"), .text("."),
            ])
    }

    // Discord names the thread as the client has it now, else as the line was posted.
    @Test
    func aNewThreadsLineTakesTheThreadsCurrentName() {
        let line = message(
            .threadCreated, content: "plans",
            reference: MessageReference(messageId: nil, channelId: id(20), guildId: id(1)))
        var renamed = Message.NoticeContext()
        renamed.threadName = "launch"

        #expect(line.notice(in: renamed) == "Mira started a thread: launch. See all threads.")
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
