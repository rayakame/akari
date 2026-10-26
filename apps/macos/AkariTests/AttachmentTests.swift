import AkariFFI
import AkariKit
import AppKit
import Testing

@testable import Akari

/// What the list asked for, in order.
final class RecordingMediaActions: MessageListActions {
    var opened: [ImageKey] = []
    var revealed: [MediaKey] = []
    var toggled: [MediaKey] = []

    func retry(_ message: MessageId) {}
    func delete(_ message: MessageId) {}
    func loadMore(_ edge: MessageTimeline.Edge) {}
    func jumpToLatest() {}
    func escape() {}
    func markServerRead() {}
    func jumpToUnread() {}
    func newMessagesLineOnScreen(_ onScreen: Bool, line: MessageId) {}
    func atBottomChanged(_ atBottom: Bool) {}
    func userScrolled(by distance: CGFloat, height: CGFloat) {}
    func reveal(_ spoiler: SpoilerKey) {}
    func openChannel(_ id: ChannelId) {}
    func openThread(_ id: ChannelId, from parent: ChannelId) {}
    func openThreads(of channel: ChannelId) {}
    func jump(to message: MessageId) {}
    func openGuild(_ id: GuildId) {}
    func openLink(_ url: URL, masked: Bool) {}
    func openMedia(_ key: ImageKey) { opened.append(key) }
    func revealMedia(_ key: MediaKey) { revealed.append(key) }
    func toggleSensitive(_ key: MediaKey) { toggled.append(key) }
}

