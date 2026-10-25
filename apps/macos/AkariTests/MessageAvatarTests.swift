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
