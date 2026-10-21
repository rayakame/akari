import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Drawn at 2 px per point whatever the screen, so measures don't depend on the Mac.
@MainActor
func renderedAt2x(_ view: NSView) throws -> NSBitmapImageRep {
    let rep = try #require(
        NSBitmapImageRep(
            bitmapDataPlanes: nil, pixelsWide: Int(view.bounds.width * 2),
            pixelsHigh: Int(view.bounds.height * 2), bitsPerSample: 8, samplesPerPixel: 4,
            hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
            bitsPerPixel: 0))
    rep.size = view.bounds.size
    view.cacheDisplay(in: view.bounds, to: rep)
    return rep
}

// The brightest ink in `rect` (points, from the view's top left): a text's own color, where its
// strokes cover whole pixels.
func brightest(in rect: NSRect, of rep: NSBitmapImageRep) -> CGFloat {
    var best: CGFloat = 0
    for x in Int(rect.minX * 2)..<Int(rect.maxX * 2) {
        for y in Int(rect.minY * 2)..<Int(rect.maxY * 2) {
            guard let color = rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB),
                color.alphaComponent > 0.99
            else {
                continue
            }
            best = max(best, min(color.redComponent, color.greenComponent, color.blueComponent))
        }
    }
    return best
}

// Where `text`'s glyphs are drawn in `view`, from its top left.
@MainActor
func glyphs(of text: String, in textView: NSTextView, within view: NSView) throws -> NSRect {
    let range = (textView.string as NSString).range(of: text)
    try #require(range.location != NSNotFound)
    let layout = try #require(textView.layoutManager)
    let container = try #require(textView.textContainer)
    let glyphRange = layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil)
    let bounds = layout.boundingRect(forGlyphRange: glyphRange, in: container)
        .offsetBy(dx: textView.textContainerOrigin.x, dy: textView.textContainerOrigin.y)
    let converted = textView.convert(bounds, to: view)
    return view.isFlipped
        ? converted
        : NSRect(
            x: converted.minX, y: view.bounds.height - converted.maxY, width: converted.width,
            height: converted.height)
}

@MainActor
struct ThreadViewTests {
    let list = MessageTableTests()
    let noon = berlinTime(2026, 10, 10, 12, 0)
    let ray = author(2, "Ray")

    init() {
        list.window.appearance = NSAppearance(named: .darkAqua)
    }

    func start(_ name: ChannelName) throws -> ChannelStartCell {
        list.show(
            MessageTableState(rows: [row(1, by: ray, at: noon)], reachedOldest: true, start: name.start))
        return try #require(try list.cell(at: 0) as? ChannelStartCell)
    }

    func shownTexts(_ cell: NSView) -> [NSTextView] {
        list.messageTexts(in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }
    }

    // A thread started from Ray's message: its first message, then Ray's reply a minute later.
    func starterRows(_ starter: MessageListModel.Row.Starter) -> [MessageListModel.Row] {
        let first = row(31, by: ray, at: noon, kind: .threadStarterMessage, content: "")
        return [
            MessageListModel.Row(id: first.id, message: first.message, starter: starter),
            row(32, by: ray, at: noon + 60, content: "hhh"),
        ]
    }

    func original() -> Message {
        row(20, by: ray, at: noon - 600, content: "the original").message
    }

    @Test
    func everyThreadStartsWithItsNameAndOwner() throws {
        let t1 = ChannelName(kind: .thread(owner: "Ray"), name: "t1")
        #expect(
            t1.start
                == ChannelStart(
                    icon: .symbol("text.bubble"), title: "t1", subtitle: "Started by ",
                    subtitleName: "Ray"))