// A message's attachments (D141–D144): Discord's mosaic from metadata only, the media asked at
// its pixels in the media tier and dropped with its view, file rows, the covers, GIFs and
// videos, the list's cursor and clicks.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class AttachmentTests {
    let images = TestImages()
    let list = MessageTableTests()
    let actions = RecordingMediaActions()

    init() {
        list.controller.actions = actions
        // Wide enough for Discord's 550: the text column is the window less 72 and 16.
        list.window.setContentSize(NSSize(width: 900, height: 700))
        list.window.layoutIfNeeded()
    }

    func show(
        _ files: [AkariFFI.Attachment], content: String = "hi", cache: ImageCache,
        revealed: Set<MediaKey> = [], shownSensitive: Set<MediaKey> = [], altText: Bool = false,
        motion: MotionPolicy = .default, focused: Bool = true, placeholders: PlaceholderCache? = nil
    ) {
        list.show(
            MessageTableState(
                rows: [row(1, at: list.noon, content: content, files: files)], reachedOldest: true,
                revealedMedia: revealed, shownSensitive: shownSensitive, showsAltText: altText,
                images: ImageCacheRef(cache, placeholders: placeholders), motion: motion,
                focused: focused))
    }

    func cell() throws -> MessageCell {
        try #require(try list.cell(of: 1) as? MessageCell)
    }

    func tiles() throws -> [MediaTileView] {
        try #require(try cell().attachments).tiles.filter { !$0.isHidden }
    }

    func fileRows() throws -> [FileRowView] {
        try #require(try cell().attachments).files.filter { !$0.isHidden }
    }

    func key(_ id: UInt64) -> MediaKey {
        MediaKey(messageId: MessageId(rawValue: 1), attachmentId: AttachmentId(rawValue: id))
    }

    // MARK: Layout

    // Module 394839 and 619517's numbers: one item contained in 550 × 350, four in a 2 × 2 grid,
    // the whole mosaic scaled down to a narrower column; files 12 under the mosaic, 8 apart.
    @Test
    func theLayoutFollowsDiscordsMosaic() {
        let wide = [media(1, width: 1920, height: 1080)]
        #expect(AttachmentsView.layout(wide, width: 600).mosaic == NSRect(x: 0, y: 0, width: 550, height: 309))
        #expect(AttachmentsView.layout(wide, width: 300).mosaic == NSRect(x: 0, y: 0, width: 300, height: 169))
        let tall = [media(1, width: 1080, height: 1920)]
        #expect(AttachmentsView.layout(tall, width: 600).mosaic?.size == NSSize(width: 197, height: 350))

        let four = (1...4).map { media($0, width: 800, height: 600) }
        let grid = AttachmentsView.layout(four, width: 600)
        #expect(grid.mosaic?.size == NSSize(width: 550, height: 350))
        #expect(grid.cells.map(\.size) == Array(repeating: NSSize(width: 273, height: 173), count: 4))
        #expect(grid.cells[1].minX == grid.cells[0].maxX + 4, "4 apart")

        let mixed = AttachmentsView.layout(
            [media(1, width: 1920, height: 1080), file(2, "a.zip"), file(3, "b.zip")], width: 600)
        #expect(mixed.files == [
            NSRect(x: 0, y: 309 + 12, width: 432, height: 74),
            NSRect(x: 0, y: 309 + 12 + 74 + 8, width: 432, height: 74),
        ])
        // 309 + 12 + 74 + 8 + 74.
        let expected: CGFloat = 477
        #expect(mixed.height == expected)
        let onlyFiles = AttachmentsView.layout([file(2, "a.zip")], width: 300)
        #expect(onlyFiles.files == [NSRect(x: 0, y: 8, width: 300, height: 74)])
        #expect(onlyFiles.mosaic == nil)
    }

    // On screen as in the layout: the large item left, the next top right, the last under it.
    @Test
    func theMosaicIsPlacedFromTheTop() throws {
        show([media(5), media(6), media(7)], cache: ImageCache(source: FakeImageSource.stalling()))
        let cell = try cell()
        let frames = try tiles().map { ThreadBoxViewTests.frame(of: $0, in: cell) }

        #expect(frames[1].minX > frames[0].maxX)
        #expect(frames[1].minY == frames[0].minY, "top right")
        #expect(frames[2].minY == frames[1].maxY + 4, "under it")
    }

    @Test
    func aVideoAloneIsAtLeast150By110() {
        let small = [media(1, "clip.mp4", width: 100, height: 50, kind: .video)]
        #expect(AttachmentsView.layout(small, width: 600).mosaic?.size == NSSize(width: 150, height: 110))
    }

    // Heights come from metadata (D135): the row is as tall before the image as after.
    @Test
    func theRowsHeightIsFinalBeforeTheImageLands() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 1100), pixels: 1100)
        let gate = Gate()
        let slow = FakeImageSource { request in
            await gate.wait()
            return try await source.image(request)
        }
        show([], cache: ImageCache(source: slow))
        let textOnly = list.table.rect(ofRow: list.index(of: 1)).height

        show([media(5, width: 1920, height: 1080)], cache: ImageCache(source: slow))
        let before = list.table.rect(ofRow: list.index(of: 1)).height
        let tile = try #require(try tiles().first)
        gate.fire()
        try await eventually { tile.picture.imageLayer.contents != nil }
        list.window.layoutIfNeeded()

        #expect(before == textOnly + 2 + 309)
        #expect(list.table.rect(ofRow: list.index(of: 1)).height == before)
        #expect(tile.frame.size == NSSize(width: 550, height: 309))
        let text = try #require(list.messageTexts(in: try cell()).first)
        let tileFrame = ThreadBoxViewTests.frame(of: tile, in: try cell())
        #expect(tileFrame.minY == ThreadBoxViewTests.frame(of: text, in: try cell()).maxY + 2, "2 under the text")
    }

    // MARK: Asking

    var attachmentKey: ImageKey {
        .attachment(
            channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1),
            attachmentId: AttachmentId(rawValue: 5))
    }

    // The author's avatar asks too.
    func mediaRequests(_ source: FakeImageSource) -> [ImageRequest] {
        source.requests.withLock { $0.filter { if case .attachment = $0.key { true } else { false } } }
    }

    @Test
    func aTileAsksAtItsPixelsInTheMediaTier() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 1100), pixels: 1100)
        let cache = ImageCache(source: source)
        show([media(5, width: 1920, height: 1080)], cache: cache)
        let tile = try #require(try tiles().first)

        try await eventually { tile.picture.imageLayer.contents != nil }

        let scale = list.window.backingScaleFactor
        let request = try #require(mediaRequests(source).first)
        #expect(request.key == attachmentKey)
        #expect(request.width == UInt32((550 * scale).rounded(.up)))
        #expect(request.height == UInt32((309 * scale).rounded(.up)))
        #expect(request.animated == nil)
        let spec = ImageSpec(key: attachmentKey, width: Int(request.width), height: Int(request.height))
        #expect(cache.mediaBytes > 0, "the media tier")
        #expect(cache.cached(spec) != nil)
    }

    // A reused cell lets its media go: the ask is cancelled with its view.
    @Test
    func anAskGoesWithItsView() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)
        show([media(5)], cache: cache)
        try await eventually { self.mediaRequests(source).count == 1 }
        let before = source.cancelled.withLock { $0 }

        list.show(MessageTableState(rows: [row(1, at: list.noon, content: "hi")], reachedOldest: true, images: ImageCacheRef(cache)))

        try await eventually { source.cancelled.withLock { $0 } == before + 1 }
        #expect(try tiles().isEmpty)
    }

    // Discord's placeholder lies under the media while it loads.
    @Test
    func thePlaceholderShowsWhileItLoads() throws {
        show(
            [media(5, placeholder: "3PcNNYSFeXh/d3eld0iHZoZgVwh2")],
            cache: ImageCache(source: FakeImageSource.stalling()), placeholders: PlaceholderCache())
        let tile = try #require(try tiles().first)

        #expect(tile.placeholderShown)
    }

    // D142: a load over 200 ms fades in over 0.2 s, a quicker one 0.05 s, the decoded LRU's none.
    @Test
    func mediaFadesInAsDiscordsDoes() async throws {
        let url = try images.png("a", pixels: 128)
        let gate = Gate()
        let slow = FakeImageSource.serving(url, pixels: 128, gate: gate)
        let view = ImageLayerView()
        let window = ScaledWindow(scale: 2)
        window.contentView?.addSubview(view)
        view.frame = NSRect(x: 0, y: 0, width: 32, height: 32)
        view.fadesIn = true
        view.tier = .media
        let cache = ImageCache(source: slow)
        view.images = cache
        let key = ImageKey.attachment(
            channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1),
            attachmentId: AttachmentId(rawValue: 5))
        view.show(ImageLayerView.Content(key: key, canAnimate: false))
        view.layoutSubtreeIfNeeded()
        try await Task.sleep(for: .milliseconds(250))
        gate.fire()
        try await eventually { view.imageLayer.contents != nil }
        #expect(view.imageLayer.animation(forKey: "fade")?.duration == 0.2)

        let quick = ImageLayerView()
        window.contentView?.addSubview(quick)
        quick.frame = NSRect(x: 40, y: 0, width: 32, height: 32)
        quick.fadesIn = true
        quick.images = ImageCache(source: FakeImageSource.serving(url, pixels: 128))
        quick.show(ImageLayerView.Content(key: key, canAnimate: false))
        quick.layoutSubtreeIfNeeded()
        try await eventually { quick.imageLayer.contents != nil }
        #expect(quick.imageLayer.animation(forKey: "fade")?.duration == 0.05)

        // Asked again at new pixels, a shown image swaps without fading.
        quick.frame = NSRect(x: 40, y: 0, width: 33, height: 33)
        quick.layoutSubtreeIfNeeded()
        quick.imageLayer.removeAllAnimations()
        try await eventually { quick.imageLayer.image?.width == 66 }
        #expect(quick.imageLayer.animation(forKey: "fade") == nil)

        let cached = ImageLayerView()
        window.contentView?.addSubview(cached)
        cached.frame = NSRect(x: 0, y: 0, width: 32, height: 32)
        cached.fadesIn = true
        cached.images = cache
        cached.show(ImageLayerView.Content(key: key, canAnimate: false))
        cached.layoutSubtreeIfNeeded()
        #expect(cached.imageLayer.contents != nil, "decoded already")
        #expect(cached.imageLayer.animation(forKey: "fade") == nil)
    }

    // Discord's spinner after 2 s of loading, top right, 4 in.
    @Test
    func aSlowLoadShowsTheSpinnerAfterTwoSeconds() async throws {
        show([media(5)], cache: ImageCache(source: FakeImageSource.stalling()))
        let tile = try #require(try tiles().first)
        try await Task.sleep(for: .seconds(1.5))
        #expect(!tile.spinnerShown)

        try await eventually { tile.spinnerShown }

        #expect(tile.spinnerFrame == NSRect(x: tile.bounds.maxX - 28, y: 4, width: 24, height: 24))
    }

    // "Image failed to load." with Akari's symbol; a click asks again (D142's addition).
    @Test
    func aFailedImageSaysSoAndAClickRetries() async throws {
        let source = FakeImageSource.failing()
        show([media(5)], cache: ImageCache(source: source))
        let tile = try #require(try tiles().first)
        try await eventually { tile.failureShown }
        #expect(tile.failureText == "Image failed to load.")
        let asked = source.count

        tile.performClick(nil)

        try await eventually { source.count > asked }
        #expect(actions.opened.isEmpty, "a failed image doesn't open")
    }

    // MARK: Covers

    // D-5: covered by its placeholder (or the spoiler color) and the pill; nothing loads until
    // the click reveals it.
    @Test
    func aSpoilerAsksNothingUntilRevealed() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 64), pixels: 64)
        let cache = ImageCache(source: source)
        show([media(5, cover: .spoiler)], cache: cache)
        let tile = try #require(try tiles().first)
        #expect(tile.coverShown == .spoiler)
        #expect(tile.spoilerPillShown)
        #expect(tile.layer?.cornerRadius == 8)
        try await Task.sleep(for: .milliseconds(100))
        #expect(mediaRequests(source).isEmpty)

        tile.performClick(nil)
        #expect(actions.revealed == [key(5)])
        show([media(5, cover: .spoiler)], cache: cache, revealed: [key(5)])

        let shown = try #require(try tiles().first)
        #expect(shown.coverShown == nil)
        try await eventually { self.mediaRequests(source).count == 1 }
    }

    // Pending: plain black, nothing to click, no eye (module 302031).
    @Test
    func aPendingScanIsABlackBox() throws {
        show([media(5, cover: .pending)], cache: ImageCache(source: FakeImageSource.stalling()))
        let tile = try #require(try tiles().first)

        #expect(tile.coverShown == .pending)
        #expect(!tile.isEnabled)
        #expect(tile.eye.isHidden)
        #expect(!tile.sensitiveWarningShown)
        // Even a click that reached it opens nothing.
        tile.sendAction(tile.action, to: tile.target)
        #expect(actions.opened.isEmpty && actions.revealed.isEmpty)
    }

    // D-1: black with the warning and, where it fits on a single item, Discord's sentence; the
    // eye shows it and hides it again; a click on the cover does nothing.
    @Test
    func sensitiveMediaHidesBehindItsEye() throws {
        let cache = ImageCache(source: FakeImageSource.stalling())
        show([media(5, width: 1920, height: 1080, cover: .sensitive)], cache: cache)
        let tile = try #require(try tiles().first)
        #expect(tile.coverShown == .sensitive(shown: false))
        #expect(tile.sensitiveWarningShown && tile.sensitiveTextShown)
        #expect(!tile.isEnabled)
        tile.sendAction(tile.action, to: tile.target)
        #expect(actions.opened.isEmpty, "a click on the cover does nothing")
        #expect(!tile.eye.isHidden)
        #expect(tile.eye.symbol == "eye.slash")
        #expect(tile.eye.frame == NSRect(x: tile.bounds.maxX - 36, y: tile.bounds.maxY - 36, width: 32, height: 32))

        tile.eye.performClick(nil)
        #expect(actions.toggled == [key(5)])
        show([media(5, width: 1920, height: 1080, cover: .sensitive)], cache: cache, shownSensitive: [key(5)])
        let shown = try #require(try tiles().first)
        #expect(shown.coverShown == nil)
        #expect(shown.eye.symbol == "eye")
        #expect(!shown.eye.isHidden, "it hides again")

        show(
            [media(5, cover: .sensitive), media(6, cover: .sensitive)], cache: cache)
        #expect(try tiles().allSatisfy { $0.sensitiveWarningShown && !$0.sensitiveTextShown }, "only a single item says why")
        show([media(5, width: 1920, height: 300, cover: .sensitive)], cache: cache)
        #expect(try #require(try tiles().first).frame.height < 212)
        #expect(!(try tiles().first!.sensitiveTextShown), "where it fits")
        show([media(5, width: 300, height: 1920, cover: .sensitive)], cache: cache)
        #expect(try #require(try tiles().first).frame.width < 146)
        #expect(!(try tiles().first!.sensitiveTextShown), "where it fits")
    }

    // MARK: Cursor and clicks

    @Test
    func theListShowsTheHandWhereAClickDoesSomething() throws {
        show(
            [
                media(5, width: 800, height: 600), media(6, cover: .spoiler), media(7, cover: .pending),
                media(8, cover: .sensitive), file(9, "notes.zip"),
            ], cache: ImageCache(source: FakeImageSource.stalling()))
        let tiles = try tiles()
        let at = { (view: NSView, point: NSPoint) in view.convert(point, to: nil) }
        let middle = { (view: NSView) in at(view, NSPoint(x: view.bounds.midX, y: view.bounds.midY)) }

        #expect(list.controller.cursor(at: middle(tiles[0])) == .pointingHand, "an image")
        #expect(list.controller.cursor(at: middle(tiles[1])) == .pointingHand, "a spoiler")
        #expect(list.controller.cursor(at: middle(tiles[2])) == .arrow, "pending")
        #expect(list.controller.cursor(at: middle(tiles[3])) == .arrow, "a sensitive cover")
        #expect(list.controller.cursor(at: middle(tiles[3].eye)) == .pointingHand, "its eye")
        let row = try #require(try fileRows().first)
        #expect(list.controller.cursor(at: middle(row.name)) == .pointingHand, "a file's name")
        #expect(list.controller.cursor(at: at(row, NSPoint(x: row.bounds.maxX - 8, y: row.bounds.midY))) == .arrow)
    }

    // D-7: until the viewer, an image and a video's Play open the original in the browser.
    @Test
    func aClickOpensTheMediaAndAFilesName() throws {
        show([media(5), media(6, "clip.mp4", kind: .video), file(7, "notes.zip")], cache: ImageCache(source: FakeImageSource.stalling()))
        let tiles = try tiles()
        #expect(tiles[1].showsPlay)
        #expect(!tiles[0].showsPlay)

        tiles[0].performClick(nil)
        tiles[1].performClick(nil)
        try #require(try fileRows().first).name.performClick(nil)

        let keys = [5, 6, 7].map {
            ImageKey.attachment(
                channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 1),
                attachmentId: AttachmentId(rawValue: UInt64($0)))
        }
        #expect(actions.opened == keys)
    }

    // The Play button and the GIF badge draw over the loaded image, not under it.
    @Test
    func playAndTheBadgeShowOverTheImage() async throws {
        let red = try images.png("red", pixels: 512)
        var still = MotionPolicy.default
        still.gifAutoPlay = false
        show(
            [media(5, "cat.gif", animated: true), media(6, "clip.mp4", kind: .video)],
            cache: ImageCache(source: FakeImageSource.serving(red, pixels: 512)), motion: still)
        let tiles = try tiles()
        try await eventually { tiles.allSatisfy { $0.picture.imageLayer.contents != nil } }
        list.window.displayIfNeeded()

        let gif = try AttachmentDrawingTests.layerImage(of: tiles[0], scale: 1)
        let badge = try #require(gif.colorAt(x: 6 + 4, y: 6 + 11))
        #expect(badge.redComponent < 0.7, "the badge, not the image: \(badge)")
        let video = try AttachmentDrawingTests.layerImage(of: tiles[1], scale: 1)
        let play = try #require(video.colorAt(x: Int(tiles[1].bounds.midX) - 12, y: Int(tiles[1].bounds.midY)))
        #expect(play.redComponent < 0.7, "the Play circle, not the image: \(play)")
    }

    // MARK: GIFs

    // D136: a GIF plays by `gif_auto_play` while 60 % shows and the window is key, else only
    // under the pointer; our own badge shows while it doesn't play.
    @Test
    func aGifPlaysByTheSettingOrItsHover() throws {
        let gif = [media(5, "cat.gif", animated: true)]
        let cache = ImageCache(source: FakeImageSource.stalling())
        show(gif, cache: cache)
        #expect(try #require(try tiles().first).animates)
        #expect(!(try tiles().first!.showsGifBadge))

        var still = MotionPolicy.default
        still.gifAutoPlay = false
        show(gif, cache: cache, motion: still)
        let tile = try #require(try tiles().first)
        #expect(!tile.animates)
        #expect(tile.showsGifBadge)

        list.pointer.location = tile.convert(NSPoint(x: tile.bounds.midX, y: tile.bounds.midY), to: nil)
        list.controller.pointerMoved()
        #expect(tile.animates, "hovered")

        show(gif, cache: cache, focused: false)
        #expect(!(try tiles().first!.animates), "not key")
    }

    // MARK: Files and alt text

    @Test
    func aFileRowShowsItsSymbolNameAndSize() throws {
        var titled = file(9, "IMG_0001.zip", size: 2_000_000)
        titled.title = "Holiday"
        show([file(8, "song.mp3", size: 512), titled], cache: ImageCache(source: FakeImageSource.stalling()))
        let rows = try fileRows()

        #expect(rows.map(\.symbolName) == ["waveform", "doc.zipper"])
        #expect(rows.map(\.shownSize) == ["512 bytes", "1.91 MB"])
        #expect(rows.map { $0.name.attributedTitle.string } == ["song.mp3", "Holiday.zip"])
        #expect(rows.allSatisfy { $0.frame.size == NSSize(width: 432, height: 74) })
    }

    // `view_image_descriptions`: the ALT button bottom left, its popout Discord's title and the
    // text; never over a cover.
    @Test
    func altTextShowsWithTheSetting() throws {
        let cache = ImageCache(source: FakeImageSource.stalling())
        show([media(5, description: "a cat"), media(6, cover: .spoiler, description: "a dog")], cache: cache)
        #expect(try tiles().allSatisfy { $0.alt.isHidden })

        show([media(5, description: "a cat"), media(6, cover: .spoiler, description: "a dog")], cache: cache, altText: true)
        let tiles = try tiles()
        #expect(!tiles[0].alt.isHidden)
        #expect(tiles[1].alt.isHidden, "covered")
        #expect(tiles[0].alt.frame.minX == 4.5)
        #expect(tiles[0].alt.frame.maxY == tiles[0].bounds.maxY - 4)

        tiles[0].alt.performClick(nil)
        let popover = try #require(list.controller.altText)
        let content = try #require(popover.contentViewController as? AltTextController)
        #expect(content.text == "a cat")
        #expect(AltTextController.title == "IMAGE DESCRIPTION (ALT TEXT)")
        popover.close()
    }

    // MARK: Prefetch

    // D135: the media of rows within a screen below what shows is asked before it scrolls in;
    // rows further away aren't, and a spoiler isn't.
    @Test
    func mediaAScreenAwayIsAskedEarly() async throws {
        let source = FakeImageSource.stalling()
        var rows: [MessageListModel.Row] = []
        for raw in UInt64(1)...60 {
            let cover: MediaCover? = raw == 4 ? .spoiler : nil
            let time = list.noon + Double(raw) * 60
            rows.append(row(raw, at: time, content: "m", files: [media(raw * 10, cover: cover, message: raw)]))
        }
        list.show(MessageTableState(rows: rows, reachedOldest: true, images: ImageCacheRef(ImageCache(source: source))))
        let table = list.table
        // The rows land at the bottom: the ones above are asked.
        let atBottom = table.rows(in: table.visibleRect)
        let above = try #require(
            (1...60).last { raw in table.rect(ofRow: list.index(of: UInt64(raw))).maxY <= table.visibleRect.minY - 1 })
        #expect(atBottom.location > 0)
        try await eventually {
            self.mediaRequests(source).contains { $0.key == .attachment(
                channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: UInt64(above)),
                attachmentId: AttachmentId(rawValue: UInt64(above) * 10)) }
        }
        list.clip.scroll(to: NSPoint(x: 0, y: 0))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        let visible = table.rows(in: table.visibleRect)
        let lastShown = visible.location + visible.length - 1
        let screen = table.rows(in: table.visibleRect.offsetBy(dx: 0, dy: table.visibleRect.height))
        let asked = { (raw: Int) in
            self.mediaRequests(source).contains {
                $0.key == .attachment(
                    channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: UInt64(raw)),
                    attachmentId: AttachmentId(rawValue: UInt64(raw) * 10))
            }
        }
        let rawOf = { (index: Int) -> Int? in
            if case .message(let row, _) = self.list.controller.timeline.items[index] {
                Int(row.message.id.rawValue)
            } else {
                nil
            }
        }

        let next = try #require(rawOf(lastShown + 1))
        try await eventually { asked(next) }
        let far = try #require(rawOf(min(screen.location + screen.length + 3, table.numberOfRows - 1)))
        #expect(!asked(far), "two screens away")
        let spoiler = ImageKey.attachment(
            channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: 4),
            attachmentId: AttachmentId(rawValue: 40))
        let prefetched: [ImageKey] = list.controller.prefetching.keys.map(\.key)
        #expect(!prefetched.contains(spoiler), "a spoiler isn't asked")
    }

    // An older page lands above while the list keeps its place: its media is asked too.
    @Test
    func anOlderPagesMediaIsAskedWhereTheListStays() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)
        func rows(_ range: ClosedRange<UInt64>) -> [MessageListModel.Row] {
            range.map { raw in
                let time = list.noon + Double(raw) * 60
                return row(raw, at: time, content: "m", files: [media(raw * 10, message: raw)])
            }
        }
        list.show(MessageTableState(rows: rows(100...140), reachedOldest: false, images: ImageCacheRef(cache)))
        list.clip.scroll(to: NSPoint(x: 0, y: 0))
        list.controller.scrollView.reflectScrolledClipView(list.clip)
        let top = list.controller.timeline.items.firstIndex { if case .message = $0 { true } else { false } }
        try #require(top != nil)

        list.show(MessageTableState(rows: rows(80...140), reachedOldest: false, images: ImageCacheRef(cache)))

        let table = list.table
        let above = try #require(
            (80...99).last { raw in table.rect(ofRow: list.index(of: UInt64(raw))).maxY <= table.visibleRect.minY - 1 })
        try await eventually {
            self.mediaRequests(source).contains { $0.key == .attachment(
                channelId: ChannelId(rawValue: 10), messageId: MessageId(rawValue: UInt64(above)),
                attachmentId: AttachmentId(rawValue: UInt64(above) * 10)) }
        }
    }
}

