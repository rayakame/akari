import CoreGraphics
import Foundation
import Testing

@testable import AkariKit

// The decoded tier (D134): files decoded at the box's pixels off the main actor, two LRUs
// bounded in bytes, pins for what's on screen, one source call shared by concurrent asks.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class ImageCacheTests {
    let images = TestImages()

    func spec(_ raw: UInt64 = 1, width: Int = 80, height: Int = 80) -> ImageSpec {
        ImageSpec(key: .testAvatar(raw), width: width, height: height)
    }

    // 80 × 80 RGBA.
    static let avatarBytes = 80 * 80 * 4

    @Test
    func aFileIsDecodedAtTheBoxsPixels() async throws {
        let source = FakeImageSource.serving(
            try images.png("big", width: 256, height: 256), width: 256, height: 256)
        let cache = ImageCache(source: source)

        let image = try await cache.image(spec())

        #expect(image.width == 80)
        #expect(image.height == 80)
        #expect(source.requests.current == [spec().request])
        #expect(cache.cached(spec()) === image)
    }

    // A box narrower than the source's aspect: the decoded image covers it, never fewer pixels
    // on either side.
    @Test
    func aWideSourceCoversTheBox() async throws {
        let source = FakeImageSource.serving(
            try images.png("wide", width: 400, height: 200), width: 400, height: 200)
        let cache = ImageCache(source: source)

        let image = try await cache.image(spec())

        #expect(image.width == 160)
        #expect(image.height == 80)
    }

    @Test
    func aSourceSmallerThanTheBoxKeepsItsOwnSize() async throws {
        let source = FakeImageSource.serving(
            try images.png("small", width: 32, height: 32), width: 32, height: 32)
        let cache = ImageCache(source: source)

        let image = try await cache.image(spec())

        #expect(image.width == 32)
        #expect(image.height == 32)
    }

    // ImageIO reads the size from the header, so the cap costs no decode; a bomb of 64 MP is
    // "failed", not a 256 MB bitmap. The cap is lowered here: a real 64 MP file is too slow to
    // write in a test.
    @Test
    func aSourceOverThePixelCapIsRefused() async throws {
        let source = FakeImageSource.serving(
            try images.png("big", width: 256, height: 256), width: 256, height: 256)
        let cache = ImageCache(source: source, maxPixels: 256 * 256 - 1)

        await #expect(throws: ImageCacheError.tooLarge) {
            try await cache.image(self.spec())
        }
        #expect(cache.cached(spec()) == nil)
        #expect(ImageDecoder.maxPixels == 64_000_000)

        let fitting = ImageCache(source: source, maxPixels: 256 * 256)
        #expect(try await fitting.image(spec()).width == 80)
    }

    @Test
    func anUndecodableFileFails() async throws {
        let text = images.directory.appending(path: "text.png")
        try Data("not an image".utf8).write(to: text)
        let source = FakeImageSource.serving(text, width: 80, height: 80)
        let cache = ImageCache(source: source)

        await #expect(throws: ImageCacheError.undecodable) {
            try await cache.image(self.spec())
        }
    }

    @Test
    func aFailedLoadIsntRememberedHere() async throws {
        let source = FakeImageSource { _ in throw ImageError.Unavailable }
        let cache = ImageCache(source: source)

        await #expect(throws: ImageError.self) { try await cache.image(self.spec()) }
        await #expect(throws: ImageError.self) { try await cache.image(self.spec()) }

        #expect(source.requests.current.count == 2)
        #expect(cache.cached(spec()) == nil)
    }

    // The core keeps files a minute after handing them out; one the system purged meanwhile is
    // asked for once more, and only once.
    @Test
    func aFileMissingAtDecodeIsAskedForOnceMore() async throws {
        let real = try images.png("real", width: 80, height: 80)
        let missing = images.directory.appending(path: "gone.png")
        let answers = Locked([missing, real])
        let source = FakeImageSource { _ in
            let url = answers.withLock { $0.isEmpty ? missing : $0.removeFirst() }
            return ImageFile(path: url.path, width: 80, height: 80, animated: false, fromDisk: true)
        }
        let cache = ImageCache(source: source)

        let image = try await cache.image(spec())

        #expect(image.width == 80)
        #expect(source.requests.current.count == 2)

        let gone = FakeImageSource.serving(missing, width: 80, height: 80)
        let failing = ImageCache(source: gone)
        await #expect(throws: ImageCacheError.undecodable) {
            try await failing.image(self.spec())
        }
        #expect(gone.requests.current.count == 2)
    }

    @Test
    func theIdentityTierCountsBytesAndEvictsTheLeastRecentlyUsed() async throws {
        let file = try images.png("avatar", width: 80, height: 80)
        let source = FakeImageSource.serving(file, width: 80, height: 80)
        let cache = ImageCache(source: source, identityLimit: 3 * Self.avatarBytes)

        for raw in 1...4 as ClosedRange<UInt64> {
            _ = try await cache.image(spec(raw))
        }

        #expect(cache.identityBytes == 3 * Self.avatarBytes)
        #expect(cache.cached(spec(1)) == nil)
        for raw in 2...4 as ClosedRange<UInt64> {
            #expect(cache.cached(spec(raw)) != nil, "\(raw)")
        }

        // A hit moves an entry to the front.
        _ = cache.cached(spec(2))
        _ = try await cache.image(spec(5))
        #expect(cache.cached(spec(2)) != nil)
        #expect(cache.cached(spec(3)) == nil)
    }

    @Test
    func aPinnedEntryIsNeverEvicted() async throws {
        let file = try images.png("avatar", width: 80, height: 80)
        let source = FakeImageSource.serving(file, width: 80, height: 80)
        let cache = ImageCache(source: source, identityLimit: 2 * Self.avatarBytes)

        cache.pin(spec(1))
        for raw in 1...4 as ClosedRange<UInt64> {
            _ = try await cache.image(spec(raw))
        }

        #expect(cache.cached(spec(1)) != nil)
        #expect(cache.cached(spec(2)) == nil)
        #expect(cache.cached(spec(3)) == nil)
        #expect(cache.cached(spec(4)) != nil)
        #expect(cache.identityBytes == 2 * Self.avatarBytes)

        cache.unpin(spec(1))
        _ = try await cache.image(spec(5))
        #expect(cache.cached(spec(1)) == nil)
    }

    @Test
    func anImageLargerThanItsTierIsHandedOutButNotKept() async throws {
        let file = try images.png("avatar", width: 80, height: 80)
        let source = FakeImageSource.serving(file, width: 80, height: 80)
        let cache = ImageCache(source: source, identityLimit: Self.avatarBytes - 1)

        let image = try await cache.image(spec())

        #expect(image.width == 80)
        #expect(cache.cached(spec()) == nil)
        #expect(cache.identityBytes == 0)
    }

    @Test
    func mediaHasItsOwnTierAndCanBeDroppedAlone() async throws {
        let file = try images.png("media", width: 80, height: 80)
        let source = FakeImageSource.serving(file, width: 80, height: 80)
        let cache = ImageCache(source: source)
        let media = ImageSpec(
            key: .attachment(
                channelId: ChannelId(rawValue: 1), messageId: MessageId(rawValue: 2),
                attachmentId: AttachmentId(rawValue: 3)),
            width: 80, height: 80)

        _ = try await cache.image(spec(), tier: .identity)
        _ = try await cache.image(media, tier: .media)
        cache.pin(media)
        #expect(cache.identityBytes == Self.avatarBytes)
        #expect(cache.mediaBytes == Self.avatarBytes)

        cache.dropMedia()
        #expect(cache.cached(media) != nil, "pinned media stays")
        cache.unpin(media)
        cache.dropMedia()
        #expect(cache.cached(media) == nil)
        #expect(cache.cached(spec()) != nil)
        #expect(cache.mediaBytes == 0)
    }

    @Test
    func aMemoryWarningDropsEverythingNotPinned() async throws {
        let file = try images.png("avatar", width: 80, height: 80)
        let source = FakeImageSource.serving(file, width: 80, height: 80)
        let cache = ImageCache(source: source)

        _ = try await cache.image(spec(1))
        _ = try await cache.image(spec(2))
        cache.pin(spec(2))
        cache.dropUnpinned()

        #expect(cache.cached(spec(1)) == nil)
        #expect(cache.cached(spec(2)) != nil)
        #expect(cache.identityBytes == Self.avatarBytes)
    }

    // Two boxes asking for the same image share one download; the last one to leave cancels
    // it, the first one leaving doesn't.
    @Test
    func concurrentAsksShareOneSourceCallAndTheLastWaiterCancelsIt() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)

        let first = Task { try await cache.image(self.spec()) }
        let second = Task { try await cache.image(self.spec()) }
        _ = await source.started.next()
        try await Task.sleep(for: .milliseconds(50))
        #expect(source.requests.current.count == 1)
        #expect(cache.inflightCount == 1)

        first.cancel()
        _ = await first.result
        try await Task.sleep(for: .milliseconds(50))
        #expect(source.cancelled.current == 0, "the second still waits")
        #expect(cache.inflightCount == 1)

        second.cancel()
        _ = await second.result
        try await Task.sleep(for: .milliseconds(50))
        #expect(source.cancelled.current == 1)
        #expect(cache.inflightCount == 0)
    }

    @Test
    func anAnswerReachesEveryWaiter() async throws {
        let file = try images.png("avatar", width: 80, height: 80)
        let gate = Signal()
        let source = FakeImageSource { _ in
            await gate.wait()
            return ImageFile(path: file.path, width: 80, height: 80, animated: false, fromDisk: false)
        }
        let cache = ImageCache(source: source)

        let first = Task { try await cache.image(self.spec()) }
        let second = Task { try await cache.image(self.spec()) }
        _ = await source.started.next()
        gate.fire()

        let (one, two) = (try await first.value, try await second.value)
        #expect(one === two)
        #expect(source.requests.current.count == 1)
    }

    @Test
    func anAnimationGivesItsFramesDelaysAndLoops() async throws {
        let source = FakeImageSource.serving(images.animatedWebP, width: 32, height: 32, animated: true)
        let cache = ImageCache(source: source)
        let spec = ImageSpec(key: .testAvatar(1), width: 16, height: 16, animated: .webP)

        let animation = try await cache.animation(spec)

        #expect(animation.frameCount == 5)
        #expect(animation.delays.map { ($0 * 1000).rounded() } == [100, 200, 50, 300, 150])
        #expect(animation.loopCount == 3)
        let frame = try #require(animation.frame(2, pixels: (16, 16)))
        #expect(frame.width == 16 && frame.height == 16)
        #expect(source.requests.current == [spec.request])
    }

    // Un-hovering cancels the animated file's download, as it cancels a still's (D131).
    @Test
    func cancellingAnAnimationAskCancelsTheSourceCall() async throws {
        let source = FakeImageSource.stalling()
        let cache = ImageCache(source: source)
        let spec = ImageSpec(key: .testAvatar(1), width: 16, height: 16, animated: .webP)

        let ask = Task { try await cache.animation(spec) }
        _ = await source.started.next()
        ask.cancel()

        await #expect(throws: CancellationError.self) { try await ask.value }
        try await eventually { source.cancelled.current == 1 }
    }

    // Coming back online asks every failed box again; a hidden window drops the media tier.
    @Test
    func retryFailedSaysSoToItsViews() async throws {
        let cache = ImageCache(source: FakeImageSource.stalling())
        let heard = Locked(0)
        let token = NotificationCenter.default.addObserver(
            forName: ImageCache.retryFailedNotification, object: cache, queue: .main
        ) { [heard] _ in heard.withLock { $0 += 1 } }
        defer { NotificationCenter.default.removeObserver(token) }

        cache.retryFailed()

        #expect(heard.current == 1)
    }

    @Test
    func specsCarryTheirRequest() {
        let spec = ImageSpec(key: .testAvatar(1), width: 80, height: 40, animated: .gif)

        #expect(spec.request == ImageRequest(key: .testAvatar(1), width: 80, height: 40, animated: .gif))
        #expect(ImageSpec(key: .testAvatar(1), width: 80, height: 40).request.animated == nil)
    }
}

