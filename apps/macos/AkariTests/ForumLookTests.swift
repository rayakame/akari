import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The list page as Discord's showed in Chris's comparison (2026-10-31), measured to the ink in
// points; unread posts bright, read ones muted.
@MainActor
struct ForumLookTests {
    static let width: CGFloat = 700

    func session(
        unread: Set<Int> = [], firstMessage: String? = nil, new: Set<Int> = [], firstTitle: String? = nil
    ) -> SessionModel {
        var fakes = ForumFakes()
        fakes.untagged = true
        fakes.posts = 3
        fakes.unread = unread
        fakes.firstMessage = firstMessage
        fakes.new = new
        fakes.firstTitle = firstTitle
        return fakes.session()
    }

    // The box (points) of the pixels in Discord's badge blurple between rows `top` and `bottom`.
    func pill(_ image: Rendered, from top: CGFloat, to bottom: CGFloat) -> NSRect? {
        var (left, right, first, last) = (Int.max, -1, Int.max, -1)
        for row in Int(top * image.scale)..<Int(bottom * image.scale) {
            for column in 0..<image.rep.pixelsWide where blurple(image.pixel(column, row)) {
                (left, right) = (min(left, column), max(right, column))
                (first, last) = (min(first, row), max(last, row))
            }
        }
        guard right >= 0 else {
            return nil
        }
        return NSRect(
            x: CGFloat(left) / image.scale, y: CGFloat(first) / image.scale,
            width: CGFloat(right - left + 1) / image.scale, height: CGFloat(last - first + 1) / image.scale)
    }

    // A saturated blue: the badge's fill, whatever the window's display makes of it.
    func blurple(_ color: NSColor?) -> Bool {
        guard let color else {
            return false
        }
        return color.blueComponent > 0.8 && color.blueComponent - color.redComponent > 0.35
            && color.blueComponent - color.greenComponent > 0.3
    }

    // The bottom (points) of the white letters inside `box`.
    func whiteBottom(_ image: Rendered, in box: NSRect) -> CGFloat? {
        let columns = Int(box.minX * image.scale)..<Int(box.maxX * image.scale)
        let rows = Int(box.minY * image.scale)..<Int(box.maxY * image.scale)
        let last = rows.last { row in
            columns.contains { column in
                guard let pixel = image.pixel(column, row) else {
                    return false
                }
                return pixel.brightnessComponent > 0.9 && pixel.saturationComponent < 0.25
            }
        }
        return last.map { CGFloat($0 + 1) / image.scale }
    }

    // The columns (points) of bright text between rows `top` and `bottom`, left of `x1`.
    func inkRight(_ image: Rendered, x1: CGFloat, from top: CGFloat, to bottom: CGFloat) -> CGFloat {
        var right = 0
        for row in Int(top * image.scale)..<Int(bottom * image.scale) {
            for column in 0..<Int(x1 * image.scale)
            where (image.pixel(column, row)?.brightnessComponent ?? 0) > 0.6
                && (image.pixel(column, row)?.saturationComponent ?? 1) < 0.2
            {
                right = max(right, column)
            }
        }
        return CGFloat(right + 1) / image.scale
    }

