import AkariKit
import AppKit
import Testing

@testable import Akari

// A box's image as a layer (D135): asked at the box's pixels on the window's scale, set on whole
// backing pixels at that scale, the ask cancelled when the box shows something else or goes;
// nothing drawn, nothing tracked.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ImageLayerViewTests {
    let images = TestImages()
    let window = NSWindow(
        contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled],
        backing: .buffered, defer: false)

    var scale: CGFloat { window.backingScaleFactor }

    func shown(_ cache: ImageCache, size: CGFloat = 40, at origin: NSPoint = NSPoint(x: 10, y: 10))
        -> ImageLayerView
    {
        let view = ImageLayerView()
        view.images = cache
        view.frame = NSRect(origin: origin, size: NSSize(width: size, height: size))
        window.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        return view
    }

    @Test
    func theImageLayerSitsOnWholeBackingPixels() {
        let bounds = NSRect(x: 0.3, y: 0.6, width: 40, height: 40)

        #expect(
            ImageLayerView.alignedFrame(bounds, scale: 2) == NSRect(x: 0.5, y: 0.5, width: 40, height: 40))
        #expect(
            ImageLayerView.alignedFrame(bounds, scale: 1) == NSRect(x: 0, y: 1, width: 40, height: 40))
        #expect(
            ImageLayerView.alignedFrame(NSRect(x: 0, y: 0, width: 16.4, height: 16.4), scale: 2)
                == NSRect(x: 0, y: 0, width: 16.5, height: 16.5))
    }

    @Test
    func theAskedPixelsAreThePointsTimesTheScaleRoundedUp() {
        #expect(ImageLayerView.pixels(NSSize(width: 40, height: 40), scale: 2) == (80, 80))
        #expect(ImageLayerView.pixels(NSSize(width: 16.5, height: 16.5), scale: 2) == (33, 33))
        #expect(ImageLayerView.pixels(NSSize(width: 16.5, height: 16.5), scale: 1) == (17, 17))
        #expect(ImageLayerView.pixels(NSSize(width: 40, height: 40), scale: 3) == (120, 120))
    }

    @Test
    func itAsksAtTheWindowsScaleAndSetsTheContentsAtIt() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        let view = shown(cache)

        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }

        let request = try #require(source.requests.withLock { $0.first })
        #expect(request.width == UInt32((40 * scale).rounded(.up)))
        #expect(request.height == request.width)
        #expect(request.animated == nil)
        #expect(view.imageLayer.contentsScale == scale)
        let image = try #require(view.imageLayer.image)
        #expect(CGFloat(image.width) == (40 * scale).rounded(.up))
        #expect(view.imageLayer.frame == ImageLayerView.alignedFrame(view.bounds, scale: scale))
    }

    @Test
    func aCachedImageIsSetWithoutWaiting() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 256), pixels: 256)
        let cache = ImageCache(source: source)
        let first = shown(cache)
        first.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { first.imageLayer.contents != nil }

        let second = shown(cache, at: NSPoint(x: 100, y: 10))
        second.show(ImageLayerView.Content(key: .avatar(1)))

        #expect(second.imageLayer.contents != nil)
        #expect(source.count == 1)
    }

    @Test
    func showingSomethingElseCancelsTheAsk() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)
        let view = shown(cache)

        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { source.count == 1 }
        view.show(ImageLayerView.Content(key: .avatar(2)))

        try await eventually { source.cancelled.withLock { $0 } == 1 }
        #expect(source.count == 2)
        view.show(ImageLayerView.Content(key: .avatar(2)))
        #expect(source.count == 2, "the same content asks nothing more")
    }

    @Test
    func leavingTheWindowCancelsAndUnpins() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)
        let view = shown(cache)
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { source.count == 1 }

        view.removeFromSuperview()

        try await eventually { source.cancelled.withLock { $0 } == 1 }
        #expect(cache.inflightCount == 0)
        #expect(!cache.isPinned(ImageSpec(key: .avatar(1), width: Int(40 * scale), height: Int(40 * scale))))
    }

    @Test
    func theDiscShowsUntilTheImageLandsAndStaysOnFailure() async throws {
        let gate = Gate()
        let source = FakeImageSource.serving(try images.png("a", pixels: 80), pixels: 80, gate: gate)
        let cache = ImageCache(source: source)
        let view = shown(cache)
        view.placeholder = .disc

        view.show(ImageLayerView.Content(key: .avatar(1)))
        #expect(!view.placeholderLayer.isHidden)
        #expect(view.placeholderLayer.backgroundColor == Palette.avatarPlaceholder.cgColor)
        gate.fire()
        try await eventually { view.imageLayer.contents != nil }
        #expect(view.placeholderLayer.isHidden)

        let failing = shown(ImageCache(source: FakeImageSource.failing()), at: NSPoint(x: 100, y: 10))
        failing.placeholder = .disc
        failing.show(ImageLayerView.Content(key: .avatar(2)))
        try await eventually { failing.failed }
        #expect(!failing.placeholderLayer.isHidden)
        #expect(failing.imageLayer.contents == nil)
    }

    // A changed avatar or icon keeps the old image until the new one has decoded (D137).
    @Test
    func aNewKeyKeepsTheOldImageUntilTheNewOneDecoded() async throws {
        let first = try images.png("a", pixels: 80)
        let source = FakeImageSource { request in
            if case .avatar(_, let hash) = request.key, hash.hasPrefix("a_") {
                try await Task.sleep(for: .seconds(60))
            }
            return ImageFile(path: first.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
        let view = shown(ImageCache(source: source))
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }
        let shownBefore = view.imageLayer.image

        view.show(ImageLayerView.Content(key: .avatar(1, animated: true), canAnimate: true))
        try await eventually { source.count == 2 }

        #expect(view.imageLayer.image === shownBefore)
        view.show(nil)
        #expect(view.imageLayer.contents == nil)
    }

    @Test
    func itDrawsNoBitmapAndTracksNothing() throws {
        let view = shown(ImageCache(source: FakeImageSource.failing()))

        #expect(!RowDrawingTests.drawsItsOwnBitmap(ImageLayerView.self, over: NSView.self))
        #expect(view.trackingAreas.isEmpty)
        #expect(view.hitTest(NSPoint(x: 20, y: 20)) == nil)
    }

    @Test
    func roundBoxesClipTheirLayer() {
        let view = shown(ImageCache(source: FakeImageSource.failing()))
        view.cornerRadius = 20

        #expect(view.imageLayer.cornerRadius == 20)
        #expect(view.imageLayer.masksToBounds)
        #expect(view.imageLayer.contentsGravity == .resizeAspectFill)
    }

    @Test
    func animatesPlaysTheAnimationAndStoppingShowsTheStill() async throws {
        let source = FakeImageSource.serving(
            still: try images.png("still", pixels: 80), animation: images.animated)
        let cache = ImageCache(source: source)
        let view = shown(cache, size: 16)
        view.show(ImageLayerView.Content(key: .avatar(1, animated: true), canAnimate: true))
        try await eventually { view.imageLayer.contents != nil }
        let still = view.imageLayer.image

        view.animates = true
        try await eventually { view.player?.isPlaying == true }
        let animated = source.requests.withLock { $0.last }
        #expect(animated?.animated == AnimatedFormatProbe.current)
        #expect(animated?.width == UInt32((16 * scale).rounded(.up)))
        try await eventually { view.imageLayer.image !== still }

        view.animates = false
        #expect(view.player == nil)
        #expect(view.imageLayer.image === still)
    }

    @Test
    func aStillKeyNeverAnimates() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 80), pixels: 80)
        let view = shown(ImageCache(source: source))
        view.show(ImageLayerView.Content(key: .avatar(1)))

        view.animates = true
        try await eventually { source.count == 1 }
        try await Task.sleep(for: .milliseconds(50))

        #expect(view.player == nil)
        #expect(source.count == 1)
    }
}

extension CALayer {
    /// The contents as the image they are; `nil` without one.
    var image: CGImage? {
        guard let contents, CFGetTypeID(contents as CFTypeRef) == CGImage.typeID else {
            return nil
        }
        return unsafeBitCast(contents as CFTypeRef, to: CGImage.self)
    }
}
