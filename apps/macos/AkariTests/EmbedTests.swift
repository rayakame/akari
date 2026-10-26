import AkariFFI
import AkariKit
import AppKit
import Testing

@testable import Akari

// A message's embeds (D145): Discord's card and its rows, media alone, galleries, the video pill,
// GIFVs, the link text hidden under its image, covers, and the list's clicks.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class EmbedTests {
    let list = MessageTableTests(onScreen: { _ in true })
    let actions = RecordingMediaActions()
    let cache = ImageCache(source: FakeImageSource.stalling())

    init() {
        list.controller.actions = actions
        list.window.setContentSize(NSSize(width: 900, height: 700))
        list.window.layoutIfNeeded()
    }

    func show(
        _ embeds: [Embed], content: String = "https://example.com/", hidesContent: Bool = false,
        files: [AkariFFI.Attachment] = [], revealed: Set<MediaKey> = [], shownSensitive: Set<MediaKey> = [],
        motion: MotionPolicy = .default, focused: Bool = true
    ) {
        list.show(
            MessageTableState(
                rows: [row(1, at: list.noon, content: content, files: files, embeds: embeds, hidesContent: hidesContent)],
                reachedOldest: true, revealedMedia: revealed, shownSensitive: shownSensitive,
                images: ImageCacheRef(cache), motion: motion, focused: focused))
    }

    func cell() throws -> MessageCell {
        try #require(try list.cell(of: 1) as? MessageCell)
    }

    func views() throws -> [EmbedView] {
        try #require(try cell().embeds).views.filter { !$0.isHidden }
    }

    func shown(_ embed: Embed) -> EmbedView.Shown {
        EmbedView.shown(
            embed, cover: nil, spoilersShown: false, revealed: [], now: list.noon,
            times: TimestampText(locale: Locale(identifier: "en_US"), timeZone: berlin.timeZone, calendar: berlin),
            footerTime: nil)
    }

    // MARK: Layout

    // Module 161573 and 659674: media alone fitted in 400 × 300 (450 tall for TikTok), never
    // scaled up; a video at least 150 × 144.
    @Test
    func mediaAloneFitsDiscordsBox() {
        let wide = embed(kind: .image, inline: true, title: nil, description: nil, media: [embedImage(1200, 630)])
        #expect(EmbedView.layout(shown(wide), width: 800).size == NSSize(width: 400, height: 210))
        let small = embed(kind: .image, inline: true, title: nil, description: nil, media: [embedImage(300, 200)])
        #expect(EmbedView.layout(shown(small), width: 800).size == NSSize(width: 300, height: 200))
        let tall = embed(
            kind: .video, inline: true, title: nil, description: nil,
            video: EmbedVideo(
                poster: embedImage(1080, 1920, slot: .thumbnail), width: 1080, height: 1920, proxied: false,
                key: .embedMedia(
                    channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .video)),
            tall: true)
        #expect(EmbedView.layout(shown(tall), width: 800).size == NSSize(width: 253, height: 450))
        #expect(EmbedView.layout(shown(wide), width: 200).size == NSSize(width: 200, height: 105), "scaled to the column")
    }

    // Two columns 4 apart: two side by side, three as one left and two right, four as 2 × 2.
    @Test
    func galleriesFollowDiscordsGrid() {
        #expect(
            EmbedView.gallery(2, width: 400, height: 300) == [
                NSRect(x: 0, y: 0, width: 198, height: 300), NSRect(x: 202, y: 0, width: 198, height: 300),
            ])
        #expect(
            EmbedView.gallery(3, width: 400, height: 300) == [
                NSRect(x: 0, y: 0, width: 198, height: 300), NSRect(x: 202, y: 0, width: 198, height: 148),
                NSRect(x: 202, y: 152, width: 198, height: 148),
            ])
        #expect(EmbedView.gallery(4, width: 400, height: 300).map(\.origin) == [
            NSPoint(x: 0, y: 0), NSPoint(x: 202, y: 0), NSPoint(x: 0, y: 152), NSPoint(x: 202, y: 152),
        ])
    }

    // The card's grid: 1 + 2 above the first row, 8 between rows, 4 + 12 to the text's left,
    // 16 + 1 on the right and below; as wide as its content, at most 516 inside.
    @Test
    func theCardIsDiscordsGrid() throws {
        let short = shown(embed(title: "Hi", description: nil))
        let layout = EmbedView.layout(short, width: 800)
        let title = try #require(layout.title)
        #expect(title.origin == NSPoint(x: 16, y: 11))
        #expect(layout.size.width == title.width + 4 + 1 + 12 + 16)
        #expect(layout.size.height == 11 + title.height + 16 + 1)

        let long = shown(embed(description: String(repeating: "word ", count: 200)))
        let widest: CGFloat = 516 - 28 + 33
        #expect(EmbedView.layout(long, width: 800).size.width == widest, "at most 516 inside")
        #expect(EmbedView.layout(long, width: 300).size.width == 300, "the column's width")
    }

    // A wide image (≥ 300 fitted) makes the card its width; the thumbnail sits right, 16 from the
    // text, 8 down; the image 16 under the text.
    @Test
    func mediaAndThumbnailTakeTheirPlaces() throws {
        let card = shown(embed(media: [embedImage(1200, 630)], thumbnail: embedImage(80, 80, slot: .thumbnail)))
        let layout = EmbedView.layout(card, width: 800)
        let media = try #require(layout.media)
        let thumbnail = try #require(layout.thumbnail)
        let description = try #require(layout.description)
        #expect(media.size == NSSize(width: 400, height: 210))
        let cardWidth: CGFloat = 400 + 33
        #expect(layout.size.width == cardWidth)
        #expect(thumbnail == NSRect(x: 16 + 400 - 96 + 16, y: 11, width: 80, height: 80))
        #expect(description.maxX == thumbnail.minX - 16)
        #expect(media.minY == max(description.maxY, thumbnail.maxY) + 16)
    }

    // Module 161573 `renderFields`: inline fields three to a row; one that isn't has its own.
    @Test
    func fieldsShareRowsOfThree() {
        let card = shown(
            embed(fields: [
                field("A", "1"), field("B", "2"), field("C", "3"), field("D", "4"),
                field("Long", "own row", inline: false), field("E", "5"),
            ]))
        let layout = EmbedView.layout(card, width: 800)
        let rows = Dictionary(grouping: layout.fields, by: \.name.minY).values.map(\.count).sorted()
        #expect(rows == [1, 1, 1, 3], "ABC, D, Long, E")
        let first = layout.fields.prefix(3).map(\.name.minX)
        #expect(first[1] > first[0] && first[2] > first[1], "side by side")
        #expect(layout.fields[0].value.minY == layout.fields[0].name.maxY + 2)
    }

    // MARK: In the list

    // Embeds come under the attachments, 4 apart; the row is as tall as its cell.
    @Test
    func embedsComeUnderTheAttachments() throws {
        show(
            [embed(), embed(1, kind: .image, inline: true, title: nil, description: nil, media: [embedImage(640, 480, embed: 1)])],
            files: [media(5)])
        let cell = try cell()
        let attachments = try #require(cell.attachments)
        let embeds = try #require(cell.embeds)
        let below = attachments.convert(attachments.bounds, to: cell)
        let rows = embeds.convert(embeds.bounds, to: cell)
        #expect(rows.minY >= below.maxY + 2 - 0.5 || rows.maxY <= below.minY - 2 + 0.5, "after the mosaic")
        let views = try views()
        #expect(views.count == 2)
        #expect(abs(views[1].frame.minY - views[0].frame.maxY - 4) < 0.5)
    }

    // Module 521981: one link under its one inline image hides the text.
    @Test
    func theLinkUnderItsImageIsHidden() throws {
        let image = embed(kind: .image, inline: true, title: nil, description: nil, media: [embedImage(640, 480)])
        show([image], hidesContent: true)
        let text = try #require(try cell().subviews(of: MessageTextView.self).first { $0.string.contains("example") })
        #expect(text.isHidden)
    }

    // A card's spoiler covers all of it; a click reveals it by its place.
    @Test
    func aSpoileredCardIsCoveredUntilClicked() throws {
        show([embed(cover: .spoiler)])
        let view = try #require(try views().first)
        #expect(view.textViews.allSatisfy { $0.isHidden }, "nothing of its text")
        let cover = try #require(view.subviews.compactMap { $0 as? MediaTileView }.first { !$0.isHidden && $0.coverShown == .spoiler })
        cover.performClick(nil)
        #expect(actions.revealed == [MediaKey(messageId: MessageId(rawValue: 1), embed: 0)])

        show([embed(cover: .spoiler)], revealed: [MediaKey(messageId: MessageId(rawValue: 1), embed: 0)])
        let revealed = try #require(try views().first)
        #expect(revealed.textViews.contains { !$0.isHidden })
    }

    // A flagged card is black under the warning; its eye shows it and, shown, hides it again.
    @Test
    func aSensitiveCardTogglesWithItsEye() throws {
        let key = MediaKey(messageId: MessageId(rawValue: 1), embed: 0)
        show([embed(cover: .sensitive)])
        let view = try #require(try views().first)
        #expect(view.textViews.allSatisfy { $0.isHidden })
        let cover = try #require(view.subviews.compactMap { $0 as? MediaTileView }.first { !$0.isHidden && $0.coverShown == .sensitive(shown: false) })
        cover.eye.performClick(nil)
        #expect(actions.toggled == [key])

        show([embed(cover: .sensitive)], shownSensitive: [key])
        let shown = try #require(try views().first)
        #expect(shown.textViews.contains { !$0.isHidden })
        #expect(!shown.eye.isHidden)
        shown.eye.performClick(nil)
        #expect(actions.toggled == [key, key])
    }

    // Waiting for Discord's scan: black, nothing to click, no text.
    @Test
    func aPendingCardWaitsUnderBlack() throws {
        show([embed(cover: .pending)])
        let view = try #require(try views().first)
        #expect(view.textViews.allSatisfy { $0.isHidden })
        #expect(view.subviews.compactMap { $0 as? MediaTileView }.contains { !$0.isHidden && $0.coverShown == .pending })
    }

    // Media alone takes the attachments' covers and clicks: a spoiler reveals by the embed's
    // place; an image opens its original; a card's thumbnail its own.
    @Test
    func mediaAloneCoversAndOpensAsAttachmentsDo() throws {
        let image = embedImage(640, 480)
        show([embed(kind: .image, inline: true, title: nil, description: nil, media: [image], cover: .spoiler)])
        let tile = try #require(try views().first?.tiles.first)
        #expect(tile.coverShown == .spoiler)
        tile.performClick(nil)
        #expect(actions.revealed == [MediaKey(messageId: MessageId(rawValue: 1), embed: 0)])

        show(
            [embed(kind: .image, inline: true, title: nil, description: nil, media: [image]),
             embed(1, thumbnail: embedImage(80, 80, embed: 1, slot: .thumbnail))])
        try #require(try views().first?.tiles.first).performClick(nil)
        let thumbnail = try #require(try views()[1].subviews.compactMap { $0 as? MediaTileView }.first { !$0.isHidden })
        thumbnail.performClick(nil)
        #expect(actions.opened == [image.key, embedImage(80, 80, embed: 1, slot: .thumbnail).key])
    }

    // The author's name links where Discord's does.
    @Test
    func theAuthorLinksOut() throws {
        show([embed(author: EmbedAuthor(name: "Akari", url: "https://example.com/akari", icon: nil))])
        let view = try #require(try views().first)
        let author = try #require(view.textViews.first { $0.string == "Akari" })
        let url = try #require(author.textStorage?.attribute(.link, at: 0, effectiveRange: nil) as? URL)
        author.onLink(url)
        #expect(actions.links.first?.0 == URL(string: "https://example.com/akari"))
        #expect(actions.links.first?.masked == false)
    }

    // A video's pill: Play and Open Link, both open the link (no inline player yet, D145).
    @Test
    func aVideosPillOpensItsLink() throws {
        let video = EmbedVideo(
            poster: embedImage(1280, 720, slot: .thumbnail), width: 1280, height: 720, proxied: false,
            key: .embedMedia(channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .video))
        show([embed(kind: .video, title: "A video", description: nil, url: "https://www.youtube.com/watch?v=x", video: video)])
        let view = try #require(try views().first)
        let pill = try #require(view.subviews.compactMap { $0 as? VideoPill }.first)
        #expect(!pill.isHidden)
        #expect(pill.frame.height == 48)
        pill.buttons[0].performClick(nil)
        #expect(actions.links.map(\.0) == [URL(string: "https://www.youtube.com/watch?v=x")!])
    }

    // A GIFV plays its video while GIFs play; its poster shows otherwise.
    @Test
    func aGifvLoopsWhileGifsPlay() async throws {
        actions.video = URL(fileURLWithPath: "/dev/null")
        let gifv = embed(
            kind: .gifv, inline: true, title: nil, description: nil,
            video: EmbedVideo(
                poster: embedImage(498, 280, slot: .thumbnail), width: 498, height: 280, proxied: true,
                key: .embedMedia(channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1), embed: 0, slot: .video)))
        show([gifv])
        let tile = try #require(try views().first?.tiles.first)
        try await eventually { !self.actions.videos.isEmpty }
        #expect(tile.isLooping)
        #expect(actions.videos == [gifv.video!.key])

        show([gifv], focused: false)
        #expect(!tile.isLooping, "the window isn't key")
    }

    // MARK: Text

    // Discord's footer time (module 58703).
    @Test
    func theFooterTimeReadsAsDiscords() {
        let now = berlinTime(2026, 10, 26, 15, 0)
        let locale = Locale(identifier: "en_US")
        let time = { MessageFormat.footerTime($0, now: now, calendar: berlin, locale: locale) }
        #expect(time(berlinTime(2026, 10, 26, 9, 5)) == "Today at 9:05\u{202F}AM")
        #expect(time(berlinTime(2026, 10, 25, 23, 59)) == "Yesterday at 11:59\u{202F}PM")
        #expect(time(berlinTime(2026, 10, 27, 8, 0)) == "Tomorrow at 8:00\u{202F}AM")
        #expect(time(berlinTime(2026, 10, 30, 8, 0)) == "10/30/2026")
        #expect(time(berlinTime(2026, 10, 20, 8, 0)).hasPrefix("10/20/26"))
    }

    // A rich embed's title is semibold, a link's in the link color; its emoji 18.
    @Test
    func theTitleIsALinkInTheLinkColor() throws {
        let text = try #require(shown(embed(title: "Release")).title)
        let font = try #require(text.attribute(.font, at: 0, effectiveRange: nil) as? NSFont)
        #expect(font.pointSize == 16)
        #expect(text.attribute(.link, at: 0, effectiveRange: nil) as? URL == URL(string: "https://example.com/"))
        let plain = try #require(shown(embed(title: "Release", url: nil)).title)
        #expect(plain.attribute(.link, at: 0, effectiveRange: nil) == nil)
    }

    // Emoji in an embed are 18 (Discord's `.embed .emoji`), 1.375 em in a heading.
    @Test
    func emojiInAnEmbedAre18() throws {
        let text = try #require(shown(embed(description: "hi <:wave:1>\n# big <:wave:1>")).description)
        var sides: [CGFloat] = []
        text.enumerateAttribute(.akariEmoji, in: NSRange(location: 0, length: text.length)) { value, _, _ in
            if let box = value as? MessageText.EmojiBox {
                sides.append(box.side)
            }
        }
        #expect(sides == [18, (TypeScale.heading1.size * 1.375).rounded()])
    }

    // A masked link in an embed opens through the list, masked.
    @Test
    func aMaskedLinkInADescriptionAsks() throws {
        show([embed(description: "[docs](https://example.org/docs)")])
        let view = try #require(try views().first)
        let description = try #require(view.textViews.first { $0.string == "docs" })
        let url = try #require(description.textStorage?.attribute(.link, at: 0, effectiveRange: nil) as? URL)
        description.onLink(url)
        #expect(actions.links.first?.0 == URL(string: "https://example.org/docs"))
        #expect(actions.links.first?.masked == true)
    }
}