// Discord's numbers at 2×, measured on the drawing (D130's sharpness rule for media too).
@MainActor @Suite(.timeLimit(.minutes(1)))
struct AttachmentDrawingTests {
    // The layer tree as Core Animation composites it at `scale`, read from the top left.
    func drawn(_ view: NSView, scale: CGFloat) throws -> NSBitmapImageRep {
        let window = ScaledWindow(scale: scale)
        window.appearance = NSAppearance(named: .darkAqua)
        window.setContentSize(view.frame.size)
        window.contentView = view
        view.layoutSubtreeIfNeeded()
        return try Self.layerImage(of: view, scale: scale)
    }

    // A view's layer tree where it is, its top left first.
    static func layerImage(of view: NSView, scale: CGFloat) throws -> NSBitmapImageRep {
        let layer = try #require(view.layer)
        let rep = try #require(
            NSBitmapImageRep(
                bitmapDataPlanes: nil, pixelsWide: Int(view.bounds.width * scale),
                pixelsHigh: Int(view.bounds.height * scale), bitsPerSample: 8, samplesPerPixel: 4,
                hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 0,
                bitsPerPixel: 0))
        let context = try #require(NSGraphicsContext(bitmapImageRep: rep)).cgContext
        context.scaleBy(x: scale, y: scale)
        // Core Animation draws upward; the bitmap's rows run down.
        context.translateBy(x: 0, y: view.bounds.height)
        context.scaleBy(x: 1, y: -1)
        view.effectiveAppearance.performAsCurrentDrawingAppearance {
            layer.render(in: context)
        }
        return rep
    }

    // The spoiler pill: 15/600 uppercase, 12 and 8 around, radius 20, black .6.
    @Test
    func theSpoilerPillHasDiscordsPadding() throws {
        let tile = MediaTileView()
        tile.frame = NSRect(x: 0, y: 0, width: 300, height: 200)
        tile.configure(MediaTileView.Content(attachment: media(5, cover: .spoiler), cover: .spoiler))
        tile.layoutSubtreeIfNeeded()

        let pill = tile.spoilerPillFrame
        let text = NSAttributedString(
            string: "SPOILER",
            attributes: [.font: NSFont.systemFont(ofSize: 15, weight: .semibold), .kern: 0.5]).size()
        #expect(pill.width == ceil(text.width) + 24)
        #expect(pill.height == ceil(text.height) + 16)
        #expect(abs(pill.midX - 150) < 0.5 && abs(pill.midY - 100) < 0.5, "centered")
    }

    // The file row: 432 × 74, 1 pt subtle border, 8 corners, Discord's surface-high inside.
    @Test(arguments: [1, 2] as [CGFloat])
    func theFileRowIsDiscordsBox(scale: CGFloat) throws {
        let row = FileRowView()
        row.frame = NSRect(x: 0, y: 0, width: 432, height: 74)
        row.configure(file(9, "notes.zip"))
        let rep = try drawn(row, scale: scale)

        #expect(rep.pixelsWide == Int(432 * scale) && rep.pixelsHigh == Int(74 * scale))
        let inside = try #require(rep.colorAt(x: Int(200 * scale), y: Int(4 * scale)))
        var surface: NSColor?
        row.effectiveAppearance.performAsCurrentDrawingAppearance {
            surface = Palette.surfaceHigh.usingColorSpace(.deviceRGB)
        }
        let expected = try #require(surface)
        #expect(abs(inside.redComponent - expected.redComponent) < 0.02, "\(inside) \(expected)")
        let corner = try #require(rep.colorAt(x: 0, y: 0))
        #expect(corner.alphaComponent < 0.5, "rounded")
    }
}
