import AkariKit
import AppKit
import Testing

@testable import Akari

// Discord's names and bold parts draw white and medium; a part links only where Akari has its
// target.
@MainActor
struct NoticePartsTests {
    let list = MessageTableTests()
    let actions = MessageTableTests.RecordingActions()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
        list.controller.actions = actions
    }

    func text(of row: MessageListModel.Row) throws -> MessageTextView {
        list.show([row])
        let cell = try #require(try list.cell(of: row.id.rawValue) as? NoticeCell)
        return try #require(list.messageTexts(in: cell).first)
    }

    @Test
    func aStrongPartIsANameWithoutALink() throws {
        let text = try text(
            of: row(1, by: author(2, "Ray"), at: noon, kind: .channelNameChange, content: "lobby"))

        #expect(text.string.hasPrefix("Ray changed the channel name: lobby"))
        #expect(try attribute(.font, of: "lobby", in: text) as? NSFont == TypeScale.noticeName.nsFont)
        #expect(
            try attribute(.foregroundColor, of: "lobby", in: text) as? NSColor
                == Palette.textStrong)
        #expect(try attribute(.link, of: "lobby", in: text) == nil)
    }

    @Test
    func anAlertsChannelOpensTheChannel() throws {
        var context = Message.NoticeContext()
        context.alertChannelName = "lobby"
        context.alertChannelId = ChannelId(rawValue: 11)
        let text = try text(
            of: row(
                1, at: noon, kind: .autoModerationAction,
                fields: [EmbedField(name: "decision_outcome", value: "blocked")],
                context: context))
        let link = try #require(try attribute(.link, of: "lobby", in: text))

        text.clicked(onLink: link, at: try index(of: "lobby", in: text))

        #expect((link as? URL)?.absoluteString == "akari://channel/11")
        #expect(actions.calls.last == "channel 11")
    }

    @Test
    func aDiscoveryNoticesServerOpensTheServer() throws {
        var context = Message.NoticeContext()
        context.guildName = "Lab"
        context.guildId = GuildId(rawValue: 1)
        let text = try text(of: row(1, at: noon, kind: .guildDiscoveryDisqualified, context: context))
        let link = try #require(try attribute(.link, of: "Lab", in: text))

        text.clicked(onLink: link, at: try index(of: "Lab", in: text))

        #expect((link as? URL)?.absoluteString == "akari://guild/1")
        #expect(actions.calls.last == "guild 1")
        #expect(try attribute(.link, of: "Server Settings", in: text) == nil)
    }

    @Test
    func aPollsTitleJumpsToThePoll() throws {
        let text = try text(
            of: row(
                1, by: author(2, "Ray"), at: noon, kind: .pollResult,
                reference: MessageReference(
                    messageId: MessageId(rawValue: 5), channelId: ChannelId(rawValue: 10),
                    guildId: nil),
                fields: [EmbedField(name: "poll_question_text", value: "Lunch?")]))
        let link = try #require(try attribute(.link, of: "Lunch?", in: text))

        text.clicked(onLink: link, at: try index(of: "Lunch?", in: text))

        #expect(text.string.hasPrefix("Ray's poll Lunch? has closed."))
        #expect(actions.calls.last == "jump to 5")
    }

    // The header shows the name the list resolves, by the rule system lines use too.
    @Test
    func aHeaderShowsTheRowsName() throws {
        list.show([row(1, by: author(2, "Ray"), at: noon, authorName: "Rayo")])
        let cell = try list.cell(of: 1)

        let names = list.textFields(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }
            .map(\.stringValue)

        #expect(names.contains("Rayo"))
        #expect(!names.contains("Ray"))
    }

    @Test
    func aKindWithoutALineHasNoRow() {
        list.show([
            row(1, at: noon), row(2, at: noon + 1, kind: .guildInviteReminder),
            row(3, at: noon + 2),
        ])

        #expect(!list.controller.timeline.items.contains { $0.id == .message(MessageId(rawValue: 2)) })
    }
}
