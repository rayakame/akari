import AkariKit
import AppKit
import Testing
import os

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

    // The layer lands on whole backing pixels of the window, wherever its view's frame sits
    // (a thread box's avatar is at a half point), so the bitmap draws 1:1.
    @Test
    func theImageLayerSitsOnWholeBackingPixelsOfTheWindow() {
        let bounds = NSRect(x: 0.3, y: 0.6, width: 40, height: 40)
        #expect(
            ImageLayerView.alignedFrame(bounds, scale: 2) == NSRect(x: 0.5, y: 0.5, width: 40, height: 40))
        #expect(
            ImageLayerView.alignedFrame(bounds, scale: 1) == NSRect(x: 0, y: 1, width: 40, height: 40))
        #expect(
            ImageLayerView.alignedFrame(NSRect(x: 0, y: 0, width: 16.4, height: 16.4), scale: 2)
                == NSRect(x: 0, y: 0, width: 16.5, height: 16.5))

        let cache = ImageCache(source: FakeImageSource.failing())
        for origin in [NSPoint(x: 10.25, y: 37.5), NSPoint(x: 100.3, y: 10.8), NSPoint(x: 10, y: 10)] {
            let view = shown(cache, size: 16, at: origin)
            let inWindow = view.convert(view.imageLayer.frame.origin, to: nil)
            #expect(abs(inWindow.x * scale - (inWindow.x * scale).rounded()) < 1e-6, "\(origin)")
            #expect(abs(inWindow.y * scale - (inWindow.y * scale).rounded()) < 1e-6, "\(origin)")
            #expect(abs(view.imageLayer.frame.minX) < 1 / scale && abs(view.imageLayer.frame.minY) < 1 / scale)
            #expect(view.placeholderLayer.frame == view.imageLayer.frame)
        }
    }

    // A table cell's subviews join the window at zero size and get their frame in the layout
    // pass after: the ask happens then.
    @Test
    func aViewSizedAfterJoiningTheWindowAsks() async throws {
        let source = FakeImageSource.serving(try images.png("a", pixels: 80), pixels: 80)
        let cache = ImageCache(source: source)
        let view = ImageLayerView()
        view.images = cache
        view.show(ImageLayerView.Content(key: .avatar(1)))
        window.contentView?.addSubview(view)
        #expect(source.count == 0)

        view.frame = NSRect(x: 10, y: 10, width: 40, height: 40)
        view.layoutSubtreeIfNeeded()

        try await eventually { view.imageLayer.contents != nil }
        #expect(source.count == 1)
    }

    // A recycled box showing another user or server clears at once: the disc, never the
    // previous subject's picture, until the new one decoded. The same subject's new hash keeps
    // the old image (D137).
    @Test
    func anotherSubjectClearsTheOldImageAtOnce() async throws {
        let gate = Gate()
        let source = FakeImageSource { request in
            if case .avatar(let user, _) = request.key, user.rawValue == 2 {
                await gate.wait()
            }
            return ImageFile(path: "/nowhere", width: 80, height: 80, animated: false, fromDisk: false)
        }
        let file = try images.png("a", pixels: 80)
        source.replace { request in
            if case .avatar(let user, _) = request.key, user.rawValue == 2 {
                await gate.wait()
            }
            return ImageFile(path: file.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
        let view = shown(ImageCache(source: source))
        view.placeholder = .disc
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }

        view.show(ImageLayerView.Content(key: .avatar(2)))

        #expect(view.imageLayer.contents == nil)
        #expect(view.imageLayer.isHidden)
        #expect(!view.placeholderLayer.isHidden)
        gate.fire()
        try await eventually { view.imageLayer.contents != nil }
        #expect(view.placeholderLayer.isHidden)
    }

    @Test
    func aFailedLoadHidesTheOldImage() async throws {
        let file = try images.png("a", pixels: 80)
        let source = FakeImageSource { request in
            if case .avatar(_, let hash) = request.key, hash.hasPrefix("a_") {
                throw ImageError.Unavailable
            }
            return ImageFile(path: file.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
        let view = shown(ImageCache(source: source))
        view.placeholder = .disc
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }

        view.show(ImageLayerView.Content(key: .avatar(1, animated: true), canAnimate: true))
        try await eventually { view.failed }

        #expect(view.imageLayer.contents == nil)
        #expect(view.imageLayer.isHidden)
        #expect(!view.placeholderLayer.isHidden)
    }

    // Offline, a box fails fast; when the session is back online the cache says so and every
    // failed box asks again (D131).
    @Test
    func aFailedViewAsksAgainWhenTheCacheRetries() async throws {
        let file = try images.png("a", pixels: 80)
        let source = FakeImageSource { _ in throw ImageError.Unavailable }
        let cache = ImageCache(source: source)
        let view = shown(cache)
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.failed }
        source.replace { _ in
            ImageFile(path: file.path, width: 80, height: 80, animated: false, fromDisk: false)
        }

        cache.retryFailed()

        try await eventually { view.imageLayer.contents != nil }
        #expect(!view.failed)
        #expect(source.count == 2)
        let other = ImageCache(source: FakeImageSource.failing())
        other.retryFailed()
        #expect(source.count == 2, "another cache's retry isn't this view's")
    }

    // Hover, un-hover, hover again on a slow network: the first ask's failure leaves the
    // second one in place, so un-hovering still stops it.
    @Test
    func aFailedAnimationAskLeavesItsSuccessorAlone() async throws {
        let first = Gate()
        let second = Gate()
        let asks = OSAllocatedUnfairLock(initialState: 0)
        let still = try images.png("still", pixels: 80)
        let animated = images.animated
        let source = FakeImageSource { request in
            guard request.animated != nil else {
                return ImageFile(path: still.path, width: 80, height: 80, animated: false, fromDisk: false)
            }
            let ask = asks.withLock { count in
                count += 1
                return count
            }
            if ask == 1 {
                await first.wait()
                throw ImageError.Unavailable
            }
            await second.wait()
            return ImageFile(path: animated.path, width: 32, height: 32, animated: true, fromDisk: false)
        }
        let view = shown(ImageCache(source: source), size: 16)
        view.show(ImageLayerView.Content(key: .avatar(1, animated: true), canAnimate: true))
        try await eventually { view.imageLayer.contents != nil }

        view.animates = true
        view.animates = false
        view.animates = true
        try await eventually { asks.withLock { $0 } == 2 }
        first.fire()
        try await Task.sleep(for: .milliseconds(50))
        view.animates = false
        second.fire()
        try await Task.sleep(for: .milliseconds(100))

        #expect(view.player == nil)
    }

    // A disc without a key: the store doesn't know the person yet, or the group has no icon.
    @Test
    func aNilKeyWithTheDiscShowsTheDisc() {
        let view = shown(ImageCache(source: FakeImageSource.failing()))
        view.placeholder = .disc

        view.show(nil)

        #expect(!view.placeholderLayer.isHidden)
        view.placeholder = .none
        #expect(view.placeholderLayer.isHidden)
    }

    @Test
    func noLayerPropertyAnimatesImplicitly() {
        let view = ImageLayerView()
        for key in ["contents", "hidden", "bounds", "position", "cornerRadius", "backgroundColor"] {
            #expect(view.imageLayer.actions?[key] is NSNull, "\(key)")
            #expect(view.placeholderLayer.actions?[key] is NSNull, "\(key)")
        }
    }

    // Moved to a display with another scale, the box asks at the new pixels and keeps the old
    // image until the new one decoded.
    @Test
    func aScaleChangeAsksAgainKeepingTheOldImage() async throws {
        let gate = Gate()
        let file = try images.png("a", pixels: 256)
        let source = FakeImageSource { request in
            if request.width > 40 {
                await gate.wait()
            }
            return ImageFile(path: file.path, width: 256, height: 256, animated: false, fromDisk: false)
        }
        let cache = ImageCache(source: source)
        let one = ScaledWindow(scale: 1)
        let two = ScaledWindow(scale: 2)
        let view = ImageLayerView()
        view.images = cache
        view.frame = NSRect(x: 10, y: 10, width: 40, height: 40)
        one.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }
        let before = view.imageLayer.image
        #expect(before?.width == 40)

        two.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        try await eventually { source.count == 2 }

        #expect(source.requests.withLock { $0.last }?.width == 80)
        #expect(view.imageLayer.image === before)
        gate.fire()
        try await eventually { view.imageLayer.image?.width == 80 }
        #expect(view.imageLayer.contentsScale == 2)
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

    // A box that fits its image (emoji, stickers) shows the whole picture, centered, never
    // cut: a wide image gets shorter. The box's size is the same before and after it lands.
    @Test
    func aFittingBoxShowsTheWholeImageCentered() async throws {
        let source = FakeImageSource.serving(
            try images.png("wide", width: 200, height: 100), pixels: 200)
        let cache = ImageCache(source: source)
        let view = shown(cache)
        view.fit = .contain
        #expect(view.imageLayer.contentsGravity == .resizeAspect)

        view.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { view.imageLayer.contents != nil }

        let image = try #require(view.imageLayer.image)
        #expect(CGFloat(image.width) == (40 * scale).rounded(.up))
        #expect(CGFloat(image.height) == (20 * scale).rounded(.up))
        #expect(view.frame.size == NSSize(width: 40, height: 40))
        let rep = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: rep)
        let middle = rep.colorAt(x: rep.pixelsWide / 2, y: rep.pixelsHigh / 2)?.usingColorSpace(.sRGB)
        let top = rep.colorAt(x: rep.pixelsWide / 2, y: 2)?.usingColorSpace(.sRGB)
        #expect(middle?.redComponent ?? 0 > 0.9 && middle?.alphaComponent ?? 0 > 0.9, "the image in the middle")
        #expect(top?.alphaComponent ?? 1 < 0.1, "nothing above it: the box isn't filled")

        let covering = shown(cache, at: NSPoint(x: 100, y: 10))
        covering.show(ImageLayerView.Content(key: .avatar(1)))
        try await eventually { covering.imageLayer.contents != nil }
        #expect(covering.imageLayer.image?.height == Int((40 * scale).rounded(.up)), "a covering box is another entry")
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

/// A window whose backing scale is what the test says.
final class ScaledWindow: NSWindow {
    private let scale: CGFloat

    init(scale: CGFloat) {
        self.scale = scale
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 300, height: 200), styleMask: [.titled],
            backing: .buffered, defer: false)
    }

    override var backingScaleFactor: CGFloat { scale }
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
