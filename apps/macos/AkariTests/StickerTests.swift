import AkariKit
import AppKit
import Testing

@testable import Akari

// Stickers under a message's text (D140): 160 × 160 boxes in a row, 8 apart, asked through the
// cache in the sticker's format, playing per `animate_stickers` and the message's hover; a
// Lottie sticker shows a box with its name, a failed one "Uh Oh!"; nothing clicks yet.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class StickerTests {
    let images = TestImages()
    let list = MessageTableTests()

    func rows(_ stickers: [String], format: StickerFormatType = .png, content: String = "hi")
        -> [MessageListModel.Row]
    {
        [row(1, at: list.noon, content: content, stickers: stickers, stickerFormat: format)]
    }

    func state(
        _ rows: [MessageListModel.Row], cache: ImageCache, focused: Bool = true,
        motion: MotionPolicy = .default
    ) -> MessageTableState {
        MessageTableState(
            rows: rows, reachedOldest: true, images: ImageCacheRef(cache), motion: motion,
            focused: focused)
    }

    func cell() throws -> MessageCell {
        try #require(try list.cell(of: 1) as? MessageCell)
    }

    func stickerViews(_ cell: MessageCell) -> [StickerView] {
        MessageMenuAreaTests.views(StickerView.self, in: cell).filter { !$0.isHiddenOrHasHiddenAncestor }
    }

    @Test
    func aStickerRowIs160HighUnderTheText() throws {
        let plain = row(1, at: list.noon, content: "hi")
        let withSticker = rows(["wave"]).first!
        let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)
        let body = MessageText.build(plain.message, revealed: [], now: .now, times: times).text
        let width = list.table.tableColumns[0].width
        let without = MessageCell.height(plain.message, body: body, startsGroup: true, width: width)
        let with = MessageCell.height(withSticker.message, body: body, startsGroup: true, width: width)
        #expect(with == without + 160 + 2)

        list.show(state(rows([]), cache: ImageCache(source: FakeImageSource.stalling())))
        let plainRow = list.table.rect(ofRow: list.index(of: 1)).height
        list.show(state(rows(["wave"]), cache: ImageCache(source: FakeImageSource.stalling())))
        #expect(list.table.rect(ofRow: list.index(of: 1)).height == plainRow + 160 + 2)
        let cell = try cell()
        let views = stickerViews(cell)
        #expect(views.count == 1)
        let sticker = try #require(views.first)
        #expect(sticker.frame.size == NSSize(width: 160, height: 160))
        let text = try #require(list.messageTexts(in: cell).first)
        let textFrame = ThreadBoxViewTests.frame(of: text, in: cell)
        let stickerFrame = ThreadBoxViewTests.frame(of: sticker, in: cell)
        #expect(stickerFrame.minY >= textFrame.maxY, "under the text")
        #expect(stickerFrame.minX == textFrame.minX, "at the text's left")

        list.show(state(rows([]), cache: ImageCache(source: FakeImageSource.stalling())))
        #expect(stickerViews(try self.cell()).isEmpty)
    }

    @Test
    func stickersSitInARowEightApart() throws {
        list.show(state(rows(["a", "b", "c"]), cache: ImageCache(source: FakeImageSource.stalling())))

        let views = stickerViews(try cell()).sorted { $0.frame.minX < $1.frame.minX }
        #expect(views.count == 3)
        #expect(views[1].frame.minX == views[0].frame.maxX + 8)
        #expect(views[2].frame.minX == views[1].frame.maxX + 8)
        #expect(views.allSatisfy { $0.frame.minY == views[0].frame.minY })
    }

    @Test
    func aStickerAsksThroughTheCacheInItsFormat() async throws {
        let source = FakeImageSource.serving(try images.png("s", pixels: 320), pixels: 320)
        let cache = ImageCache(source: source)
        list.show(state(rows(["wave"], format: .apng), cache: cache))
        let sticker = try #require(stickerViews(try cell()).first)

        try await eventually { sticker.image.imageLayer.contents != nil }

        let scale = list.window.backingScaleFactor
        let key = ImageKey.sticker(stickerId: StickerId(rawValue: 10), format: .apng)
        try await eventually { source.requests.withLock { $0.contains { $0.key == key && $0.animated != nil } } }
        let requests = source.requests.withLock { $0.filter { $0.key == key } }
        let still = try #require(requests.first { $0.animated == nil })
        #expect(still.width == UInt32((160 * scale).rounded(.up)))
        #expect(requests.contains { $0.animated != nil }, "the setting plays it: the animated form too")
        #expect(sticker.image.content?.canAnimate == true)
        #expect(sticker.image.cornerRadius == 0)
        #expect(sticker.image.images === cache)
        #expect(sticker.image.fit == .contain, "the whole sticker, never cut")
    }

    // A Lottie sticker has no image Akari shows: a box with a face and the sticker's name, no ask.
    @Test
    func aLottieStickerShowsABoxWithItsName() async throws {
        let source = FakeImageSource.stalling()
        list.show(state(rows(["dance"], format: .lottie), cache: ImageCache(source: source)))

        let sticker = try #require(stickerViews(try cell()).first)
        #expect(sticker.symbolName == "face.smiling")
        #expect(sticker.caption == "dance")
        #expect(sticker.image.isHidden)
        try await Task.sleep(for: .milliseconds(50))
        let asked = source.requests.withLock { $0.filter { if case .sticker = $0.key { true } else { false } } }
        #expect(asked.isEmpty)
    }

    @Test
    func aFailedStickerShowsUhOh() async throws {
        list.show(state(rows(["wave"]), cache: ImageCache(source: FakeImageSource.failing())))
        let sticker = try #require(stickerViews(try cell()).first)

        try await eventually { sticker.image.failed }

        #expect(sticker.symbolName == "exclamationmark.triangle")
        #expect(sticker.caption == "Uh Oh!")
    }

    // `animate_stickers`: always plays while 70 % visible and the window key; on interaction
    // plays while the message is hovered; never doesn't.
    @Test
    func stickersPlayPerTheSettingAndTheHover() throws {
        let cache = ImageCache(source: FakeImageSource.stalling())
        let rows = rows(["wave"], format: .apng)
        list.show(state(rows, cache: cache))
        #expect(try cell().animatesStickers)
        #expect(try stickerViews(cell()).first?.image.animates == true)

        list.show(state(rows, cache: cache, focused: false))
        #expect(try !cell().animatesStickers)

        var onHover = MotionPolicy.default
        onHover.animateStickers = .onInteraction
        list.show(state(rows, cache: cache, motion: onHover))
        #expect(try !cell().animatesStickers)
        let text = try #require(list.messageTexts(in: try cell()).first)
        list.pointer.location = text.convert(NSPoint(x: 4, y: text.bounds.midY), to: nil)
        list.controller.pointerMoved()
        #expect(try cell().animatesStickers, "hovered")
        list.pointer.location = nil
        list.controller.pointerMoved()
        #expect(try !cell().animatesStickers)

        var never = MotionPolicy.default
        never.animateStickers = .never
        list.show(state(rows, cache: cache, motion: never))
        list.pointer.location = text.convert(NSPoint(x: 4, y: text.bounds.midY), to: nil)
        list.controller.pointerMoved()
        #expect(try !cell().animatesStickers)
    }

    // A reused cell for a message without stickers lets its stickers go: no ask, no pin, no
    // player, and the list's say turns off.
    @Test
    func aReusedCellWithoutStickersDropsThem() async throws {
        let source = FakeImageSource.serving(
            still: try images.png("s", pixels: 320), animation: images.animated)
        let cache = ImageCache(source: source)
        list.show(state(rows(["wave"], format: .apng), cache: cache))
        let sticker = try #require(stickerViews(try cell()).first)
        try await eventually { sticker.image.player != nil }
        let scale = list.window.backingScaleFactor
        let spec = ImageSpec(
            key: .sticker(stickerId: StickerId(rawValue: 10), format: .apng),
            width: Int((160 * scale).rounded(.up)), height: Int((160 * scale).rounded(.up)), fit: .contain)
        #expect(cache.isPinned(spec))

        list.show(state([row(1, at: list.noon, content: "plain now")], cache: cache))

        let cell = try cell()
        #expect(stickerViews(cell).isEmpty)
        #expect(sticker.image.content == nil)
        #expect(sticker.image.player == nil)
        #expect(!cache.isPinned(spec))
        #expect(!cell.animatesStickers)
    }

    // The list's say arrives before the row exists for a fresh cell; it holds.
    @Test
    func animatesStickersSetBeforeTheRowExistsHolds() throws {
        let cell = MessageCell()
        cell.animatesStickers = true
        let message = rows(["wave"], format: .apng).first!.message

        cell.configure(
            message, body: NSAttributedString(string: "hi"), startsGroup: true, groupTime: "", shortTime: "",
            fullDate: "")

        #expect(cell.animatesStickers)
        #expect(MessageMenuAreaTests.views(StickerView.self, in: cell).first?.image.animates == true)
    }

    @Test
    func theCursorStaysTheArrowOverASticker() throws {
        list.show(state(rows(["wave"]), cache: ImageCache(source: FakeImageSource.stalling())))
        let sticker = try #require(stickerViews(try cell()).first)
        let over = sticker.convert(NSPoint(x: 80, y: 80), to: nil)

        #expect(list.controller.cursor(at: over) == .arrow)
        #expect(sticker.hitTest(sticker.convert(NSPoint(x: 80, y: 80), to: sticker.superview)) == nil)
    }
}
