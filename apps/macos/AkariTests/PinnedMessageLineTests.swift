import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// "Ray pinned a message to this channel. See all pinned messages." (TB3.4): the actor, "a
// message" and "pinned messages" white and medium, the rest gray; "a message" jumps to the pin.
@MainActor
struct PinnedMessageLineTests {
    let list = MessageTableTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
    }

    func shown(pinned: UInt64? = 5) throws -> (cell: NoticeCell, text: MessageTextView) {
        let line = row(
            1, by: author(2, "Ray"), at: noon, kind: .channelPinnedMessage,
            reference: MessageReference(
                messageId: pinned.map { MessageId(rawValue: $0) },
                channelId: ChannelId(rawValue: 10), guildId: GuildId(rawValue: 1)))
        list.show([line])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)
        let text = try #require(list.messageTexts(in: cell).first)
        return (cell, text)
    }

    @Test
    func theLineStylesItsActorTheMessageAndPinnedMessages() throws {
        let (cell, text) = try shown()
        #expect(
            text.string.hasPrefix(
                "Ray pinned a message to this channel. See all pinned messages."))

        for strong in ["Ray", "a message", "pinned messages"] {
            #expect(
                try attribute(.font, of: strong, in: text) as? NSFont
                    == TypeScale.noticeName.nsFont)
            #expect(
                try attribute(.foregroundColor, of: strong, in: text) as? NSColor
                    == Palette.textStrong)
        }
        for gray in [" pinned ", " to this channel. See all "] {
            #expect(try attribute(.font, of: gray, in: text) as? NSFont == TypeScale.body.nsFont)
            #expect(
                try attribute(.foregroundColor, of: gray, in: text) as? NSColor
                    == Palette.chatTextMuted)
        }

        // Measured at 2x: #FBFBFB against #818289.
        let rep = try renderedAt2x(cell)
        let message = brightest(in: try glyphs(of: "a message", in: text, within: cell), of: rep)
        let rest = brightest(in: try glyphs(of: "channel", in: text, within: cell), of: rep)
        #expect(message > 0.95, "\(message)")
        #expect(rest > 0.45 && rest < 0.58, "\(rest)")
    }

    @Test
    func aMessageJumpsToThePinnedMessage() throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        let (_, text) = try shown()
        let link = try #require(try attribute(.link, of: "a message", in: text))
        #expect((link as? URL)?.absoluteString == "akari://message/5")

        actions.calls = []
        text.clicked(onLink: link, at: try index(of: "a message", in: text))

        #expect(actions.calls == ["jump to 5"])
    }

    @Test
    func aMessageShowsThePointerAndAnUnderlineOnHover() throws {
        let (_, text) = try shown()
        let layout = try #require(text.layoutManager)
        let underline = { (part: String) in
            layout.temporaryAttribute(
                .underlineStyle, atCharacterIndex: try index(of: part, in: text),
                effectiveRange: nil) as? Int
        }

        text.underline(at: try middle(of: "a message", in: text))
        #expect(try underline("a message") == NSUnderlineStyle.single.rawValue)

        text.underline(at: try middle(of: "Ray", in: text))
        #expect(try underline("a message") == nil)
        #expect(try underline("Ray") == nil)
    }

    // "See all pinned messages" opens the pins popout, which Akari doesn't have yet.
    @Test
    func pinnedMessagesIsntALinkYet() throws {
        let (_, text) = try shown()
        let layout = try #require(text.layoutManager)

        text.underline(at: try middle(of: "pinned messages", in: text))

        #expect(try attribute(.link, of: "pinned messages", in: text) == nil)
        #expect(
            layout.temporaryAttribute(
                .underlineStyle, atCharacterIndex: try index(of: "pinned messages", in: text),
                effectiveRange: nil) == nil)
    }

    @Test
    func aLineThatDoesntSayWhichMessageHasNoLink() throws {
        let (_, text) = try shown(pinned: nil)

        #expect(try attribute(.link, of: "a message", in: text) == nil)
        #expect(
            try attribute(.foregroundColor, of: "a message", in: text) as? NSColor
                == Palette.chatTextMuted)
    }
}