        let cell = try start(t1)
        let texts = shownTexts(cell)
        #expect(texts.map(\.string) == ["t1", "Started by Ray"])
        let subtitle = try #require(texts.last)
        let storage = try #require(subtitle.textStorage)
        let owner = (storage.string as NSString).range(of: "Ray")
        let attribute = { (key: NSAttributedString.Key, at: Int) in
            storage.attribute(key, at: at, effectiveRange: nil)
        }
        #expect(attribute(.font, owner.location) as? NSFont == TypeScale.startOwner.nsFont)
        #expect(attribute(.foregroundColor, owner.location) as? NSColor == Palette.textStrong)
        #expect(attribute(.font, 0) as? NSFont == TypeScale.startSubtitle.nsFont)
        #expect(attribute(.foregroundColor, 0) as? NSColor == Palette.textDefault)
        #expect(list.table.rect(ofRow: 0).height == list.fittingHeight(ofRow: 0))

        // Measured at 2x: the owner's ink is the strong white, "Started by" the message text's.
        let rep = try renderedAt2x(cell)
        let ownerInk = brightest(in: try glyphs(of: "Ray", in: subtitle, within: cell), of: rep)
        let restInk = brightest(
            in: try glyphs(of: "Started by", in: subtitle, within: cell), of: rep)
        #expect(ownerInk > 0.97, "\(ownerInk)")
        #expect(restInk < 0.96 && restInk > 0.9, "\(restInk)")
    }

    @Test
    func aThreadWithoutAKnownOwnerLeavesTheLineOut() throws {
        let unknown = ChannelName(kind: .thread(owner: nil), name: "t1")
        #expect(unknown.start?.subtitle == nil)
        let known = try start(ChannelName(kind: .thread(owner: "Ray"), name: "t1"))
        let withOwner = list.table.rect(ofRow: 0).height
        _ = known

        let cell = try start(unknown)

        #expect(shownTexts(cell).map(\.string) == ["t1"])
        #expect(list.table.rect(ofRow: 0).height < withOwner)
        #expect(list.table.rect(ofRow: 0).height == list.fittingHeight(ofRow: 0))
    }

    @Test
    func aStarterMessageShowsAsTheOriginalMessage() throws {
        list.show(starterRows(.loaded(original())))

        let starter = try #require(try list.cell(of: 31) as? MessageCell)
        #expect(starter.showsHeader)
        let texts = list.shownTexts(in: starter)
        #expect(texts.contains("Ray"))
        #expect(
            texts.contains(
                MessageFormat.groupTime(
                    noon - 600, now: berlinTime(2026, 10, 10, 15, 0), calendar: berlin,
                    locale: .autoupdatingCurrent)))
        #expect(texts.contains("the original"))
        #expect(list.table.rect(ofRow: list.index(of: 31)).height
            == list.fittingHeight(ofRow: list.index(of: 31)))
    }

    @Test
    func theMessageAfterTheStarterStartsItsOwnGroup() throws {
        list.show(starterRows(.loaded(original())))

        let reply = try #require(try list.cell(of: 32) as? MessageCell)
        #expect(reply.showsHeader)
        #expect(list.shownTexts(in: reply).contains("Ray"))
    }

    @Test
    func anUnavailableStarterSaysSorryWeCouldntLoadIt() throws {
        list.show(starterRows(.unavailable))

        let notice = try #require(try list.cell(of: 31) as? NoticeCell)
        let line = try #require(shownTexts(notice).first?.string)
        #expect(line.hasPrefix("Sorry, we couldn't load the first message in this thread"))
        #expect(
            line.hasSuffix(
                MessageFormat.groupTime(
                    noon, now: berlinTime(2026, 10, 10, 15, 0), calendar: berlin,
                    locale: .autoupdatingCurrent)))
        #expect(NoticeCell.symbol(.threadStarterMessage) == "text.bubble")
    }

    @Test
    func aLoadingStarterHasNoRow() throws {
        list.show(starterRows(.loading))

        #expect(list.index(of: 31) == -1)
        #expect(try list.cell(of: 32) is MessageCell)
    }
}

