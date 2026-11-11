import AkariFFI
import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// A message on one line (thread box, Threads browser, forum card): a custom emoji is its image
// at the line's text size, `:name:` until the image is here, played by the list's emoji rule.
@MainActor @Suite(.timeLimit(.minutes(1)))
final class PreviewLineTests {
    let images = TestImages()
    let window = TestWindow.make(size: NSSize(width: 400, height: 100))
    let now = berlinTime(2026, 10, 10, 15, 0)
    let times = TimestampText(locale: .current, timeZone: .current, calendar: .current)

    init() {
        window.appearance = NSAppearance(named: .darkAqua)
    }

    func message(_ content: String) -> Message {
        row(25, by: author(3, "Mira"), at: now, content: content).message
    }

    func line(_ content: String, size: CGFloat = TypeScale.size(14)) -> NSAttributedString {
        ThreadBoxView.content(
            MessageText.build(message(content), revealed: [], now: now, times: times).text, size: size)
    }

    // Every image is a red picture.
    func red() throws -> FakeImageSource {
        FakeImageSource.serving(try images.png("red", pixels: 64, color: [255, 0, 0, 255]), pixels: 64)
    }

    func shown(_ content: String, source: FakeImageSource? = nil, width: CGFloat = 300) -> PreviewLineView {
        let view = PreviewLineView()
        view.images = source.map { ImageCache(source: $0) }
        view.text = line(content)
        view.frame = NSRect(x: 10, y: 10, width: width, height: 18)
        window.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        return view
    }

    // The line with its images here.
    func loaded(_ content: String, width: CGFloat = 300) async throws -> PreviewLineView {
        let view = shown(content, source: try red(), width: width)
        try await eventually {
            view.layoutSubtreeIfNeeded()
            return view.emojiViews.first?.hasImage == true
        }
        return view
    }

    var emoji: ImageKey { .emoji(emojiId: EmojiId(rawValue: 5), animated: false) }
    var side: CGFloat { (TypeScale.size(14) * 1.375).rounded() }