// Which keys can move: `a_` hashes and animated emoji.
struct ImageKeyMotionTests {
    @Test
    func animatedHashesAndEmojiCanAnimate() {
        let still = "0123456789abcdef0123456789abcdef"
        let moving = "a_0123456789abcdef0123456789abcdef"
        #expect(!ImageKey.avatar(userId: UserId(rawValue: 1), hash: still).canAnimate)
        #expect(ImageKey.avatar(userId: UserId(rawValue: 1), hash: moving).canAnimate)
        #expect(
            ImageKey.memberAvatar(guildId: GuildId(rawValue: 1), userId: UserId(rawValue: 1), hash: moving)
                .canAnimate)
        #expect(ImageKey.guildIcon(guildId: GuildId(rawValue: 1), hash: moving).canAnimate)
        #expect(!ImageKey.guildIcon(guildId: GuildId(rawValue: 1), hash: still).canAnimate)
        #expect(!ImageKey.groupIcon(channelId: ChannelId(rawValue: 1), hash: moving).canAnimate)
        #expect(ImageKey.emoji(emojiId: EmojiId(rawValue: 1), animated: true).canAnimate)
        #expect(!ImageKey.emoji(emojiId: EmojiId(rawValue: 1), animated: false).canAnimate)
        #expect(!ImageKey.defaultAvatar(index: 3).canAnimate)
    }

    @Test
    func theSameSubjectIsTheSameUserServerOrGroup() {
        let ray = ImageKey.testAvatar(1)
        let rayNew = ImageKey.avatar(userId: UserId(rawValue: 1), hash: "a_0123456789abcdef0123456789abcdef")
        let rayInGuild = ImageKey.memberAvatar(
            guildId: GuildId(rawValue: 5), userId: UserId(rawValue: 1), hash: "0123456789abcdef0123456789abcdef")
        let mira = ImageKey.testAvatar(2)
        #expect(ray.sameSubject(as: rayNew))
        #expect(ray.sameSubject(as: rayInGuild))
        #expect(!ray.sameSubject(as: mira))
        #expect(!ray.sameSubject(as: .defaultAvatar(index: 0)))
        #expect(!ImageKey.defaultAvatar(index: 0).sameSubject(as: .defaultAvatar(index: 1)))
        let lab = ImageKey.guildIcon(guildId: GuildId(rawValue: 5), hash: "0123456789abcdef0123456789abcdef")
        let labNew = ImageKey.guildIcon(guildId: GuildId(rawValue: 5), hash: "a_0123456789abcdef0123456789abcdef")
        #expect(lab.sameSubject(as: labNew))
        #expect(!lab.sameSubject(as: .guildIcon(guildId: GuildId(rawValue: 6), hash: "0123456789abcdef0123456789abcdef")))
        #expect(!lab.sameSubject(as: ray))
    }

}
