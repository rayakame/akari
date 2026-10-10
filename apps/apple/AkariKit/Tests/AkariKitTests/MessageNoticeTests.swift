import Foundation
import Testing

@testable import AkariKit

struct MessageNoticeTests {
    func notice(_ kind: MessageType, content: String = "") -> String? {
        let row = row(1, by: user(1, name: "Mira"), at: .now, kind: kind)
        let message = row.message
        return Message(
            id: message.id, channelId: message.channelId, kind: kind, author: message.author,
            fromWebhook: false, content: content, timestamp: message.timestamp,
            editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
            embedCount: 0, stickerNames: [], delivery: .sent
        ).notice
    }

    @Test
    func noticesNameTheAuthor() {
        #expect(notice(.userJoin) == "Mira joined the server.")
        #expect(notice(.channelPinnedMessage) == "Mira pinned a message to this channel.")
        #expect(
            notice(.premiumGuildSubscriptionTier2) == "Mira boosted the server. It reached level 2."
        )
        #expect(notice(.threadCreated, content: "plans") == "Mira started a thread: plans")
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