    // The box is the line's, whatever the message was in the list (emoji-only is jumbo there).
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
        #expect(boxes.first?.side == side)
        #expect(boxes.first?.centered == false)
        #expect(!text.string.contains(":pog:"))
        #expect(text.size().height <= 20, "\(text.size())")
        #expect(MessageText.accessible(text).contains(":pog:"), "VoiceOver reads its name")
    }

    @Test
    func aCallerSetsTheBoxsSide() {
        let text = ThreadBoxView.content(
            MessageText.build(message("<:pog:5>"), revealed: [], now: now, times: times).text,
            size: 15, emojiSide: 18)

        let box = text.attribute(.akariEmoji, at: 0, effectiveRange: nil) as? MessageText.EmojiBox
        #expect(box?.side == 18)
    }

    // Symbols (a channel mention's "#") have no place on the line; only emoji keep a box.
    @Test
    func otherAttachmentsLeaveTheLine() {
        let text = line("see <#11> <:pog:5>")

        var attachments = 0
        text.enumerateAttribute(.attachment, in: NSRange(location: 0, length: text.length)) { value, range, _ in
            if value != nil {
                attachments += range.length
            }
        }
        #expect(attachments == 1)
    }

    @Test
    func theImageStandsOnItsBoxInTheLine() async throws {
        let view = try await loaded("look <a:pog:5> there")

        let layer = try #require(view.emojiViews.first)
        #expect(view.emojiViews.count == 1)
        #expect(layer.content?.key == .emoji(emojiId: EmojiId(rawValue: 5), animated: true))
        #expect(layer.content?.canAnimate == true)
        #expect(layer.fit == .contain)
        #expect(layer.frame.size == NSSize(width: side, height: side))
        let before = ("look " as NSString).size(
            withAttributes: [.font: NSFont.systemFont(ofSize: TypeScale.size(14))]).width
        #expect(abs(layer.frame.minX - before) <= 1, "\(layer.frame) after \(before)")
        #expect(abs(layer.frame.midY - view.bounds.midY) <= 0.5, "\(layer.frame) in \(view.bounds)")
        #expect(layer.imageLayer.contents != nil && !layer.imageLayer.isHidden)
        #expect(layer.superview?.frame == view.bounds, "the layers' view covers the line")
        #expect(!view.shown.string.contains(":pog:"))
    }

    // Until the image is decoded the line reads the emoji's name, in full; so it does when the
    // image can't be had.
    @Test
    func theNameStandsInUntilTheImageIsHere() async throws {
        let gate = Gate()
        let picture = try images.png("red", pixels: 64, color: [255, 0, 0, 255])
        let source = FakeImageSource.serving(picture, pixels: 64, gate: gate)
        let view = shown("look <:pog:5> there", source: source)
        #expect(view.shown.string == "look :pog: there")
        #expect(view.emojiViews.isEmpty)

        gate.fire()
        try await eventually {
            view.layoutSubtreeIfNeeded()
            return view.emojiViews.count == 1
        }
        #expect(view.shown.string == "look \u{FFFC} there")

        let refused = FakeImageSource.failing()
        let failing = shown("look <:pog:5> there", source: refused)
        try await eventually { refused.count >= 1 }
        try await Task.sleep(for: .milliseconds(20))
        failing.layoutSubtreeIfNeeded()
        #expect(failing.shown.string == "look :pog: there")
        #expect(failing.emojiViews.isEmpty)
    }

    // An image the cache already has takes its box at once: no name flashes.
    @Test
    func aDecodedImageTakesItsBoxAtOnce() async throws {
        let source = try red()
        let cache = ImageCache(source: source)
        let first = PreviewLineView()
        first.images = cache
        first.text = line("<:pog:5>")
        first.frame = NSRect(x: 10, y: 10, width: 300, height: 18)
        window.contentView?.addSubview(first)
        try await eventually {
            first.layoutSubtreeIfNeeded()
            return first.emojiViews.first?.hasImage == true
        }

        let second = PreviewLineView()
        second.images = cache
        second.text = line("again <:pog:5>")
        second.frame = NSRect(x: 10, y: 40, width: 300, height: 18)
        window.contentView?.addSubview(second)
        second.layoutSubtreeIfNeeded()

        #expect(second.shown.string == "again \u{FFFC}")
        #expect(second.emojiViews.count == 1)
        #expect(source.count == 1, "one download for both")
    }

    // The session is back online: a line whose image failed asks once more.
    @Test
    func aFailedImageIsAskedAgainWhenTheCacheSaysSo() async throws {
        let source = FakeImageSource.failing()
        let cache = ImageCache(source: source)
        let view = PreviewLineView()
        view.images = cache
        view.text = line("<:pog:5>")
        view.frame = NSRect(x: 10, y: 10, width: 300, height: 18)
        window.contentView?.addSubview(view)
        view.layoutSubtreeIfNeeded()
        try await eventually { source.count == 1 }
        view.layoutSubtreeIfNeeded()
        #expect(source.count == 1, "not asked again by a layout")

        let picture = try images.png("red", pixels: 64, color: [255, 0, 0, 255])
        source.replace { _ in
            ImageFile(path: picture.path, width: 64, height: 64, animated: false, fromDisk: false)
        }
        cache.retryFailed()

        try await eventually {
            view.layoutSubtreeIfNeeded()
            return view.emojiViews.first?.hasImage == true
        }
    }

    // An emoji the ellipsis cut off, or under a hidden spoiler, isn't asked for.
    @Test
    func aCutOrCoveredEmojiAsksNothing() async throws {
        let long = "a long line of words before the emoji that will not fit <:pog:5>"
        let cut = try red()
        #expect(shown(long, source: cut, width: 120).emojiViews.isEmpty)
        let covered = try red()
        #expect(shown("hi ||<:pog:5>||", source: covered).emojiViews.isEmpty)
        let whole = try await loaded(long, width: 600)

        #expect(whole.emojiViews.count == 1)
        #expect(cut.count == 0 && covered.count == 0)
    }

    // The same emoji beside itself, one of the two under a hidden spoiler.
    @Test
    func aCoveredEmojiBesideItsTwinHasNoLayer() async throws {
        let view = try await loaded("<:pog:5>||<:pog:5>||")

        #expect(view.emojiViews.count == 1)
        #expect(view.emojiViews.first?.frame.minX == 0)
    }

    // SwiftUI builds the line again with every update: the same line keeps its layers.
    @Test
    func theSameLineBuiltAgainChangesNothing() async throws {
        let source = try red()
        let view = shown("look <:pog:5>", source: source)
        try await eventually {
            view.layoutSubtreeIfNeeded()
            return view.emojiViews.first?.hasImage == true
        }
        let layer = view.emojiViews.first
        let asked = source.count

        view.text = line("look <:pog:5>")
        view.layoutSubtreeIfNeeded()

        #expect(view.emojiViews.first === layer)
        #expect(view.shown.string == "look \u{FFFC}")
        #expect(source.count == asked)
    }

    // Whatever ends a line in the message is a space on the one line; an object replacement
    // character someone typed is nothing.
    @Test
    func everyLineEndIsASpace() {
        #expect(line("one\ntwo").string == "one two")
        let built = NSAttributedString(
            string: "one\rtwo\u{2029}three\u{2028}four \u{FFFC}five",
            attributes: [.font: NSFont.systemFont(ofSize: 16)])
        #expect(ThreadBoxView.content(built).string == "one two three four five")
    }

    @Test
    func aNewTextReplacesItsLayers() async throws {
        let view = try await loaded("<:pog:5> <:wave:6>")
        #expect(view.emojiViews.count == 2)

        view.text = line("plain")
        view.layoutSubtreeIfNeeded()

        #expect(view.emojiViews.isEmpty)
        #expect(view.shown.string == "plain")
    }

    // Told before or after its emoji are there.
    @Test
    func theLinePlaysItsEmojiWhenTold() async throws {
        let before = PreviewLineView()
        before.animatesEmoji = true
        before.images = ImageCache(source: try red())
        before.text = line("<a:dance:6>")
        before.frame = NSRect(x: 10, y: 10, width: 300, height: 18)
        window.contentView?.addSubview(before)
        try await eventually {
            before.layoutSubtreeIfNeeded()
            return before.emojiViews.first?.hasImage == true
        }
        #expect(before.emojiViews.first?.animates == true)

        let after = try await loaded("<a:dance:6>")
        #expect(after.emojiViews.first?.animates == false)
        after.animatesEmoji = true
        #expect(after.emojiViews.first?.animates == true)
    }

    // `view` in the window on the chat's color, at `scale`, with a red picture for every image.
    func hosted<Content: View>(
        _ view: Content, width: CGFloat, height: CGFloat, scale: CGFloat = 2, motion: MotionPolicy = .default,
        hovered: Bool = false, awaitsImage: Bool = true
    ) async throws -> (host: NSView, line: PreviewLineView) {
        let cache = ImageCache(source: try red())
        let scaled = ScaledWindow.make(scale: scale)
        scaled.appearance = NSAppearance(named: .darkAqua)
        scaled.setContentSize(NSSize(width: width, height: height))
        let host = NSHostingView(
            rootView: view.environment(\.imageCache, cache).environment(\.motion, motion)
                .environment(\.windowFocused, true).environment(\.rowHovered, hovered)
                .frame(width: width, height: height).background(Color(nsColor: Palette.chat)))
        scaled.contentView = host
        host.layoutSubtreeIfNeeded()
        let line = try #require(Self.find(PreviewLineView.self, in: host))
        if awaitsImage {
            try await eventually {
                host.layoutSubtreeIfNeeded()
                return line.emojiViews.first?.hasImage == true
            }
        }
        windows.append(scaled)
        return (host, line)
    }

    private var windows: [NSWindow] = []

    // The rows (points from the top) of bright, opaque ink in `host` between two columns.
    func ink(_ host: NSView, x0: CGFloat, x1: CGFloat) throws -> (top: CGFloat, bottom: CGFloat) {
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let rows = (0..<rep.pixelsHigh).filter { row in
            (Int(x0 * scale)..<Int(x1 * scale)).contains { column in
                guard let color = rep.colorAt(x: column, y: row)?.usingColorSpace(.sRGB) else {
                    return false
                }
                return color.alphaComponent > 0.5 && color.brightnessComponent > 0.6
                    && color.saturationComponent < 0.3
            }
        }
        let (first, last) = (try #require(rows.first), try #require(rows.last))
        return (CGFloat(first) / scale, CGFloat(last + 1) / scale)
    }

    func width(_ text: String, _ font: NSFont) -> CGFloat {
        (text as NSString).size(withAttributes: [.font: font]).width
    }

    // The forum card: Discord's 18 high emoji beside the author, the message on the author's
    // baseline and at its size.
    @Test(arguments: [1, 2] as [CGFloat])
    func theForumCardsLineShowsTheEmojisImage(scale: CGFloat) async throws {
        let (host, line) = try await hosted(
            ForumCardMessage(preview: .message(message("TTT <:pog:5>"), authorName: "TTT"), unread: true),
            width: 300, height: 18, scale: scale)

        let layer = try #require(line.emojiViews.first)
        #expect(layer.frame.size == NSSize(width: 18, height: 18))
        #expect(layer.content?.key == emoji)
        #expect(layer.imageLayer.contents != nil && !layer.imageLayer.isHidden)
        let box = layer.convert(layer.bounds, to: host)
        #expect(box.minY >= -0.5 && box.maxY <= 18.5, "inside the line: \(box)")
        let left = line.convert(line.bounds, to: host).minX
        let author = try ink(host, x0: 0, x1: left - 4)
        let text = try ink(host, x0: left, x1: left + layer.frame.minX - 2)
        let near = scale == 1 ? 1 : 0.5
        #expect(abs(author.bottom - text.bottom) <= near, "one baseline: \(author) and \(text)")
        #expect(abs(author.top - text.top) <= near, "one size: \(author) and \(text)")
    }

    // The Threads browser's row.
    @Test(arguments: [1, 2] as [CGFloat])
    func theThreadsBrowsersLineShowsTheEmojisImage(scale: CGFloat) async throws {
        let (host, line) = try await hosted(
            ThreadBrowserSubtitle(
                subtitle: .latest(message("TTT <a:pog:5>"), authorName: "TTT", time: now - 600), now: now),
            width: 400, height: 18, scale: scale)

        let layer = try #require(line.emojiViews.first)
        #expect(layer.frame.size == NSSize(width: side, height: side))
        #expect(layer.content?.key == .emoji(emojiId: EmojiId(rawValue: 5), animated: true))
        #expect(layer.imageLayer.contents != nil && !layer.imageLayer.isHidden)
        let left = line.convert(line.bounds, to: host).minX
        // The author's capitals, before ":" and the line.
        let font = TypeScale.browserText.nsFont
        let colon = width(":\u{00a0}", font)
        let name = width("TTT", NSFont.systemFont(ofSize: font.pointSize, weight: .semibold))
        let author = try ink(host, x0: left - colon - name + 1, x1: left - colon - 1)
        let text = try ink(host, x0: left, x1: left + layer.frame.minX - 2)
        let near = scale == 1 ? 1 : 0.5
        #expect(abs(author.bottom - text.bottom) <= near, "one baseline: \(author) and \(text)")
        #expect(abs(author.top - text.top) <= near, "one size: \(author) and \(text)")
    }

    // The list's rule for emoji: the setting, or the pointer over the row or card; neither
    // while the window isn't key.
    @Test
    func aSwiftUILinePlaysByTheEmojiRule() throws {
        var still = MotionPolicy.default
        still.animateEmoji = false
        func plays(_ motion: MotionPolicy, focused: Bool, hovered: Bool) throws -> Bool {
            let host = NSHostingView(
                rootView: PreviewLine(text: line("<a:dance:6>"), lineHeight: 18)
                    .environment(\.motion, motion)
                    .environment(\.windowFocused, focused)
                    .environment(\.rowHovered, hovered)
                    .frame(width: 200, height: 18))
            host.frame = NSRect(x: 0, y: 40, width: 200, height: 18)
            window.contentView?.addSubview(host)
            host.layoutSubtreeIfNeeded()
            defer { host.removeFromSuperview() }
            return try #require(Self.find(PreviewLineView.self, in: host)).animatesEmoji
        }

        #expect(try plays(.default, focused: true, hovered: false))
        #expect(try !plays(.default, focused: false, hovered: false), "the window isn't key")
        #expect(try !plays(still, focused: true, hovered: false))
        #expect(try plays(still, focused: true, hovered: true), "hovered")
        #expect(try !plays(still, focused: false, hovered: true), "hovered, the window not key")
    }

    // The forum card's message and the browser's row take the hover their card gives them.
    @Test
    func theCardsAndRowsHoverReachesTheirLine() async throws {
        var still = MotionPolicy.default
        still.animateEmoji = false
        for hovered in [true, false] {
            let card = try await hosted(
                ForumCardMessage(preview: .message(message("<a:dance:6>"), authorName: "TTT")),
                width: 300, height: 18, motion: still, hovered: hovered, awaitsImage: false)
            #expect(card.line.animatesEmoji == hovered)
            let row = try await hosted(
                ThreadBrowserSubtitle(
                    subtitle: .latest(message("<a:dance:6>"), authorName: "TTT", time: now - 600), now: now),
                width: 400, height: 18, motion: still, hovered: hovered, awaitsImage: false)
            #expect(row.line.animatesEmoji == hovered)
        }
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

    // Where no image layer can stand (the gallery card's fading text): the name.
    @Test
    func theGalleryCardsTextNamesTheEmoji() {
        #expect(ThreadBoxView.named(line("look <:pog:5> there")).string == "look :pog: there")
        #expect(ForumCardMessage.fading(of: message("look <:pog:5>")).string == "look :pog:")
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
