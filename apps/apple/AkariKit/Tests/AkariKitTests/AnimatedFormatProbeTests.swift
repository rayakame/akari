import CoreGraphics
import Foundation
import Testing

@testable import AkariKit

// The launch probe: Akari asks animations as WebP where ImageIO composites them right, else
// as GIF (macOS 14 disposes frame 1 to frame 0 instead of clearing it; amendment 277).
@Suite(.timeLimit(.minutes(1)))
struct AnimatedFormatProbeTests {
    @Test
    func theFixtureShipsWithAkariKit() throws {
        let url = try #require(AnimatedFormatProbe.fixtureURL)

        #expect(url.lastPathComponent == "animated.webp")
        #expect(FileManager.default.fileExists(atPath: url.path))
    }

    // A clear left half of frame 2 is a right composite; anything else is the macOS 14 bug.
    @Test
    func theDecisionReadsFrameTwosLeftHalf() {
        #expect(AnimatedFormatProbe.decide(leftHalf: [0, 0, 0, 0]) == .webP)
        #expect(AnimatedFormatProbe.decide(leftHalf: [0, 0, 0, 2]) == .webP)
        #expect(AnimatedFormatProbe.decide(leftHalf: [255, 0, 0, 255]) == .gif)
        #expect(AnimatedFormatProbe.decide(leftHalf: nil) == .gif)
    }

    @Test
    func thisMacDecodesTheFixtureRight() throws {
        let decided = AnimatedFormatProbe.probe(try #require(AnimatedFormatProbe.fixtureURL))

        let expected: AnimatedFormat =
            ProcessInfo.processInfo.operatingSystemVersion.majorVersion == 14 ? .gif : .webP
        #expect(decided == expected)
    }

    @Test
    func aMissingOrUnreadableFileIsGif() throws {
        let missing = URL(fileURLWithPath: "/nonexistent/animated.webp")
        #expect(AnimatedFormatProbe.probe(missing) == .gif)

        let text = FileManager.default.temporaryDirectory.appending(
            path: "probe-\(UUID().uuidString).webp")
        try Data("not an image".utf8).write(to: text)
        defer { try? FileManager.default.removeItem(at: text) }
        #expect(AnimatedFormatProbe.probe(text) == .gif)
    }

    @Test
    func runDecidesOffTheMainActor() async {
        let decided = await AnimatedFormatProbe.run()

        let expected: AnimatedFormat =
            ProcessInfo.processInfo.operatingSystemVersion.majorVersion == 14 ? .gif : .webP
        #expect(decided == expected)
    }
}
