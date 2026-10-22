import AkariKit
import AppKit
import Testing

@testable import Akari

// "Ray started a thread: t2. See all threads." (Q38, D121): the actor, the thread's name and
// "threads" white and medium, the rest gray; the name opens the thread.
@MainActor
struct StartedThreadLineTests {
    let list = MessageTableTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
    }

    func shown() throws -> (cell: NoticeCell, text: MessageTextView) {
        let line = row(
            1, by: author(2, "Ray"), at: noon, kind: .threadCreated, content: "t2",
            reference: MessageReference(
                messageId: nil, channelId: ChannelId(rawValue: 20), guildId: GuildId(rawValue: 1)))
        list.show([line])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)
        let text = try #require(list.messageTexts(in: cell).first)
        return (cell, text)
    }

    @Test
    func theLineStylesItsActorNameAndThreads() throws {
        let (cell, text) = try shown()
        #expect(text.string.hasPrefix("Ray started a thread: t2. See all threads."))

        for strong in ["Ray", "t2", "threads"] {
            #expect(try attribute(.font, of: strong, in: text) as? NSFont == TypeScale.noticeName.nsFont)
            #expect(try attribute(.foregroundColor, of: strong, in: text) as? NSColor == Palette.textStrong)
        }
        for gray in [" started a thread: ", ". See all "] {
            #expect(try attribute(.font, of: gray, in: text) as? NSFont == TypeScale.body.nsFont)
            #expect(
                try attribute(.foregroundColor, of: gray, in: text) as? NSColor
                    == Palette.chatTextMuted)
        }

        // Measured at 2x: #FBFBFB against #818289.
        let rep = try renderedAt2x(cell)
        let actor = brightest(in: try glyphs(of: "Ray", in: text, within: cell), of: rep)
        let rest = brightest(in: try glyphs(of: "started", in: text, within: cell), of: rep)
        #expect(actor > 0.95, "\(actor)")
        #expect(rest > 0.45 && rest < 0.58, "\(rest)")
    }

    @Test
    func theThreadsNameOpensTheThread() throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        let (_, text) = try shown()
        let link = try #require(try attribute(.link, of: "t2", in: text))
        #expect((link as? URL)?.absoluteString == "akari://thread/20")

        actions.calls = []
        text.clicked(onLink: link, at: try index(of: "t2", in: text))

        #expect(actions.calls == ["thread 20 from 10"])
    }

    // A configure adds or drops only the hover area; NSTextView's own areas stay as they were.
    @Test
    func showingTheLineAgainLeavesTheTextViewsTrackingAreasAlone() throws {
        let (_, text) = try shown()
        let before = text.trackingAreas.map(ObjectIdentifier.init)
        try #require(!before.isEmpty)

        text.show(try #require(text.textStorage?.copy() as? NSAttributedString))

        #expect(text.trackingAreas.map(ObjectIdentifier.init) == before)
    }

    // An author's name opens nothing yet (no profile popouts), and neither does the actor.
    @Test
    func theActorDoesWhatAnAuthorsNameDoes() throws {
        let (_, text) = try shown()

        #expect(try attribute(.link, of: "Ray", in: text) == nil)
        #expect(try attribute(MessageTextView.underlinesOnHover, of: "Ray", in: text) == nil)
    }

    @Test
    func theLinksShowThePointerAndAnUnderlineOnHover() throws {
        let (cell, text) = try shown()
        #expect(text.linkTextAttributes?[.cursor] as? NSCursor == NSCursor.pointingHand)
        let name = try index(of: "t2", in: text)
        let layout = try #require(text.layoutManager)
        let underline = {
            layout.temporaryAttribute(.underlineStyle, atCharacterIndex: name, effectiveRange: nil)
                as? Int
        }
        let glyphRect = try glyphs(of: "t2", in: text, within: cell)
        let before = try renderedAt2x(cell)

        text.underline(at: try middle(of: "t2", in: text))

        #expect(underline() == NSUnderlineStyle.single.rawValue)
        // Measured at 2x: at least one full pixel row under the name changes, nothing beside it.
        let hovered = try renderedAt2x(cell)
        let changed = changedPixels(in: glyphRect, before, hovered)
        #expect(changed >= Int(glyphRect.width * 2 * 0.8), "\(changed)")
        let actor = try glyphs(of: "Ray", in: text, within: cell)
        #expect(changedPixels(in: actor, before, hovered) == 0)
        text.underline(at: try middle(of: "Ray", in: text))
        #expect(underline() == nil)
        text.underline(at: try middle(of: "t2", in: text))
        text.underline(at: nil)
        #expect(underline() == nil)
    }

    // "See all threads" opens the Threads modal of the line's channel (amendment 251).
    @Test
    func threadsOpensTheChannelsThreads() throws {
        let actions = MessageTableTests.RecordingActions()
        list.controller.actions = actions
        let (_, text) = try shown()
        let link = try #require(try attribute(.link, of: "threads", in: text))
        #expect((link as? URL)?.absoluteString == "akari://threads")

        actions.calls = []
        text.clicked(onLink: link, at: try index(of: "threads", in: text))

        #expect(actions.calls == ["threads of 10"])
    }

    // Both lines observed draw their actor so (TB3.4); every notice does, in the same gray.
    @Test
    func everyNoticesActorIsWhiteAndMedium() throws {
        list.show([
            row(1, by: author(2, "Ray"), at: noon, kind: .userJoin),
            row(2, by: author(3, "Mira"), at: noon + 60, kind: .pollResult),
        ])
        let join = try #require(list.messageTexts(in: try list.cell(of: 1)).first)
        let poll = try #require(list.messageTexts(in: try list.cell(of: 2)).first)

        #expect(
            try attribute(.foregroundColor, of: "Ray", in: join) as? NSColor == Palette.textStrong)
        #expect(try attribute(.font, of: "Ray", in: join) as? NSFont == TypeScale.noticeName.nsFont)
        // Message 1's time picks Discord's welcome line 5.
        #expect(join.string.hasPrefix("Ray just slid into the server."))
        #expect(
            try attribute(.foregroundColor, of: " just slid", in: join) as? NSColor
                == Palette.chatTextMuted)
        #expect(
            try attribute(.foregroundColor, of: "'s poll", in: poll) as? NSColor
                == Palette.chatTextMuted)
    }
}