// The header of a thread opened full width: "# new › [thread] t1", the parent a link.
@MainActor
struct ThreadHeaderTests {
    let parent = Channel(
        id: ChannelId(rawValue: 10), kind: .guildText, guildId: GuildId(rawValue: 1),
        parentId: nil, name: "new", position: 0, topic: nil, nsfw: false, rateLimitPerUser: 0,
        recipientIds: [], thread: nil)
    var thread: Channel {
        Channel(
            id: ChannelId(rawValue: 20), kind: .publicThread, guildId: GuildId(rawValue: 1),
            parentId: ChannelId(rawValue: 10), name: "t1", position: 0, topic: nil, nsfw: false,
            rateLimitPerUser: 0, recipientIds: [],
            thread: ThreadInfo(
                joined: true, archived: false, locked: false, private: false, messageCount: nil,
                ownerId: nil, lastMessageId: nil, createdAt: nil))
    }

    // Where the first strong-white ink starts, across the header's middle: the open channel's
    // name ("t1"); the symbols and the parent's name are dimmer.
    func nameStart(_ header: ChannelHeader) throws -> CGFloat? {
        let image = try Rendered(
            header.background(Color(nsColor: Palette.chat)).ignoresSafeArea(),
            size: NSSize(width: 600, height: ChannelHeader.height + 1))
        let columns = 0..<Int(image.width * image.scale)
        let rows = Int(16 * image.scale)..<Int(32 * image.scale)
        return columns.first { x in rows.contains { isBright(image.pixel(x, $0)) } }
            .map { CGFloat($0) / image.scale }
    }

    func interactiveInk(_ header: ChannelHeader, in xs: Range<CGFloat>) throws -> Int {
        let image = try Rendered(
            header.background(Color(nsColor: Palette.chat)).ignoresSafeArea(),
            size: NSSize(width: 600, height: ChannelHeader.height + 1))
        let dim = { (color: NSColor?) -> Bool in
            guard let color else {
                return false
            }
            let level = min(color.redComponent, color.greenComponent, color.blueComponent)
            return level > 0.5 && level < 0.8
        }
        return count(image, in: xs, where: dim)
    }

    @Test
    func theHeaderShowsTheParentAndTheThread() throws {
        let alone = try #require(try nameStart(ChannelHeader(channel: thread, name: "t1")))
        let withParent = try #require(
            try nameStart(ChannelHeader(channel: thread, name: "t1", parent: parent)))

        // "# new ›" before the thread's symbol pushes its name right.
        #expect(alone < 50, "\(alone)")
        #expect(withParent > alone + 40, "\(withParent)")
        #expect(
            try interactiveInk(
                ChannelHeader(channel: thread, name: "t1", parent: parent), in: 40..<(withParent - 30))
                > 20)
    }

    // The full view's "…" with Join Thread or Leave Thread (D119); a channel's header has none yet.
    @Test
    func aThreadsHeaderHasItsMoreMenu() throws {
        func ink(_ header: ChannelHeader) throws -> Int {
            let image = try Rendered(
                header.background(Color(nsColor: Palette.chat)).ignoresSafeArea(),
                size: NSSize(width: 600, height: ChannelHeader.height + 1))
            let lit = { (color: NSColor?) -> Bool in
                (color?.brightnessComponent ?? 0) > 0.4
            }
            return count(image, in: 560..<600, where: lit)
        }
        let leave = ChannelHeader.Membership(
            item: SessionModel.MembershipItem(joins: false, enabled: true), act: {})

        #expect(try ink(ChannelHeader(channel: thread, name: "t1", membership: leave)) > 10)
        #expect(try ink(ChannelHeader(channel: thread, name: "t1")) == 0)
    }

    // The header's link shows the pointing hand through a cursor rect, which can't outlive it.
    @Test
    func thePointingHandAreaLetsClicksThrough() {
        let area = PointingHandArea.Area(frame: NSRect(x: 0, y: 0, width: 40, height: 20))

        #expect(area.hitTest(NSPoint(x: 10, y: 10)) == nil)
    }

    @Test
    func aChannelsHeaderHasNoParent() throws {
        let plain = try nameStart(ChannelHeader(channel: parent, name: "new"))
        let given = try nameStart(ChannelHeader(channel: parent, name: "new", parent: parent))

        #expect(plain == given)
    }
}

