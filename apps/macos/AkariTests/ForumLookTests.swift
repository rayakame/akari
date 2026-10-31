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

    func session(unread: Set<Int> = []) -> SessionModel {
        var fakes = ForumFakes()
        fakes.untagged = true
        fakes.posts = 3
        fakes.unread = unread
        return fakes.session()
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
