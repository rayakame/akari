import AkariKit
import AppKit
import Testing

@testable import Akari

// Avatars in the message list: a group start's 40 point avatar from the cache, rows and the
// cursor unmoved by its landing, the group's hover animating it through the list's authority.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class MessageAvatarTests {
    let images = TestImages()
    let list = MessageTableTests()

    func rows() -> [MessageListModel.Row] {
        let ray = User(
            id: UserId(rawValue: 2), username: "ray", globalName: "Ray", displayName: "Ray",
            bot: false, system: false, avatar: .avatar(2, animated: true))
        return [
            row(1, by: ray, at: list.noon, content: "first"),
            row(2, by: ray, at: list.noon + 10, content: "second"),
            row(3, by: author(3, "Mira"), at: list.noon + 20, content: "third"),
        ]
    }

    func state(_ cache: ImageCache, focused: Bool = true, motion: MotionPolicy = .default)
        -> MessageTableState
    {
        MessageTableState(
            rows: rows(), reachedOldest: true, images: ImageCacheRef(cache), motion: motion,
            focused: focused)
    }

    func avatar(of raw: UInt64) throws -> ImageLayerView {
        let cell = try #require(try list.cell(of: raw) as? MessageCell)
        return try #require(MessageMenuAreaTests.views(ImageLayerView.self, in: cell).first)
    }

    func text(of raw: UInt64) throws -> MessageTextView {
        try #require(list.messageTexts(in: try list.cell(of: raw)).first)
    }

    func over(_ view: NSView) -> NSPoint {
        view.convert(NSPoint(x: 4, y: view.bounds.midY), to: nil)
    }

    @Test
    func theHeaderShowsTheAuthorsAvatarAt40Points() async throws {
        let source = FakeImageSource.serving(try images.png("ray", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        list.controller.show(state(cache))
        list.window.layoutIfNeeded()

        let avatar = try avatar(of: 1)
        try await eventually { avatar.imageLayer.contents != nil }

        #expect(avatar.frame.size == NSSize(width: 40, height: 40))
        #expect(avatar.cornerRadius == 20)
        let scale = list.window.backingScaleFactor
        #expect(source.requests.withLock { $0.first }?.width == UInt32((40 * scale).rounded(.up)))
        #expect(source.requests.withLock { $0.first }?.key == .avatar(2, animated: true))
        #expect(source.requests.withLock { $0.first }?.animated == nil, "the still")
    }

    @Test
    func aLandingAvatarMovesNeitherTheRowNorTheCursor() async throws {
        let gate = Gate()
        let source = FakeImageSource.serving(
            try images.png("ray", pixels: 256), pixels: 256, gate: gate)
        let cache = ImageCache(source: source)
        list.controller.show(state(cache))
        list.window.layoutIfNeeded()
        let heights = (0..<list.table.numberOfRows).map { list.table.rect(ofRow: $0).height }
        let avatar = try avatar(of: 1)
        let overAvatar = avatar.convert(NSPoint(x: 20, y: 20), to: nil)
        let overText = over(try text(of: 1))
        let before = (list.controller.cursor(at: overAvatar), list.controller.cursor(at: overText))

        gate.fire()
        try await eventually { avatar.imageLayer.contents != nil }
        list.window.layoutIfNeeded()

        #expect((0..<list.table.numberOfRows).map { list.table.rect(ofRow: $0).height } == heights)
        #expect(list.controller.cursor(at: overAvatar) == before.0)
        #expect(list.controller.cursor(at: overText) == before.1)
        #expect(before.0 == .arrow)
        #expect(before.1 == .iBeam)
    }

    func distinctAuthors(_ count: Int) -> [MessageListModel.Row] {
        (1...count).map { raw in
            let user = User(
                id: UserId(rawValue: UInt64(raw)), username: "u\(raw)", globalName: "U\(raw)",
                displayName: "U\(raw)", bot: false, system: false, avatar: .avatar(UInt64(raw)))
            return row(UInt64(raw), by: user, at: list.noon + Double(raw) * 600, content: "m \(raw)")
        }
    }

    /// The header avatars of the visible rows, with their author's ID.
    func headerAvatars() -> [(raw: UInt64, view: ImageLayerView)] {
        let visible = list.table.rows(in: list.table.visibleRect)
        return (visible.location..<(visible.location + visible.length)).compactMap { index in
            guard
                let cell = list.table.view(atColumn: 0, row: index, makeIfNecessary: false)
                    as? MessageCell, cell.showsHeader,
                case .message(let row, _) = list.controller.timeline.items[index],
                let view = MessageMenuAreaTests.views(ImageLayerView.self, in: cell).first
            else { return nil }
            return (row.message.author.id.rawValue, view)
        }
    }

    /// Scrolls up by 250 points and lets the loads land; false at the top.
    func scrollUp() async throws -> Bool {
        let clip = list.clip
        let y = max(0, clip.bounds.minY - 250)
        clip.scroll(to: NSPoint(x: 0, y: y))
        list.controller.scrollView.reflectScrolledClipView(clip)
        list.window.layoutIfNeeded()
        try await Task.sleep(for: .milliseconds(60))
        list.window.layoutIfNeeded()
        return y > 0
    }

    // A follow-up row's avatar view is hidden and asks for nothing; only the header's loads.
    @Test
    func aFollowUpsHiddenAvatarAsksNothing() async throws {
        let source = FakeImageSource.serving(try images.png("ray", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        list.controller.show(state(cache))
        list.window.layoutIfNeeded()
        try await eventually { try self.avatar(of: 1).imageLayer.contents != nil }

        let followUp = try avatar(of: 2)
        #expect(followUp.isHidden)
        #expect(followUp.content == nil)
        try await Task.sleep(for: .milliseconds(50))
        let wanted = ImageKey.avatar(2, animated: true)
        #expect(source.requests.withLock { $0.filter { $0.key == wanted } }.count == 1)
    }

    // The table inserts a cell before it lays it out: its avatar joins the window at zero size
    // and must still ask once sized. Every header made while scrolling loads.
    @Test
    func freshCellsCreatedWhileScrollingLoadTheirAvatars() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 80), pixels: 80)
        let cache = ImageCache(source: source)
        list.controller.show(
            MessageTableState(rows: distinctAuthors(120), reachedOldest: true, images: ImageCacheRef(cache)))
        list.window.layoutIfNeeded()

        var missing = Set<UInt64>()
        var checked = 0
        for _ in 0..<30 {
            let more = try await scrollUp()
            let shown = headerAvatars()
            checked += shown.count
            // Each gets 2 s to land, as a busy machine may need; one that never asked never does.
            var waiting = shown.filter { $0.view.imageLayer.contents == nil }
            let deadline = ContinuousClock.now + .seconds(2)
            while !waiting.isEmpty, ContinuousClock.now < deadline {
                try await Task.sleep(for: .milliseconds(10))
                waiting = waiting.filter { $0.view.imageLayer.contents == nil }
            }
            missing.formUnion(waiting.map(\.raw))
            if !more {
                break
            }
        }

        #expect(checked > 20)
        #expect(missing.isEmpty, "header avatars that never loaded: \(missing.sorted())")
    }

    // A reused cell for another author never shows the previous author's picture: the disc
    // until the new one decoded, and the disc when it fails.
    @Test
    func aReusedCellWhoseNewAvatarFailsShowsTheDisc() async throws {
        let file = try images.png("a", pixels: 80)
        let source = FakeImageSource { request in
            if case .avatar(let user, _) = request.key, user.rawValue % 2 == 1 {
                throw ImageError.Unavailable
            }
            return ImageFile(path: file.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
        let cache = ImageCache(source: source)
        list.controller.show(
            MessageTableState(rows: distinctAuthors(120), reachedOldest: true, images: ImageCacheRef(cache)))
        list.window.layoutIfNeeded()

        var wrong = Set<UInt64>()
        var failedSeen = 0
        for _ in 0..<30 {
            let more = try await scrollUp()
            for (raw, view) in headerAvatars() where raw % 2 == 1 {
                if view.failed {
                    failedSeen += 1
                }
                if view.failed, view.imageLayer.contents != nil || !view.imageLayer.isHidden {
                    wrong.insert(raw)
                }
                if view.failed, view.placeholderLayer.isHidden {
                    wrong.insert(raw)
                }
            }
            if !more {
                break
            }
        }

        #expect(failedSeen > 10)
        #expect(wrong.isEmpty, "a failed avatar shows someone else's image: \(wrong.sorted())")
    }

    // The DM start's avatar (80 points) sizes itself in the cell's layout, after it joined
    // the window.
    @Test
    func aDirectMessagesStartAvatarLoads() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        let start = ChannelStart(
            icon: .avatar(.avatar(7)), title: "Ray", username: "ray",
            subtitle: "This is the beginning of your direct message history with Ray.")
        list.controller.show(
            MessageTableState(
                rows: distinctAuthors(2), reachedOldest: true, start: start, images: ImageCacheRef(cache)))
        list.window.layoutIfNeeded()

        let cell = try #require(
            (0..<list.table.numberOfRows).lazy.compactMap {
                self.list.table.view(atColumn: 0, row: $0, makeIfNecessary: false) as? ChannelStartCell
            }.first)
        let view = cell.icon.avatar
        try await eventually { view.imageLayer.contents != nil }

        #expect(view.frame.size == NSSize(width: 80, height: 80))
        let scale = list.window.backingScaleFactor
        let wanted = ImageKey.avatar(7)
        let request = source.requests.withLock { $0.first { $0.key == wanted } }
        #expect(request?.width == UInt32((80 * scale).rounded(.up)))
    }

    // The new messages line starts no group: a follow-up under it still hovers its header.
    @Test
    func hoveringAFollowUpBelowTheNewMessagesLineAnimatesTheHeader() throws {
        let source = FakeImageSource.serving(
            still: try images.png("still", pixels: 80), animation: images.animated)
        let cache = ImageCache(source: source)
        var state = state(cache)
        state.newMessages = MessageId(rawValue: 2)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        let header = try avatar(of: 1)

        list.pointer.location = over(try text(of: 2))
        list.controller.pointerMoved()

        #expect(header.animates)
    }

    // Hovering any message of the group animates the group's header avatar; off the group, or
    // with the window not key, it stops.
    @Test
    func hoveringAnyMessageOfTheGroupAnimatesItsHeaderAvatar() async throws {
        let source = FakeImageSource.serving(
            still: try images.png("still", pixels: 80), animation: images.animated)
        let cache = ImageCache(source: source)
        list.controller.show(state(cache))
        list.window.layoutIfNeeded()
        let avatar = try avatar(of: 1)
        let followUp = over(try text(of: 2))
        let other = over(try text(of: 3))

        list.pointer.location = followUp
        list.controller.pointerMoved()
        #expect(avatar.animates)

        list.pointer.location = other
        list.controller.pointerMoved()
        #expect(!avatar.animates)

        list.pointer.location = followUp
        list.controller.pointerMoved()
        #expect(avatar.animates)
        list.controller.show(state(cache, focused: false))
        #expect(!avatar.animates, "nothing animates while the window isn't key")

        list.controller.show(state(cache, focused: true))
        #expect(avatar.animates, "the pointer is still there")
        list.pointer.location = nil
        list.controller.pointerMoved()
        #expect(!avatar.animates)
    }
}