@MainActor
struct LockedBannerTests {
    let noon = berlinTime(2026, 10, 10, 12, 0)

    // From the cell's top to its icon's.
    func iconTop(_ cell: ChannelStartCell) -> CGFloat {
        cell.layoutSubtreeIfNeeded()
        return cell.isFlipped ? cell.icon.frame.minY : cell.bounds.height - cell.icon.frame.maxY
    }

    // The banner covers the list's top 4–36; a locked thread's start keeps its icon below it.
    @Test
    func aLockedThreadsStartSitsBelowTheBanner() throws {
        let list = MessageTableTests()
        let start = ChannelName(kind: .thread(owner: "Ray"), name: "t1").start
        let rows = [row(1, by: author(2, "Ray"), at: noon)]
        list.show(MessageTableState(rows: rows, reachedOldest: true, start: start))
        let open = iconTop(try #require(try list.cell(at: 0) as? ChannelStartCell))

        list.show(
            MessageTableState(
                rows: rows, reachedOldest: true, start: start, startRoom: LockedBanner.room))
        let locked = try #require(try list.cell(at: 0) as? ChannelStartCell)

        #expect(iconTop(locked) == open + LockedBanner.room)
        #expect(iconTop(locked) >= LockedBanner.margin + LockedBanner.height + 16)
        #expect(list.table.rect(ofRow: 0).height == list.fittingHeight(ofRow: 0))
    }

    // MessageArea's part: a locked thread's composer brings the banner and the denied composer.
    @Test
    func aLockedThreadsAreaShowsTheBannerOverTheListAndDisablesTheComposer() throws {
        let screen = SessionScreenTests()
        let session = screen.formattedSession(FormattedAccount(contents: ["test"], locked: true))
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screen.shownWindow(host)
        window.appearance = NSAppearance(named: .darkAqua)
        defer { window.orderOut(nil) }
        host.layoutSubtreeIfNeeded()
        let table = try #require(screen.views(MessageTableView.self, in: host).first)
        let list = try #require(table.enclosingScrollView)
        let area = list.convert(list.bounds, to: host)
        let rep = try renderedAt2x(host)

        // Inside the banner's left padding, at its middle.
        let x = Int((area.minX + 16 + 4) * 2)
        let y = Int((area.minY + LockedBanner.margin + LockedBanner.height / 2) * 2)
        var card: NSColor?
        NSAppearance(named: .darkAqua)?.performAsCurrentDrawingAppearance {
            card = Palette.card.usingColorSpace(.sRGB)
        }
        #expect(same(rep.colorAt(x: x, y: y)?.usingColorSpace(.sRGB), card))
        let composer = try #require(screen.views(ComposerTextView.self, in: host).first)
        #expect(!composer.isEditable)
        let controller = try #require(table.delegate as? MessageTableController)
        #expect(controller.state.startRoom == LockedBanner.room)
    }

    // Q38's wording, a gray bar at the top of the list.
    @Test
    func aLockedThreadShowsTheBannerAndDisablesTheComposer() throws {
        #expect(
            LockedBanner.text == "This thread has been locked. Only moderators can send messages.")
        let image = try Rendered(
            LockedBanner().frame(width: 600).background(Color(nsColor: Palette.chat))
                .ignoresSafeArea(),
            size: NSSize(width: 600, height: 60))

        // Inside its left padding, at its middle: the banner's gray, not the chat's.
        let fill = image.color(16 + 4, LockedBanner.margin + LockedBanner.height / 2)
        let outside = image.color(4, LockedBanner.margin + LockedBanner.height / 2)
        #expect(!same(fill, outside))
        #expect(same(image.color(4, 2), outside))
        #expect(isBright(fill) == false)
    }
}