    // Chris's comparison (2026-11-11), Discord's badge: a round pill 16 tall with 8 inside, white
    // 12 semibold capitals on #5966f2, 8 after the title, its text 2 above the title's baseline.
    @Test
    func aNewPostHasThePillAfterItsTitle() throws {
        let plain = try page(session(unread: [0]))
        let image = try page(session(unread: [0], new: [0]))
        let background = image.color(4, 4)
        let cards = Array(boxes(image, x: 50, background: background).dropFirst(2))
        let usual = Array(boxes(plain, x: 50, background: background).dropFirst(2))
        try #require(cards.count >= 2 && usual.count >= 2)
        #expect(cards[0].bottom - cards[0].top == usual[0].bottom - usual[0].top, "the card's height")
        #expect(pill(plain, from: usual[0].top, to: usual[0].bottom) == nil)
        #expect(pill(image, from: cards[1].top, to: cards[1].bottom) == nil, "only the new post")

        let badge = try #require(pill(image, from: cards[0].top, to: cards[0].bottom))
        let title = try #require(
            inkRuns(plain, x0: 26, x1: 400, from: usual[0].top + 2, to: usual[0].bottom - 2).first)
        let label = ("NEW" as NSString).size(withAttributes: [.font: TypeScale.forumBadge.nsFont]).width
        #expect(abs(badge.height - 16) <= 0.75, "\(badge)")
        #expect(abs(badge.width - (label.rounded(.up) + 16)) <= 0.5, "\(badge) for \(label)")
        let titleEnd = inkRight(plain, x1: 600, from: title.top, to: title.bottom)
        #expect((7...10.5).contains(badge.minX - titleEnd), "after the title: \(badge.minX - titleEnd)")
        // The fake's title has no descenders: its ink ends on the baseline. Discord's pill ends
        // 1.5 under it, its capitals 2 above it.
        #expect((1...2.25).contains(badge.maxY - title.bottom), "under the baseline: \(badge.maxY - title.bottom)")
        let capitals = try #require(whiteBottom(image, in: badge))
        #expect((1.25...2.75).contains(title.bottom - capitals), "the capitals: \(title.bottom - capitals)")
        // Round ends: the box's corner is the card, its middle the fill; white letters inside.
        #expect(!blurple(image.color(badge.minX + 0.5, badge.minY + 0.5)))
        // #5966f2, as a swatch of it draws in the same window.
        let swatch = try Rendered(
            Color(nsColor: NSColor(hex: "#5966f2")).frame(width: 20, height: 20),
            size: NSSize(width: 20, height: 20), scale: 2)
        #expect(near(image.color(badge.minX + 4, badge.midY), swatch.color(10, 10)))
        #expect(brightest(image, x0: badge.minX + 8, x1: badge.maxX - 8, y0: badge.minY, y1: badge.maxY) > 0.97)
    }

    // A title that fills its two lines keeps them: the pill stands at its right, 4 down.
    @Test
    func aTitleFillingTwoLinesHasThePillAtItsRight() throws {
        let long = String(repeating: "A long title that goes on and on ", count: 6)
        let plain = try page(session(unread: [0], firstTitle: long))
        let image = try page(session(unread: [0], new: [0], firstTitle: long))
        let background = image.color(4, 4)
        let card = try #require(boxes(image, x: 50, background: background).dropFirst(2).first)
        let usual = try #require(boxes(plain, x: 50, background: background).dropFirst(2).first)
        #expect(card.bottom - card.top == usual.bottom - usual.top, "two lines, as without the pill")

        let badge = try #require(pill(image, from: card.top, to: card.bottom))
        let lines = inkRuns(image, x0: 26, x1: 400, from: card.top + 2, to: card.bottom - 2)
        try #require(lines.count == 4, "two title lines, the preview, the footer: \(lines)")
        let label = ("NEW" as NSString).size(withAttributes: [.font: TypeScale.forumBadge.nsFont]).width
        #expect(abs(badge.height - 16) <= 0.75, "\(badge)")
        #expect(abs(badge.width - (label.rounded(.up) + 16)) <= 0.5, "the same pill as in the text: \(badge)")
        #expect(badge.minY >= lines[0].top - 6 && badge.maxY <= lines[1].top + 4, "at the top: \(badge)")
        let titleEnd = inkRight(image, x1: badge.minX - 1, from: lines[0].top, to: lines[1].bottom)
        #expect(badge.minX - titleEnd >= 7, "right of the title: \(badge.minX - titleEnd)")
        #expect(badge.maxX <= Self.width - 16 - 10, "inside the card: \(badge)")
    }

    // A title that leaves the pill no room on its line: the pill takes the next, a line as tall
    // as the title's.
    @Test
    func aPillAloneOnTheSecondLineKeepsTheLinesHeight() throws {
        // The card's text is 648 wide at this page width; the pill needs 53 of it.
        var words = "Wide"
        let font = TypeScale.forumTitle.nsFont
        while (words as NSString).size(withAttributes: [.font: font]).width < 615 {
            words += " wide"
        }
        let twoLines = try page(session(unread: [0], firstTitle: words + " and then some more words"))
        let image = try page(session(unread: [0], new: [0], firstTitle: words))
        let background = image.color(4, 4)
        let card = try #require(boxes(image, x: 50, background: background).dropFirst(2).first)
        let usual = try #require(boxes(twoLines, x: 50, background: background).dropFirst(2).first)

        let badge = try #require(pill(image, from: card.top, to: card.bottom))
        try #require(badge.minX < 60, "the pill wrapped: \(badge)")
        #expect(card.bottom - card.top == usual.bottom - usual.top, "as a card with a two-line title")
    }

    // Without an author (the first message still loading) the pill moves nothing either.
    @Test
    func aNewGalleryCardsTitleStaysWhereItIs() throws {
        var fakes = ForumFakes()
        fakes.untagged = true
        let plain = try #require(fakes.session().forum)
        fakes.new = [4]
        let new = try #require(fakes.session().forum)
        let size = CGSize(width: 320, height: ForumMeasures.galleryCardHeight(width: 320))
        func lines(_ forum: ForumModel) throws -> [(top: CGFloat, bottom: CGFloat)] {
            let image = try Rendered(
                ForumGalleryCard(card: forum.posts[4], filter: [], size: size, now: ForumFakes.now) {}
                    .frame(width: size.width, height: size.height),
                size: NSSize(width: size.width, height: size.height), scale: 2)
            return inkRuns(image, x0: 14, x1: 200, from: 2, to: 58)
        }
        try #require(new.posts[4].isNew && new.posts[4].preview == .loading)

        let (usual, shown) = (try lines(plain), try lines(new))

        try #require(usual.count == 2 && shown.count == 2, "\(usual) \(shown)")
        #expect(shown[1].top == usual[1].top && shown[0].top == usual[0].top, "\(shown) for \(usual)")
    }

    @Test
    func aNewCardSaysSo() throws {
        let forum = try #require(session(new: [0]).forum)

        #expect(ForumListCard.label(forum.posts[0]).hasSuffix(", new"))
        #expect(!ForumListCard.label(forum.posts[1]).contains("new"))
    }

    // The gallery's pill ends the author's row, at its top.
    @Test
    func aNewGalleryCardHasThePillInItsAuthorRow() throws {
        let forum = try #require(session(unread: [0], new: [0]).forum)
        let size = CGSize(width: 320, height: ForumMeasures.galleryCardHeight(width: 320))
        func card(_ index: Int) throws -> Rendered {
            try Rendered(
                ForumGalleryCard(card: forum.posts[index], filter: [], size: size, now: ForumFakes.now) {}
                    .frame(width: size.width, height: size.height),
                size: NSSize(width: size.width, height: size.height), scale: 2)
        }

        let badge = try #require(pill(try card(0), from: 0, to: size.height))
        #expect(pill(try card(1), from: 0, to: size.height) == nil)
        #expect(abs(badge.height - 16) <= 0.75, "\(badge)")
        #expect(abs(badge.maxX - (size.width - 12)) <= 1, "at the row's end: \(badge)")
        #expect(abs(badge.minY - 12) <= 0.5, "at the row's top: \(badge)")
    }

    // Rows (points) where the column at `x` isn't the page's color: the boxes it crosses.
    func boxes(_ image: Rendered, x: CGFloat, background: NSColor?) -> [(top: CGFloat, bottom: CGFloat)] {
        let column = Int(x * image.scale)
        var found: [(top: CGFloat, bottom: CGFloat)] = []
        var start: Int?
        for row in 0..<image.rep.pixelsHigh {
            let inside = !near(image.pixel(column, row), background)
            if inside, start == nil {
                start = row
            } else if !inside, let top = start {
                found.append((CGFloat(top) / image.scale, CGFloat(row) / image.scale))
                start = nil
            }
        }
        return found
    }

    func near(_ color: NSColor?, _ other: NSColor?) -> Bool {
        guard let color, let other else {
            return false
        }
        return abs(color.redComponent - other.redComponent) < 2.5 / 255
            && abs(color.greenComponent - other.greenComponent) < 2.5 / 255
            && abs(color.blueComponent - other.blueComponent) < 2.5 / 255
    }

    // Rows (points) of text in a card: brighter than its fill by a margin, between `x0` and `x1`.
    func inkRuns(_ image: Rendered, x0: CGFloat, x1: CGFloat, from top: CGFloat, to bottom: CGFloat)
        -> [(top: CGFloat, bottom: CGFloat)]
    {
        let fill = image.color(x0 - 4, top + 4)
        var found: [(top: CGFloat, bottom: CGFloat)] = []
        var start: Int?
        let columns = Int(x0 * image.scale)..<Int(x1 * image.scale)
        for row in Int(top * image.scale)..<Int(bottom * image.scale) {
            let inked = columns.contains { column in
                guard let pixel = image.pixel(column, row), let fill else {
                    return false
                }
                return pixel.brightnessComponent - fill.brightnessComponent > 0.25
            }
            if inked, start == nil {
                start = row
            } else if !inked, let first = start {
                found.append((CGFloat(first) / image.scale, CGFloat(row) / image.scale))
                start = nil
            }
        }
        return found
    }

    func page(_ session: SessionModel, scale: CGFloat = 2) throws -> Rendered {
        let forum = try #require(session.forum)
        return try Rendered(
            ForumPage(forum: forum, session: session, now: ForumFakes.now)
                .frame(width: Self.width, height: 700),
            size: NSSize(width: Self.width, height: 700), scale: scale)
    }

    // At 1× (CI's runner) edges round to whole points.
    @Test(arguments: [1, 2] as [CGFloat])
    func cardsAre16ApartAndThePill19AboveThem(scale: CGFloat) throws {
        let image = try page(session(), scale: scale)
        let near = scale == 1 ? 1 : 0.5
        let background = image.color(4, 4)

        // Left of Sort & View's text, right of its round end; the search field and cards cross it.
        let crossed = boxes(image, x: 50, background: background)
        try #require(crossed.count >= 5, "\(crossed)")
        let (search, pill, first, second, third) = (crossed[0], crossed[1], crossed[2], crossed[3], crossed[4])
        let toPill = pill.top - search.bottom
        let toCard = first.top - pill.bottom
        let gap = second.top - first.bottom
        #expect(abs(toPill - 16) <= near, "search to pill \(toPill)")
        #expect(abs(pill.bottom - pill.top - 32) <= near, "pill \(pill.bottom - pill.top)")
        #expect(abs(toCard - 19) <= near, "pill to card \(toCard)")
        #expect(abs(gap - 16) <= near, "card gap \(gap)")
        #expect(abs(third.top - second.bottom - 16) <= near)
        #expect(abs(first.bottom - first.top - 102) <= near, "card \(first.bottom - first.top)")
    }

    // Discord's secondary control: a 12 % fill and a 1 pt border on the page.
    @Test(arguments: [1, 2] as [CGFloat])
    func sortAndViewHasAFillAndABorder(scale: CGFloat) throws {
        let image = try page(session(), scale: scale)
        let background = try #require(image.color(4, 4))
        let pill = try #require(boxes(image, x: 50, background: background).dropFirst().first)

        let fill = try #require(image.color(50, pill.top + 3))
        let edge = try #require(image.color(50, pill.top + 0.5 / scale))
        #expect(fill.brightnessComponent - background.brightnessComponent > 0.03, "\(fill)")
        #expect(edge.brightnessComponent > fill.brightnessComponent + 0.01, "\(edge) over \(fill)")
    }

    // Title, preview and footer spread over the card as Discord's, its footer 14 above its bottom.
    @Test(arguments: [1, 2] as [CGFloat])
    func aCardsLinesSpreadAsDiscords(scale: CGFloat) throws {
        let image = try page(session(), scale: scale)
        let background = image.color(4, 4)
        let card = try #require(boxes(image, x: 50, background: background).dropFirst(2).first)

        let lines = inkRuns(image, x0: 26, x1: 400, from: card.top + 2, to: card.bottom - 2)

        try #require(lines.count == 3, "\(lines)")
        let (title, preview, footer) = (lines[0], lines[1], lines[2])
        #expect((16.5...19.5).contains(title.top - card.top), "title \(title.top - card.top)")
        #expect((26...30).contains(preview.top - title.top), "title to preview \(preview.top - title.top)")
        #expect((25.5...29).contains(footer.top - preview.top), "preview to footer \(footer.top - preview.top)")
        #expect((12.5...16).contains(card.bottom - footer.bottom), "footer to bottom \(card.bottom - footer.bottom)")
    }

    // Chris's comparison (2026-11-11): a first message of only emoji sits on the preview's line
    // beside "Ray:", at the text's size, clear of the title and the footer.
    @Test(arguments: ["🗿", "<:pog:5>", "🗿🗿 <:pog:5>"])
    func anEmojiOnlyPreviewStaysOnItsLine(content: String) throws {
        let plain = try page(session())
        let image = try page(session(firstMessage: content))
        let background = image.color(4, 4)
        let card = try #require(boxes(image, x: 50, background: background).dropFirst(2).first)
        let usual = try #require(boxes(plain, x: 50, background: background).dropFirst(2).first)

        let expected = inkRuns(plain, x0: 26, x1: 400, from: usual.top + 2, to: usual.bottom - 2)
        let lines = inkRuns(image, x0: 26, x1: 400, from: card.top + 2, to: card.bottom - 2)

        try #require(expected.count == 3 && lines.count == 3, "\(lines)")
        #expect(abs(lines[0].bottom - expected[0].bottom) <= 0.5, "the title's line \(lines[0])")
        // The system's emoji stands 18 tall at this size, as Discord's: 3.5 over the capitals.
        #expect(lines[1].top >= expected[1].top - 4, "the preview's top \(lines[1]) for \(expected[1])")
        #expect(lines[1].bottom <= expected[1].bottom + 3, "the preview's bottom \(lines[1]) for \(expected[1])")
        #expect(abs(lines[2].top - expected[2].top) <= 0.5, "the footer's line \(lines[2])")
    }

    func brightest(_ image: Rendered, x0: CGFloat, x1: CGFloat, y0: CGFloat, y1: CGFloat) -> CGFloat {
        var most: CGFloat = 0
        for row in Int(y0 * image.scale)..<Int(y1 * image.scale) {
            for column in Int(x0 * image.scale)..<Int(x1 * image.scale) {
                most = max(most, image.pixel(column, row)?.brightnessComponent ?? 0)
            }
        }
        return most
    }

    // Discord's unread look: an unread post's title and preview bright, a read one's muted.
    @Test(arguments: [1, 2] as [CGFloat])
    func anUnreadCardIsBrightAndAReadOneMuted(scale: CGFloat) throws {
        let image = try page(session(unread: [0]), scale: scale)
        let background = image.color(4, 4)
        let cards = Array(boxes(image, x: 50, background: background).dropFirst(2))
        try #require(cards.count >= 2)

        for (index, card) in cards.prefix(2).enumerated() {
            let lines = inkRuns(image, x0: 26, x1: 400, from: card.top + 2, to: card.bottom - 2)
            try #require(lines.count == 3, "\(lines)")
            let title = brightest(image, x0: 26, x1: 400, y0: lines[0].top, y1: lines[0].bottom)
            let preview = brightest(image, x0: 26, x1: 400, y0: lines[1].top, y1: lines[1].bottom)
            if index == 0 {
                #expect(title > 0.95, "unread title \(title)")
                #expect(preview > 0.9, "unread preview \(preview)")
            } else {
                // Muted gray text renders a little lighter than its color (0.62): 0.68.
                #expect(title < 0.72, "read title \(title)")
                #expect(preview < 0.72, "read preview \(preview)")
            }
        }
    }

    // VoiceOver hears what the bright title shows.
    @Test
    func anUnreadCardSaysSo() throws {
        let forum = try #require(session(unread: [0]).forum)

        #expect(ForumListCard.label(forum.posts[0]).hasSuffix(", unread"))
        #expect(!ForumListCard.label(forum.posts[1]).contains("unread"))
    }

    // The gallery's author keeps Discord's 14 (text-sm/semibold); only the list's message is 15.
    @Test
    func theGallerysAuthorStays14() {
        #expect(TypeScale.forumGalleryAuthor == TextStyle(14, .semibold, line: 18))
    }

    // Discord's filled forum symbol, the same in the sidebar and the header, and a filled bubble
    // for the reply count.
    @Test
    func theForumAndItsRepliesHaveFilledSymbols() {
        #expect(ChannelSymbols.name(for: .guildForum) == "bubble.left.and.bubble.right.fill")
        #expect(ForumCardCount.symbol == "bubble.left.fill")
    }

    // The header's channel symbol in the header icons' gray: the Threads button's at rest.
    @Test
    func theHeadersSymbolIsTheHeaderIconsGray() throws {
        let session = session()
        let forum = try #require(session.forum?.channel)
        let image = try Rendered(
            ChannelHeader(
                channel: forum, name: "forum-test", threads: ThreadsButton(selected: false) {}
            )
            .frame(width: 300, height: ChannelHeader.height + 1)
            .background(Color(nsColor: Palette.chat)),
            size: NSSize(width: 300, height: ChannelHeader.height + 1))

        let symbol = brightest(image, x0: 14, x1: 38, y0: 10, y1: 38)
        let threads = brightest(image, x0: 240, x1: 300, y0: 10, y1: 38)
        #expect(abs(symbol - threads) < 0.02, "\(symbol) vs \(threads)")
    }
}

extension Rendered {
    // In a plain window at `scale`: CI's runner draws at 1×, a Retina Mac at 2×.
    init<Content: View>(_ view: Content, size: NSSize, scale: CGFloat) throws {
        let window = ScaledWindow.make(scale: scale)
        window.appearance = NSAppearance(named: .darkAqua)
        window.setContentSize(size)
        let host = NSHostingView(rootView: view)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        self.scale = CGFloat(rep.pixelsWide) / host.bounds.width
    }
}