// The model's jump lands; the table puts the message where the unread bar's jump puts its line.
@MainActor
struct MessageJumpScrollTests {
    let list = MessageTableTests()
    let actions = MessageTableTests.RecordingActions()

    init() {
        list.controller.actions = actions
    }

    func jump(to raw: UInt64, _ number: Int) -> MessageListModel.MessageJump {
        MessageListModel.MessageJump(messageId: MessageId(rawValue: raw), number: number)
    }

    // How far the message's row starts below the top of what the list shows.
    func offset(of raw: UInt64) -> CGFloat {
        list.table.rect(ofRow: list.index(of: raw)).minY - list.clip.bounds.minY
    }

    @Test
    func aJumpScrollsTheMessageToTheTop() {
        list.show(MessageTableState(rows: list.page))
        #expect(list.controller.isPinnedToBottom)

        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 1)))

        #expect(abs(offset(of: 120)) <= 0.5)
        #expect(actions.bottoms.last == false)

        // Where the reader is now, a message arriving below moves nothing.
        let arrived = list.page + [row(201, by: author(2), at: list.noon + 201 * 120)]
        list.show(MessageTableState(rows: arrived, messageJump: jump(to: 120, 1)))
        #expect(abs(offset(of: 120)) <= 0.5)
    }

    @Test
    func jumpingToItAgainScrollsAgain() {
        list.show(MessageTableState(rows: list.page))
        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 1)))
        list.scroll(toRowOf: 150)

        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 2)))

        #expect(abs(offset(of: 120)) <= 0.5)
    }

    // Deleted since it was pinned.
    @Test
    func aJumpToAGoneMessageScrollsToTheNextOne() {
        let rows = list.page.filter { $0.id.rawValue != 120 }
        list.show(MessageTableState(rows: rows))

        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 1)))

        #expect(abs(offset(of: 121)) <= 0.5)
    }

    // The pinned message wasn't loaded: the page around it comes with the jump.
    @Test
    func aJumpWithThePageAroundTheMessageScrollsToIt() {
        list.show(MessageTableState(rows: list.page))

        list.show(
            MessageTableState(
                rows: list.messages(50, from: 21), atPresent: false, messageJump: jump(to: 46, 1)))

        #expect(abs(offset(of: 46)) <= 0.5)
    }

    // A new table for a list that jumped before doesn't jump again.
    @Test
    func aNewTableDoesntRepeatAnEarlierJump() {
        list.show(MessageTableState(rows: list.page, messageJump: jump(to: 120, 1)))

        #expect(list.controller.isPinnedToBottom)
    }
}

// Through the app's bridge: the first channel's newest message pins its fifth.
@MainActor
struct PinnedMessageLineScreenTests {
    let screens = SessionScreenTests()
    let account = FormattedAccount(contents: ["test"], pinned: 5)
    let channel = FormattedStore.channelIds[0]

    @Test
    func clickingAMessageScrollsThePinnedMessageToTheTop() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let controller = try #require(table.delegate as? MessageTableController)
        let pinned = MessageId(rawValue: channel.rawValue * 1000 + 5)
        let row = try #require(controller.timeline.items.firstIndex { $0.id == .message(pinned) })
        let clip = try #require(table.enclosingScrollView).contentView
        let offset = { table.rect(ofRow: row).minY - clip.bounds.minY }
        #expect(offset() < 0, "the pinned message is above the view")
        let line = try #require(
            table.view(atColumn: 0, row: table.numberOfRows - 1, makeIfNecessary: false)
                as? NoticeCell)
        let text = try #require(screens.views(MessageTextView.self, in: line).first)
        let link = try #require(try attribute(.link, of: "a message", in: text))

        text.clicked(onLink: link, at: try index(of: "a message", in: text))

        try await screens.waitUntil(host) { abs(offset()) <= 0.5 }
        #expect(session.messages?.messageJump?.messageId == pinned)
    }
}
