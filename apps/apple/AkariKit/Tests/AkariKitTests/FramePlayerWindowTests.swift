import CoreGraphics
import Foundation
import Testing

@testable import AkariKit

// An animation whose frames don't all fit the budget plays from a window of frames decoded ahead
// of the picture: in order, as fast as they decode, never past a frame that isn't there yet.
@MainActor @Suite(.timeLimit(.minutes(2)))
final class FramePlayerWindowTests {
    let clock = AnimationClock(automatic: false)
    let shown = Locked<[Int]>([])

    func player(_ frames: any FrameSource, frameBudget: Int = FramePlayer.frameBudget) -> FramePlayer {
        FramePlayer(animation: frames, pixels: (540, 405), clock: clock, frameBudget: frameBudget) {
            [shown] _, index in shown.withLock { $0.append(index) }
        }
    }

    func fixture() throws -> AnimationSource {
        let url = try #require(AnimatedFormatProbe.fixtureURL)
        return try #require(AnimationSource(url: url))
    }

    // Ticks `step` apart until `done`, giving the decoder its turn between them.
    func run(from start: Double = 0, step: Double, until done: () -> Bool) async throws -> Double {
        var now = start
        let deadline = ContinuousClock.now + .seconds(60)
        while !done() {
            try #require(ContinuousClock.now < deadline, "the animation never got there")
            now += step
            clock.advance(to: now)
            try await Task.sleep(for: .milliseconds(1))
        }
        return now
    }

    // Three frames come due for each one decoded: the picture runs slow, it doesn't stand still.
    @Test
    func aDecoderSlowerThanTheFramesStillShowsEachInOrder() async throws {
        let frames = GatedFrames(count: 327, delay: 0.04)
        let player = player(frames)
        defer { frames.allow(.max) }

        player.start(at: 0)
        for landed in 1...12 {
            clock.advance(to: Double(landed) * 0.12)
            frames.allow()
            try await eventually { self.shown.current.count >= landed }
            guard shown.current.count == landed else {
                break
            }
            #expect(player.decodedFrameCount <= 4)
        }

        #expect(shown.current == Array(0..<12))
        #expect(Array(frames.decoded.current.prefix(12)) == Array(0..<12), "decoded in order, no jumps")
    }

    @Test
    func aFrameThatDoesntDecodeIsPassedOver() async throws {
        let frames = GatedFrames(count: 327, delay: 0.04, broken: [2])
        frames.allow(.max)
        let player = player(frames)

        player.start(at: 0)
        _ = try await run(step: 0.02) { shown.current.count >= 4 }
        player.stop()

        #expect(Array(shown.current.prefix(4)) == [0, 1, 3, 4])
    }

    // A frame that failed once is tried again by the next start.
    @Test
    func aStartTriesBrokenFramesAgain() async throws {
        let frames = GatedFrames(count: 327, delay: 0.04, broken: [0, 1])
        frames.allow(.max)
        let player = player(frames)
        player.start(at: 0)
        _ = try await run(step: 0.02) { shown.current.count >= 2 }
        player.stop()
        #expect(Array(shown.current.prefix(2)) == [2, 3])
        frames.broken.withLock { $0 = [] }
        shown.withLock { $0 = [] }

        player.start(at: 10)
        _ = try await run(from: 10, step: 0.02) { shown.current.count >= 2 }
        player.stop()

        #expect(Array(shown.current.prefix(2)) == [0, 1])
    }

    // The last loop decodes nothing past its last frame.
    @Test
    func theLastLoopDoesntDecodeIntoTheNext() async throws {
        let frames = GatedFrames(count: 40, delay: 0.04, loopCount: 1)
        frames.allow(.max)
        let player = FramePlayer(animation: frames, pixels: (4000, 4000), clock: clock) {
            [shown] _, index in shown.withLock { $0.append(index) }
        }

        player.start(at: 0)
        _ = try await run(step: 0.02) { !player.isPlaying }

        #expect(shown.current == Array(0..<40))
        #expect(frames.decoded.current == Array(0..<40), "each frame once")
    }

    // The GIF that never played, by its numbers: 327 frames of 540 × 405 in 10 159 614 bytes,
    // 286 MB decoded. It plays through its loop and into the next from four frames at a time.
    @Test
    func theHeavyGifPlaysThroughItsLoopFromFourFrames() async throws {
        let url = try await TestGIF.heavyURL
        let size = try #require(try FileManager.default.attributesOfItem(atPath: url.path)[.size] as? Int)
        #expect(size == 10_159_614)
        let animation = try #require(AnimationSource(url: url))
        #expect(animation.frameCount == 327)
        #expect(animation.pixelSize == (540, 405))
        #expect(animation.loopCount == 0)
        #expect(abs(animation.cycle - 13.16) < 1e-6)
        #expect(TestGIF.heavy.decodedBytes == 286_059_600)
        #expect(TestGIF.heavy.decodedBytes > FramePlayer.frameBudget)
        let sizes = Locked<Set<[Int]>>([])
        var held = 0
        // The viewer's box on a 2× screen: the frames come at the file's own size.
        let player = FramePlayer(animation: animation, pixels: (1080, 810), fit: .contain, clock: clock) {
            [shown] image, index in
            shown.withLock { $0.append(index) }
            sizes.withLock { _ = $0.insert([image.width, image.height]) }
        }

        player.start(at: 0)
        _ = try await run(step: 0.02) {
            held = max(held, player.decodedFrameCount)
            return shown.current.count >= 332
        }
        player.stop()

        #expect(Array(shown.current.prefix(332)) == Array(0..<327) + Array(0..<5))
        #expect(sizes.current == [[540, 405]])
        #expect(held <= 4)
        #expect(held * 540 * 405 * 4 <= FramePlayer.frameBudget)
        #expect(animation.decodes.current <= 332 + 8, "each frame decoded once, in order")
    }

    // Delays 100, 200, 50, 300, 150 ms. A late tick goes to the frame that's due when the
    // frames up to it are decoded.
    @Test
    func aLateTickSkipsToTheDueFrameWhenItsDecoded() async throws {
        let animation = try fixture()
        let player = player(animation, frameBudget: 1)
        await player.prepare()

        player.start(at: 0)
        clock.advance(to: 0.36)
        player.stop()

        #expect(shown.current == [0, 3])
    }

    // Three loops, then the last frame stays.
    @Test
    func theLoopCountEndsAWindowedAnimationOnItsLastFrame() async throws {
        let animation = try fixture()
        let player = player(animation, frameBudget: 1)

        player.start(at: 0)
        _ = try await run(step: 0.05) { !player.isPlaying }

        #expect(shown.current == [0, 1, 2, 3, 4, 0, 1, 2, 3, 4, 0, 1, 2, 3, 4])
        #expect(clock.playerCount == 0)
    }
}
