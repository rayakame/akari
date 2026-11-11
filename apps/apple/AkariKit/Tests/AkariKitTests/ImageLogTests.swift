import Foundation
import Testing

@testable import AkariKit

// `-AkariLog images`: why an image or an animation doesn't show, by key, never by link or file.
@MainActor @Suite(.serialized)
struct ImageLogTests {
    let lines = Locked<[String]>([])
    let images = TestImages()

    // The log on and written to `lines` for `body`, then as it was.
    func logging(_ body: () async throws -> Void) async rethrows {
        let (enabled, write) = (ImageLog.enabled, ImageLog.write)
        ImageLog.enabled = true
        ImageLog.write = { [lines] line in lines.withLock { $0.append(line) } }
        defer {
            ImageLog.enabled = enabled
            ImageLog.write = write
        }
        try await body()
    }

    @Test(arguments: [
        ("images", true), ("akari_core=debug,images", true), (" images ", true),
        ("images=debug", true), ("image", false), ("akari_core=debug", false), ("", false),
    ])
    func theImagesLogIsOnlyOnWhenItsCategoryIsAsked(_ filter: String, _ asked: Bool) {
        #expect(ImageLog.isAsked(by: filter) == asked)
    }

    @Test
    func nothingIsWrittenWhileItsOff() async {
        await logging {
            ImageLog.enabled = false

            ImageLog.log("a line")
        }

        #expect(lines.current.isEmpty)
    }

    @Test
    func keysAreNamedByDiscordsIDs() {
        #expect(
            ImageLog.name(.attachment(channelId: id(7), messageId: id(8), attachmentId: id(9)))
                == "attachment 9 (message 8)")
        #expect(
            ImageLog.name(.embedMedia(channelId: id(7), messageId: id(8), embed: 1, slot: .thumbnail))
                == "embed 1 thumbnail (message 8)")
        #expect(ImageLog.name(.testAvatar(5)) == "avatar (user 5)")
        #expect(ImageLog.name(.emoji(emojiId: id(4), animated: true)) == "emoji 4")
    }

    // An animated ask answered with a still file: the box keeps its still, and the log says so.
    @Test
    func aStillAnsweringAnAnimationAskIsLogged() async throws {
        let still = try images.png("secret-name", width: 32, height: 32)
        let cache = ImageCache(source: FakeImageSource.serving(still, width: 32, height: 32))
        let key = ImageKey.attachment(channelId: id(7), messageId: id(8), attachmentId: id(9))

        await logging {
            await #expect(throws: ImageCacheError.undecodable) {
                try await cache.animation(ImageSpec(key: key, width: 16, height: 16, animated: .gif))
            }
        }

        #expect(lines.current.contains("animation attachment 9 (message 8): the file has one frame"))
        #expect(!lines.current.contains { $0.contains("secret-name") || $0.contains(images.directory.path) })
    }

    // Why a file gave no animation is only looked into for the log.
    @Test
    func withTheLogOffAFailedOpenIsntLookedInto() async throws {
        let still = try images.png("still", width: 32, height: 32)
        let cache = ImageCache(source: FakeImageSource.serving(still, width: 32, height: 32))
        let probes = AnimationSource.failureProbes.current

        await #expect(throws: ImageCacheError.undecodable) {
            try await cache.animation(ImageSpec(key: .testAvatar(5), width: 16, height: 16, animated: .gif))
        }
        #expect(AnimationSource.failureProbes.current == probes)

        await logging {
            _ = try? await cache.animation(ImageSpec(key: .testAvatar(5), width: 16, height: 16, animated: .gif))
        }
        #expect(AnimationSource.failureProbes.current > probes)
    }

    @Test
    func aFileThatsGoneIsLogged() async throws {
        let gone = images.directory.appending(path: "gone.gif")
        let cache = ImageCache(source: FakeImageSource.serving(gone, width: 32, height: 32, animated: true))

        await logging {
            await #expect(throws: ImageCacheError.undecodable) {
                try await cache.animation(
                    ImageSpec(key: .testAvatar(5), width: 16, height: 16, animated: .gif))
            }
        }

        #expect(lines.current.contains("animation avatar (user 5): the file is gone"))
    }

    @Test
    func aFrameThatDoesntDecodeIsLogged() async throws {
        let frames = GatedFrames(count: 327, delay: 0.04, broken: [1])
        frames.allow(.max)
        let clock = AnimationClock(automatic: false)
        let player = FramePlayer(animation: frames, pixels: (540, 405), clock: clock) { _, _ in }

        try await logging {
            player.start(at: 0)
            try await eventually { lines.current.contains { $0.contains("frame 1 of 327") } }
            player.stop()
        }

        #expect(lines.current.contains("animation: frame 1 of 327 didn't decode, passed over"))
    }
}
