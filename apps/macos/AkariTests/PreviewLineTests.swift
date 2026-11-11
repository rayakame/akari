import AkariFFI
import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// A message on one line, as the thread box, the Threads browser and the forum card show it: a
// custom emoji is its image on a box of the line's text size, `:name:` only while it loads or
// when it fails, played by the list's rule for emoji.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PreviewLineTests {
    let images = TestImages()
    let window = TestWindow.make(size: NSSize(width: 400, height: 100))
    let now = berlinTime(2026, 10, 10, 15, 0)
    let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)

    func line(_ content: String, size: CGFloat = TypeScale.size(14)) -> NSAttributedString {
        let message = row(25, by: author(3, "Mira"), at: now, content: content).message
        return ThreadBoxView.content(
            MessageText.build(message, revealed: [], now: now, times: times).text, size: size)
    }

    func shown(_ content: String, cache: ImageCache? = nil, width: CGFloat = 300) -> PreviewLineView {
        let view = PreviewLineView()
        view.images = cache
        view.text = line(content)
        view.frame = NSRect(x: 10, y: 10, width: width, height: 18)
        window.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        return view
    }

    var emoji: ImageKey { .emoji(emojiId: EmojiId(rawValue: 5), animated: false) }

    // The box is the line's: 1.375 of its text, as Discord's, whatever the message was in the
    // list (an emoji-only message is jumbo there).
    @Test(arguments: ["look <:pog:5> there", "<:pog:5>", "# <:pog:5> loud"])
    func aCustomEmojiKeepsItsBoxAtTheLinesSize(content: String) throws {
        let text = line(content)

        var boxes: [MessageText.EmojiBox] = []
        text.enumerateAttribute(.akariEmoji, in: NSRange(location: 0, length: text.length)) { value, _, _ in
            if let box = value as? MessageText.EmojiBox {
                boxes.append(box)
            }
        }
        #expect(boxes.map(\.name) == ["pog"])
        #expect(boxes.first?.side == (TypeScale.size(14) * 1.375).rounded())
        #expect(boxes.first?.centered == false)
        #expect(!text.string.contains(":pog:"))
        #expect(text.size().height <= 20, "\(text.size())")
        #expect(MessageText.accessible(text).contains(":pog:"), "VoiceOver reads its name")
    }

    @Test
    func aCallerSetsTheBoxsSide() {
        let message = row(25, by: author(3, "Mira"), at: now, content: "<:pog:5>").message
        let text = ThreadBoxView.content(
            MessageText.build(message, revealed: [], now: now, times: times).text, size: 15, emojiSide: 18)

        let box = text.attribute(.akariEmoji, at: 0, effectiveRange: nil) as? MessageText.EmojiBox
        #expect(box?.side == 18)
    }

    // Where no image layer can stand (the gallery card's fading text): the name.
    @Test
    func namedGivesTheEmojiItsName() {
        #expect(ThreadBoxView.named(line("look <:pog:5> there")).string == "look :pog: there")
    }

    @Test
    func theImageStandsOnItsBoxInTheLine() async throws {
        let cache = ImageCache(source: FakeImageSource.serving(try images.png("pog", pixels: 64), pixels: 64))
        let view = shown("look <:pog:5> there", cache: cache)

        let layer = try #require(view.emojiViews.first)
        #expect(view.emojiViews.count == 1)
        #expect(layer.content?.key == emoji)
        #expect(layer.fallbackText == ":pog:")
        #expect(layer.fit == .contain)
        let side = (TypeScale.size(14) * 1.375).rounded()
        #expect(layer.frame.size == NSSize(width: side, height: side))
        let before = ("look " as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: TypeScale.size(14))]).width
        #expect(abs(layer.frame.minX - before) <= 1, "\(layer.frame) after \(before)")
        #expect(abs(layer.frame.midY - view.bounds.midY) <= 0.5, "\(layer.frame) in \(view.bounds)")
        try await eventually { layer.hasImage }
        #expect(layer.fallbackLayer.isHidden)
    }

    @Test
    func theNameShowsWhileTheImageLoadsAndWhenItFails() async throws {
        let loading = try #require(
            shown("<:pog:5>", cache: ImageCache(source: FakeImageSource.stalling())).emojiViews.first)
        #expect(!loading.fallbackLayer.isHidden)
        #expect(loading.fallbackLayer.string as? String == ":pog:")

        let failing = try #require(
            shown("<:pog:5>", cache: ImageCache(source: FakeImageSource.failing())).emojiViews.first)
        try await eventually { failing.failed }
        #expect(!failing.fallbackLayer.isHidden)
    }

    // The list's own emoji show no name while they load, as before.
    @Test
    func theListsEmojiKeepTheirBlankBoxWhileLoading() {
        let view = ImageLayerView()
        view.images = ImageCache(source: FakeImageSource.stalling())
        view.fallbackText = ":pog:"
        view.frame = NSRect(x: 0, y: 0, width: 22, height: 22)
        window.contentView?.addSubview(view)
        view.show(ImageLayerView.Content(key: emoji))
        view.layoutSubtreeIfNeeded()

        #expect(view.fallbackLayer.isHidden)
    }

    // An emoji the ellipsis cut off, or under a hidden spoiler, has no layer and asks nothing.
    @Test
    func aCutOrCoveredEmojiHasNoLayer() {
        let long = "a long line of words before the emoji that will not fit <:pog:5>"
        #expect(shown(long, width: 120).emojiViews.isEmpty)
        #expect(shown(long, width: 600).emojiViews.count == 1)
        #expect(shown("hi ||<:pog:5>||").emojiViews.isEmpty)
    }

    @Test
    func aNewTextReplacesItsLayers() {
        let view = shown("<:pog:5> <:wave:6>")
        #expect(view.emojiViews.count == 2)

        view.text = line("plain")
        view.layoutSubtreeIfNeeded()

        #expect(view.emojiViews.isEmpty)
        #expect(view.subviews.compactMap { $0 as? EmojiOverlay }.first?.subviews.isEmpty == true)
    }

    @Test
    func theLinePlaysItsEmojiWhenTold() {
        let view = shown("<a:dance:6>")
        #expect(view.emojiViews.first?.animates == false)

        view.animatesEmoji = true

        #expect(view.emojiViews.first?.animates == true)
        #expect(view.emojiViews.first?.content?.canAnimate == true)
    }

    // In SwiftUI places: the list's rule for emoji (the setting, or the pointer over the row).
    @Test
    func aSwiftUILinePlaysByTheEmojiRule() throws {
        var still = MotionPolicy.default
        still.animateEmoji = false
        func plays(_ motion: MotionPolicy, focused: Bool, hovered: Bool) throws -> Bool {
            let host = NSHostingView(
                rootView: PreviewLine(text: line("<a:dance:6>"), lineHeight: 18, hovered: hovered)
                    .environment(\.motion, motion)
                    .environment(\.windowFocused, focused)
                    .frame(width: 200, height: 18))
            host.frame = NSRect(x: 0, y: 40, width: 200, height: 18)
            window.contentView?.addSubview(host)
            host.layoutSubtreeIfNeeded()
            defer { host.removeFromSuperview() }
            let view = try #require(Self.find(PreviewLineView.self, in: host))
            return view.animatesEmoji
        }

        #expect(try plays(.default, focused: true, hovered: false))
        #expect(try !plays(.default, focused: false, hovered: false), "the window isn't key")
        #expect(try !plays(still, focused: true, hovered: false))
        #expect(try plays(still, focused: true, hovered: true), "hovered")
    }

    // The line is as wide as its text and gives way with an ellipsis.
    @Test
    func aSwiftUILineIsItsTextsWidthAtMost() throws {
        let text = line("look <:pog:5> there")
        let ideal = ceil(text.size().width)
        for (offered, expected) in [(CGFloat(300), ideal), (CGFloat(40), CGFloat(40))] {
            let host = NSHostingView(
                rootView: HStack(spacing: 0) {
                    PreviewLine(text: text, lineHeight: 18)
                    Spacer(minLength: 0)
                }
                .frame(width: offered, height: 18))
            host.frame = NSRect(x: 0, y: 40, width: offered, height: 18)
            window.contentView?.addSubview(host)
            host.layoutSubtreeIfNeeded()
            defer { host.removeFromSuperview() }
            let view = try #require(Self.find(PreviewLineView.self, in: host))
            #expect(abs(view.frame.width - expected) <= 1, "\(view.frame.width) for \(offered)")
            #expect(view.frame.height == 18)
        }
    }

    // A view in the window with a red picture for every image, once its emoji's has landed.
    func hosted<Content: View>(_ view: Content, width: CGFloat, height: CGFloat) async throws
        -> (host: NSView, line: PreviewLineView)
    {
        let red = try images.png("red", pixels: 64, color: [255, 0, 0, 255])
        let cache = ImageCache(source: FakeImageSource.serving(red, pixels: 64))
        let host = NSHostingView(
            rootView: view.environment(\.imageCache, cache).frame(width: width, height: height))
        host.frame = NSRect(x: 0, y: 0, width: width, height: height)
        window.appearance = NSAppearance(named: .darkAqua)
        window.contentView?.addSubview(host)
        host.layoutSubtreeIfNeeded()
        let line = try #require(Self.find(PreviewLineView.self, in: host))
        try await eventually { line.emojiViews.first?.hasImage == true }
        return (host, line)
    }

    func message(_ content: String) -> Message {
        row(25, by: author(3, "Mira"), at: now, content: content).message
    }

    // The rows (points from the top) of bright ink in `host` between two columns.
    func ink(_ host: NSView, x0: CGFloat, x1: CGFloat) throws -> (top: CGFloat, bottom: CGFloat) {
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let rows = (0..<rep.pixelsHigh).filter { row in
            (Int(x0 * scale)..<Int(x1 * scale)).contains { column in
                (rep.colorAt(x: column, y: row)?.brightnessComponent ?? 0) > 0.6
            }
        }
        let (first, last) = (try #require(rows.first), try #require(rows.last))
        return (CGFloat(first) / scale, CGFloat(last + 1) / scale)
    }

    // The forum card: Discord's 18 high emoji beside "Ray:", the message on the author's
    // baseline.
    @Test
    func theForumCardsLineShowsTheEmojisImage() async throws {
        let (host, line) = try await hosted(
            ForumCardMessage(preview: .message(message("TTT <:pog:5>"), authorName: "TTT"), unread: true),
            width: 300, height: 18)

        let layer = try #require(line.emojiViews.first)
        #expect(layer.frame.size == NSSize(width: 18, height: 18))
        #expect(layer.content?.key == emoji)
        let box = layer.convert(layer.bounds, to: host)
        #expect(box.minY >= -0.5 && box.maxY <= 18.5, "inside the line: \(box)")
        let left = line.convert(line.bounds, to: host).minX
        let author = try ink(host, x0: 0, x1: left - 4)
        let text = try ink(host, x0: left, x1: left + layer.frame.minX - 2)
        #expect(abs(author.bottom - text.bottom) <= 0.5, "one baseline: \(author) and \(text)")
        #expect(abs(author.top - text.top) <= 0.5, "one size: \(author) and \(text)")
    }

    // The Threads browser's row.
    @Test
    func theThreadsBrowsersLineShowsTheEmojisImage() async throws {
        let (host, line) = try await hosted(
            ThreadBrowserSubtitle(
                subtitle: .latest(message("TTT <a:pog:5>"), authorName: "TTT", time: now - 600), now: now),
            width: 400, height: 18)

        let layer = try #require(line.emojiViews.first)
        let side = (TypeScale.size(14) * 1.375).rounded()
        #expect(layer.frame.size == NSSize(width: side, height: side))
        #expect(layer.content?.key == .emoji(emojiId: EmojiId(rawValue: 5), animated: true))
        let left = line.convert(line.bounds, to: host).minX
        // The author's capitals, before ":" and the line.
        let name = try ink(host, x0: left - 34, x1: left - 12)
        let text = try ink(host, x0: left, x1: left + layer.frame.minX - 2)
        #expect(abs(name.bottom - text.bottom) <= 0.5, "one baseline: \(name) and \(text)")
    }

    // The gallery card's text fades over several lines: no layers there, the emoji by name.
    @Test
    func theGalleryCardsTextNamesTheEmoji() {
        let line = ThreadBoxView.named(ForumCardMessage.line(of: message("look <:pog:5>")))
        #expect(line.string == "look :pog:")
    }

    static func find<View: NSView>(_ kind: View.Type, in view: NSView) -> View? {
        if let found = view as? View {
            return found
        }
        for subview in view.subviews {
            if let found = find(kind, in: subview) {
                return found
            }
        }
        return nil
    }
}
