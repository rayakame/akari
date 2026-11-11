import AkariFFI
import AkariKit
import AppKit
import Testing

@testable import Akari

// The GIF that never played, by its numbers (327 frames of 540 × 405 in 10 159 614 bytes, 286 MB
// decoded), as the embed of a posted link: it plays in the list and in the viewer on the app's
// own clock, frame after frame.
@MainActor @Suite(.serialized, .timeLimit(.minutes(2)))
final class HeavyGifTests {
    let images = TestImages()
    let gif = embedImage(540, 405, slot: .thumbnail, animated: true)

    func cache() async throws -> ImageCache {
        ImageCache(
            source: try await FakeImageSource.heavyGif(still: try images.png("still", width: 540, height: 405)))
    }

    // Frames in order, each the one after the last or a few on: the clock skips, it never turns
    // back or stands still.
    func expectAdvancing(_ frames: [Int], sourceLocation: SourceLocation = #_sourceLocation) {
        let steps = zip(frames, frames.dropFirst()).map { ($1 - $0 + 327) % 327 }
        #expect(steps.allSatisfy { (1...8).contains($0) }, "\(frames)", sourceLocation: sourceLocation)
    }

    @Test
    func itPlaysInTheList() async throws {
        let list = MessageTableTests(onScreen: { _ in true })
        list.window.setContentSize(NSSize(width: 900, height: 700))
        let cache = try await cache()
        var shown: [Int] = []

        let lines = try await imageLog {
            list.show(
                MessageTableState(
                    rows: [
                        row(
                            1, at: list.noon, content: "https://cdn.example/loop.gif",
                            embeds: [embed(kind: .image, inline: true, title: nil, description: nil, media: [gif])],
                            hidesContent: true)
                    ],
                    reachedOldest: true, images: ImageCacheRef(cache), motion: .default, focused: true))
            let cell = try #require(try list.cell(of: 1) as? MessageCell)
            let embeds = try #require(cell.embeds)
            let tile = try #require(embeds.tiles.first)
            try await eventually { tile.picture.player?.isPlaying == true }
            shown = try await frames(of: tile.picture, count: 40)
        }

        #expect(shown.count == 40)
        expectAdvancing(shown)
        #expect(lines.contains { $0.hasPrefix("list: embed 0 thumbnail (message 1) plays") }, "\(lines)")
        #expect(lines.contains { $0.contains("starts: 327 frames of 540×405") }, "\(lines)")
        #expect(lines.contains { $0.contains("shows frame") && $0.hasSuffix("of 327") }, "\(lines)")
    }

    @Test
    func itPlaysInTheViewer() async throws {
        let window = TestWindow.hide(
            ViewerTests.KeyWindow(
                contentRect: NSRect(x: 0, y: 0, width: 1280, height: 800), styleMask: [.titled],
                backing: .buffered, defer: false))
        window.isReleasedWhenClosed = false
        window.animationBehavior = .none
        window.contentView = NSView()
        window.orderFrontRegardless()
        let viewer = MediaViewer()
        viewer.reducesMotion = { true }
        viewer.focus = WindowFocus { _ in }
        viewer.plays = { _ in true }
        defer {
            viewer.close()
            window.close()
        }
        let cache = try await cache()
        let model = try #require(
            ViewerModel(
                items: [ViewerModel.Item(gif)], index: 0, author: author(2, "Ray"), authorName: "Ray",
                timestamp: Date(), hidesDownloads: false, account: ViewerAccount()))
        var shown: [Int] = []

        let lines = try await imageLog {
            viewer.open(model, in: window, images: cache)
            let view = try #require(viewer.view)
            let picture = view.stage.tile.picture
            try await eventually { picture.player?.isPlaying == true }
            shown = try await frames(of: picture, count: 40)
        }

        #expect(shown.count == 40)
        expectAdvancing(shown)
        #expect(lines.contains { $0.contains("starts: 327 frames of 540×405") }, "\(lines)")
        #expect(lines.contains { $0.contains("shows frame") && $0.hasSuffix("of 327") }, "\(lines)")
    }
}
