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

    func index(of part: String, in text: NSTextView) throws -> Int {
        let range = (text.string as NSString).range(of: part)
        try #require(range.location != NSNotFound)
        return range.location
    }

    func attribute(_ key: NSAttributedString.Key, of part: String, in text: NSTextView) throws
        -> Any?
    {
        text.textStorage?.attribute(key, at: try index(of: part, in: text), effectiveRange: nil)
    }

    func changedPixels(in rect: NSRect, _ one: NSBitmapImageRep, _ other: NSBitmapImageRep)
        -> Int
    {
        var changed = 0
        for x in Int(rect.minX * 2)..<Int(rect.maxX * 2) {
            for y in Int(rect.minY * 2)..<Int(rect.maxY * 2)
            where one.colorAt(x: x, y: y) != other.colorAt(x: x, y: y) {
                changed += 1
            }
        }
        return changed
    }

    // The middle of `part`'s glyphs in the text view's coordinates.
    func middle(of part: String, in text: NSTextView) throws -> NSPoint {
        let rect = try glyphs(of: part, in: text, within: text)
        return NSPoint(x: rect.midX, y: text.isFlipped ? rect.midY : text.bounds.height - rect.midY)
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
        #expect((link as? URL)?.absoluteString == "akari://channel/20")

        actions.calls = []
        text.clicked(onLink: link, at: try index(of: "t2", in: text))

        #expect(actions.calls == ["channel 20"])
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

    // "See all threads" opens the Threads popover, which comes in a later PR.
    @Test
    func threadsIsntALinkYet() throws {
        let (_, text) = try shown()
        let threads = try index(of: "threads", in: text)
        let layout = try #require(text.layoutManager)

        text.underline(at: try middle(of: "threads", in: text))

        #expect(try attribute(.link, of: "threads", in: text) == nil)
        #expect(
            layout.temporaryAttribute(
                .underlineStyle, atCharacterIndex: threads, effectiveRange: nil) == nil)
    }

    // Other notices keep their one muted text until their actors are observed.
    @Test
    func otherNoticesStayAsTheyWere() throws {
        list.show([row(1, by: author(2, "Ray"), at: noon, kind: .userJoin)])
        let cell = try #require(try list.cell(of: 1) as? NoticeCell)
        let text = try #require(list.messageTexts(in: cell).first)

        #expect(try attribute(.foregroundColor, of: "Ray", in: text) as? NSColor == Palette.textMuted)
        #expect(try attribute(.font, of: "Ray", in: text) as? NSFont == TypeScale.body.nsFont)
    }
}
