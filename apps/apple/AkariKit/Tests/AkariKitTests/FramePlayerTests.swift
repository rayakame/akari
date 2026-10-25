import CoreGraphics
import Foundation
import Testing

@testable import AkariKit

// Akari's own playback (amendment 277): frames decoded at the box's pixels off the main actor,
// one clock for every animation, each frame shown at its delay, loops honoured, stopped when
// told.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class FramePlayerTests {
    let clock = AnimationClock(automatic: false)
    let shown = Locked<[Int]>([])

    func animation() throws -> AnimationSource {
        let url = try #require(AnimatedFormatProbe.fixtureURL)
        return try #require(AnimationSource(url: url))
    }

    func player(_ animation: AnimationSource) -> FramePlayer {
        FramePlayer(animation: animation, pixels: (16, 16), clock: clock) { [shown] _, index in
            shown.withLock { $0.append(index) }
        }
    }

    // Delays 100, 200, 50, 300, 150 ms, three loops.
    @Test
    func framesShowAtTheirDelaysInOrder() async throws {
        let player = player(try animation())
        await player.prepare()

        player.start(at: 0)
        #expect(shown.current == [0])
        clock.advance(to: 0.05)
        #expect(shown.current == [0], "nothing before the first delay")
        clock.advance(to: 0.1)
        #expect(shown.current == [0, 1])
        clock.advance(to: 0.29)
        #expect(shown.current == [0, 1])
        clock.advance(to: 0.3)
        clock.advance(to: 0.35)
        clock.advance(to: 0.65)
        #expect(shown.current == [0, 1, 2, 3, 4])
        clock.advance(to: 0.8)
        #expect(shown.current == [0, 1, 2, 3, 4, 0], "the second loop")
        #expect(player.isPlaying)
    }

    // A late tick shows the frame that is due, not every one in between.
    @Test
    func aLateTickSkipsToTheDueFrame() async throws {
        let player = player(try animation())
        await player.prepare()

        player.start(at: 0)
        clock.advance(to: 0.36)

        #expect(shown.current == [0, 3])
    }

    @Test
    func theLoopCountEndsTheAnimationOnItsLastFrame() async throws {
        let player = player(try animation())
        await player.prepare()

        player.start(at: 0)
        clock.advance(to: 2.39)
        #expect(player.isPlaying)
        #expect(shown.current.last == 4)
        clock.advance(to: 2.4)
        clock.advance(to: 5)

        #expect(!player.isPlaying)
        #expect(shown.current.last == 4)
        #expect(clock.playerCount == 0)
    }

    @Test
    func stopEndsDeliveryAndLeavesTheClock() async throws {
        let player = player(try animation())
        await player.prepare()

        player.start(at: 0)
        clock.advance(to: 0.1)
        #expect(clock.playerCount == 1)
        player.stop()
        clock.advance(to: 1)

        #expect(shown.current == [0, 1])
        #expect(!player.isPlaying)
        #expect(clock.playerCount == 0)
    }

    @Test
    func framesAreDecodedAtTheBoxsPixels() async throws {
        let frames = Locked<[CGImage]>([])
        let player = FramePlayer(animation: try animation(), pixels: (16, 16), clock: clock) {
            image, _ in frames.withLock { $0.append(image) }
        }
        await player.prepare()

        player.start(at: 0)

        let frame = try #require(frames.current.first)
        #expect(frame.width == 16 && frame.height == 16)
    }

    // Without `prepare` the frames arrive as they decode: a tick never waits for one.
    @Test
    func aTickNeverWaitsForAFrame() async throws {
        let player = player(try animation())

        player.start(at: 0)
        clock.advance(to: 0.1)

        let before = shown.current
        #expect(before.count <= 2)
        await player.prepare()
        clock.advance(to: 0.3)
        #expect(shown.current.last == 2)
    }

    @Test
    func theClockRunsOnlyWhilePlayersAreRegistered() async throws {
        let automatic = AnimationClock(automatic: true)
        let player = FramePlayer(animation: try animation(), pixels: (16, 16), clock: automatic) {
            _, _ in
        }
        #expect(!automatic.isRunning)

        player.start()
        #expect(automatic.isRunning)
        player.stop()
        #expect(!automatic.isRunning)
    }

    @Test
    func aStillFileIsNoAnimation() throws {
        let images = TestImages()
        let still = try images.png("still", width: 32, height: 32)

        #expect(AnimationSource(url: still) == nil)
    }
}