extension NSView {
    func subviews<View: NSView>(of type: View.Type) -> [View] {
        subviews.flatMap { ($0 as? View).map { [$0] } ?? [] + $0.subviews(of: type) }
    }
}

@MainActor
struct EmbedDrawingTests {
    func drawn(_ view: NSView, size: NSSize, scale: CGFloat) throws -> NSBitmapImageRep {
        let window = ScaledWindow(scale: scale)
        window.appearance = NSAppearance(named: .darkAqua)
        view.frame = NSRect(origin: .zero, size: size)
        window.setContentSize(size)
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        // AppKit puts the subviews' layers into the tree when the window displays.
        window.displayIfNeeded()
        return try AttachmentDrawingTests.layerImage(of: view, scale: scale)
    }

    func shown(_ embed: Embed, cover: MediaTileView.Cover? = nil) -> EmbedView.Shown {
        EmbedView.shown(
            embed, cover: cover, spoilersShown: false, revealed: [], now: Date(),
            times: TimestampText(locale: Locale(identifier: "en_US"), timeZone: berlin.timeZone, calendar: berlin),
            footerTime: nil)
    }

    func color(_ color: NSColor, in view: NSView) -> NSColor? {
        var resolved: NSColor?
        view.effectiveAppearance.performAsCurrentDrawingAppearance {
            resolved = color.usingColorSpace(.deviceRGB)
        }
        return resolved
    }

