import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Pictures, not assertions: with TEST_RUNNER_AKARI_RENDERS=1 the reactions are drawn at 2x in
// both themes into the test host's temporary folder (the RENDER lines name the files).
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct ReactionRenders {
    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("reaction-renders")
    let images = TestImages()

    func save(_ rep: NSBitmapImageRep, _ name: String, theme: String, note: String = "") throws {
        let data = try #require(rep.representation(using: .png, properties: [:]))
        try FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        let file = Self.folder.appendingPathComponent("\(name)-\(theme).png")
        try data.write(to: file)
        print("RENDER \(file.path) \(note)")
    }

    func write(_ list: BarList, _ name: String, theme: String, note: String = "") throws {
        list.window.layoutIfNeeded()
        list.window.displayIfNeeded()
        let view = try #require(list.window.contentView)
        let chat = Palette.chat
        var backdrop = NSColor.black
        view.effectiveAppearance.performAsCurrentDrawingAppearance {
            backdrop = chat.usingColorSpace(.sRGB) ?? chat
        }
        try save(
            try AttachmentDrawingTests.layerImage(of: view, scale: 2, over: backdrop), name,
            theme: theme, note: note)
    }

    func frame(_ view: NSView, in list: BarList) throws -> String {
        let root = try #require(list.window.contentView)
        let box = view.convert(view.bounds, to: root)
        let top = root.isFlipped ? box.minY : root.bounds.height - box.maxY
        return "x \(box.minX * 2) y \(top * 2) w \(box.width * 2) h \(box.height * 2)"
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func rows(_ name: NSAppearance.Name) async throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = BarList(scale: 2, height: 560)
        list.window.appearance = NSAppearance(named: name)
        let cache = ImageCache(
            source: FakeImageSource.serving(
                try images.png("party", pixels: 96, color: [240, 180, 40, 255]), pixels: 96))
        let quick = ["💯", "😆", "💖"].map { QuickReaction(emoji: .unicode($0), mine: $0 == "😆") }
        list.actions.bars[1] = ([.addReaction, .createThread, .more], [.copyLink, .more])
        list.actions.quick[1] = quick
        let many = ["😀", "🙏", "🔥", "👀", "✅", "🚀", "😭", "🤔", "🥳", "💀", "🫡", "❤️", "👏", "🤝"]
            .enumerated().map { pill($1, UInt32(1 + $0 * 7 % 23), mine: $0 == 3) }
        let rows = [
            row(1, by: author(2, "Ray"), at: list.noon, content: "Plain, mine and a custom emoji.")
                .reacting(reactions(pill("👍", 3), pill("🎉", 1, mine: true), customPill(7, "party", 12))),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "Pills that only show, no add button.")
                .reacting(reactions(pill("👍", 128), pill("🎉", 4, mine: true), adds: false, toggles: false)),
            row(3, by: author(1, "Chris"), at: list.noon + 600, content: "Enough reactions for a second line.")
                .reacting(ReactionRow(pills: many, adds: true, toggles: true)),
            row(4, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin)
                .reacting(reactions(pill("👋", 5))),
        ]
        var state = MessageTableState(rows: rows, reachedOldest: true)
        state.images = ImageCacheRef(cache)
        list.controller.show(state)
        list.window.layoutIfNeeded()
        let first = try #require(try list.cell(of: 1) as? MessageCell)
        let pills = try #require(first.reactions)
        pills.layoutSubtreeIfNeeded()
        try await eventually { pills.pills[2].picture.imageLayer.contents != nil }
        let wrapped = try #require((try list.cell(of: 3) as? MessageCell)?.reactions)
        wrapped.layoutSubtreeIfNeeded()

        let notes =
            "pill1 \(try frame(pills.pills[0], in: list)) | mine \(try frame(pills.pills[1], in: list))"
            + " | custom \(try frame(pills.pills[2], in: list)) | add \(try frame(pills.add, in: list))"
            + " | wrapRow \(try frame(wrapped, in: list)) | wrapLast \(try frame(try #require(wrapped.pills.last), in: list))"
        try write(list, "1-rows", theme: theme, note: notes)

        list.move(to: list.table.convert(NSPoint(x: 20, y: 15), from: pills.pills[0]))
        try write(list, "2-pill-hovered-with-bar", theme: theme, note: "bar \(try frame(list.bar, in: list))")
        list.move(to: list.table.convert(NSPoint(x: 17, y: 15), from: pills.add))
        try write(list, "3-add-hovered", theme: theme)
        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: list.bar.quickButtons[1]))
        try write(list, "4-quick-reaction-hovered", theme: theme)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func postBar(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        for (file, bar) in [
            ("5-post-bar-default-reaction", PostBar(
                post: MessageId(rawValue: 1), following: false, canFollow: true, invites: false,
                defaultReaction: .unicode("🔥"), reacts: true, toggles: true)),
            ("6-post-bar-reactions", PostBar(
                post: MessageId(rawValue: 1), following: true, canFollow: true, invites: false,
                reactions: (0..<9).map { pill(["👍", "🎉", "🔥", "👀", "✅", "🚀", "😭", "🤔", "🥳"][$0], UInt32($0 + 1), mine: $0 == 1) },
                reacts: true, toggles: true)),
        ] {
            let list = BarList(scale: 2, height: 220)
            list.window.appearance = NSAppearance(named: name)
            var state = MessageTableState(rows: [
                row(1, by: author(2, "Ray"), at: list.noon, content: "A forum post's first message."),
                row(2, by: author(3), at: list.noon + 600, content: "An answer."),
            ])
            state.reachedOldest = true
            state.postBar = bar
            list.controller.show(state)
            try write(list, file, theme: theme)
        }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func window(_ name: NSAppearance.Name) async throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let account = ReactionsAccount(
            [reaction("👍", 2, me: true), reaction("🎉", 14), reaction("🔥", 5), reaction("👀", 128)],
            people: [
                "👀": [
                    person(1, "Chris", "schwarzlicht"), person(2, "Ray", "ray"),
                    person(3, "Mira Belle", "mira.belle"), person(4, "Kim", "kim"),
                    person(5, "A very long display name that is cut at the row's end", "longname"),
                ]
            ], moderates: true)
        let session = account.window()
        let reactors = try #require(session.reactors)
        await reactors.open()
        let size = NSSize(width: 600, height: 480)
        let image = try Rendered(
            ReactorsPanel(reactors: reactors, size: size, close: {})
                .frame(width: size.width, height: size.height)
                .background(Color(nsColor: Palette.chat))
                .ignoresSafeArea(),
            appearance: name, size: size)
        try save(image.rep, "7-reactions-window", theme: theme)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func cards(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let chips = HStack(spacing: 12) {
            ForumCardReaction(reaction: .init(emoji: .unicode("🔥"), count: 0, mine: false, toggles: true))
            ForumCardReaction(reaction: .init(emoji: .unicode("👍"), count: 12, mine: false, toggles: true))
            ForumCardReaction(reaction: .init(emoji: .unicode("🎉"), count: 3, mine: true, toggles: true))
            ForumCardCount(count: "24", size: 14)
        }
        .padding(20)
        .frame(width: 300, height: 60)
        .background(Color(nsColor: Palette.chat))
        let image = try Rendered(chips, appearance: name, size: NSSize(width: 300, height: 60))
        try save(image.rep, "8-forum-card-reactions", theme: theme)
    }
}