    // Discord's card (198496): the 4 pt bar in the embed's color, the raised surface, corners 4.
    @Test(arguments: [1, 2] as [CGFloat])
    func theCardIsDiscordsBox(scale: CGFloat) throws {
        let view = EmbedView()
        let card = shown(embed(title: "Hi", description: "Short", color: 0xFF0000))
        view.configure(card)
        let size = EmbedView.layout(card, width: 600).size
        let rep = try drawn(view, size: size, scale: scale)

        let bar = try #require(rep.colorAt(x: Int(2 * scale), y: Int(size.height / 2 * scale)))
        #expect(bar.redComponent > 0.9 && bar.greenComponent < 0.1, "\(bar)")
        let inside = try #require(rep.colorAt(x: Int((size.width - 8) * scale), y: Int(6 * scale)))
        let surface = try #require(color(Palette.surfaceHigh, in: view))
        #expect(abs(inside.redComponent - surface.redComponent) < 0.03, "\(inside) \(surface)")
        let corner = try #require(rep.colorAt(x: 0, y: 0))
        #expect(corner.alphaComponent < 0.5, "rounded")
    }

    // No color (or white) draws the border's; a covered card shows the spoiler color and no bar.
    @Test
    func theBarWithoutAColorAndUnderASpoiler() throws {
        let plain = EmbedView()
        let card = shown(embed(title: "Hi", color: nil))
        plain.configure(card)
        let size = EmbedView.layout(card, width: 600).size
        let rep = try drawn(plain, size: size, scale: 2)
        let bar = try #require(rep.colorAt(x: 4, y: Int(size.height)))
        let surface = try #require(color(Palette.surfaceHigh, in: plain))
        #expect(abs(bar.redComponent - surface.redComponent) > 0.02, "the border's color, not the surface")

        let covered = EmbedView()
        let hidden = shown(embed(title: "Hi", color: 0xFF0000), cover: .spoiler)
        covered.configure(hidden)
        let coveredSize = EmbedView.layout(hidden, width: 600).size
        let cover = try drawn(covered, size: coveredSize, scale: 2)
        let left = try #require(cover.colorAt(x: 4, y: Int(coveredSize.height)))
        #expect(left.redComponent < 0.9 || left.greenComponent > 0.1, "no red bar: \(left)")
    }
}
