import AkariKit
import AppKit
import Testing

@testable import Akari

// Pictures, not assertions: with TEST_RUNNER_AKARI_RENDERS=1 the emoji picker is drawn at 2x in
// both themes into the test host's temporary folder (the RENDER lines name the files).
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct EmojiPickerRenders {
    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("emoji-picker-renders")
    let files = TestImages()

    // A square of its own color for each custom emoji and server icon.
    func cache() throws -> ImageCache {
        var paths: [String: String] = [:]
        let colors: [(String, [Int])] = [
            ("7", [240, 160, 40, 255]), ("8", [60, 190, 120, 255]), ("9", [150, 150, 160, 255]),
            ("20", [230, 90, 170, 255]), ("21", [70, 130, 240, 255]), ("g1", [120, 90, 220, 255]),
        ]
        for (name, color) in colors {
            paths[name] = try files.png(name, pixels: 96, color: color).path
        }
        let served = paths
        return ImageCache(
            source: FakeImageSource { request in
                let name: String =
                    switch request.key {
                    case .emoji(let id, _): "\(id.rawValue)"
                    case .guildIcon: "g1"
                    default: "9"
                    }
                return ImageFile(
                    path: served[name] ?? served["9"]!, width: 96, height: 96, animated: false,
                    fromDisk: false)
            })
    }

    func save(_ view: NSView, _ name: String, theme: String, note: String = "") throws {
        view.window?.layoutIfNeeded()
        view.window?.displayIfNeeded()
        let rep = try AttachmentDrawingTests.layerImage(of: view, scale: 2)
        let data = try #require(rep.representation(using: .png, properties: [:]))
        try FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        let file = Self.folder.appendingPathComponent("\(name)-\(theme).png")
        try data.write(to: file)
        print("RENDER \(file.path) \(note)")
    }

    func picker(_ appearance: NSAppearance.Name, found: [String: [PickerEmoji]] = [:]) throws -> PickerHarness {
        let harness = try PickerHarness(found: found, icons: [GuildId(rawValue: 1): .guildIcon(guildId: GuildId(rawValue: 1), hash: "abc")])
        let window = ScaledWindow.make(scale: 2)
        window.appearance = NSAppearance(named: appearance)
        window.setContentSize(EmojiPickerView.size)
        window.contentView = harness.view
        harness.view.images = try cache()
        window.layoutIfNeeded()
        harness.view.layoutSubtreeIfNeeded()
        harness.extra = window
        return harness
    }

    func loaded(_ harness: PickerHarness) async throws {
        try await eventually {
            harness.grid.cells.values.allSatisfy { $0.picture.isHidden || $0.picture.imageLayer.contents != nil }
                && harness.view.strip.buttons.allSatisfy { $0.icon == nil || $0.picture.imageLayer.contents != nil }
        }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func browsing(_ appearance: NSAppearance.Name) async throws {
        let theme = appearance == .darkAqua ? "dark" : "light"
        let harness = try picker(appearance)
        harness.grid.pointerMoved(to: harness.middle(of: harness.at(1, 0)))
        try await loaded(harness)
        try save(harness.view, "1-browsing", theme: theme, note: "wave selected by the pointer")

        harness.grid.pointerMoved(to: harness.middle(of: harness.at(2, 0)))
        try await eventually { harness.view.footerView.picture.imageLayer.contents != nil }
        try save(harness.view, "2-locked-selected", theme: theme, note: "blob: Nitro")

        harness.grid.scroll(to: harness.grid.layout.sections[4].top - 14)
        harness.grid.pointerMoved(to: harness.middle(of: harness.at(3, 100)))
        try await loaded(harness)
        try save(harness.view, "3-sticky-pushed", theme: theme, note: "Smileys pushed up by People")

        harness.view.strip.buttons[harness.view.strip.buttons.count - 1].performClick(nil)
        try save(harness.view, "4-flags", theme: theme, note: "the strip's Flags")
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func searching(_ appearance: NSAppearance.Name) async throws {
        let theme = appearance == .darkAqua ? "dark" : "light"
        let ok = [pickerEmoji("👌"), pickerEmoji("🆗"), .custom(emoji: pickerCustom(21, "ok", in: 2))]
        let harness = try picker(appearance, found: ["ok": ok])
        harness.type("ok")
        try await eventually { harness.model.sections.first?.kind == .results }
        try await loaded(harness)
        try save(harness.view, "5-search", theme: theme)

        harness.type("zzz")
        try await eventually { harness.model.foundNothing }
        try save(harness.view, "6-nothing-found", theme: theme)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func inTheList(_ appearance: NSAppearance.Name) async throws {
        let theme = appearance == .darkAqua ? "dark" : "light"
        let account = ReactionsAccount([], people: [:])
        let list = BarList(scale: 2, height: 300)
        list.window.appearance = NSAppearance(named: appearance)
        list.actions.picking = ReactionsModel(store: account.reactionsStore, account: account)
        list.controller.emojiPicker.windows.focus = WindowFocus(makeKey: { _ in })
        list.actions.bars[2] = ([.addReaction, .createThread, .more], [.copyLink, .more])
        list.actions.quick[2] = ["💯", "😆", "💖"].map { QuickReaction(emoji: .unicode($0), mine: false) }
        list.show([
            row(1, by: author(2, "Ray"), at: list.noon, content: "first"),
            row(2, by: author(2, "Ray"), at: list.noon + 10, content: "the picker's message")
                .reacting(reactions(pill("👍", 2))),
            row(3, by: author(3, "Mira"), at: list.noon + 600, content: "below"),
        ])
        try list.hover(2)
        try #require(list.bar.button(.addReaction)).performClick(nil)
        try list.hover(3)
        try save(list, "7-row-while-open", theme: theme, note: "row 2 lit with its bar, Add Reaction selected, pointer over row 3")
        list.controller.emojiPicker.close()

        try #require((try list.cell(of: 2) as? MessageCell)?.reactions?.add).performClick(nil)
        try list.hover(3)
        try save(list, "8-add-button-open", theme: theme, note: "row 2's add button lit, its bar's Add Reaction not")
        list.controller.emojiPicker.close()
    }

    func save(_ list: BarList, _ name: String, theme: String, note: String) throws {
        list.window.layoutIfNeeded()
        list.window.displayIfNeeded()
        let view = try #require(list.window.contentView)
        let chat = Palette.chat
        var backdrop = NSColor.black
        view.effectiveAppearance.performAsCurrentDrawingAppearance {
            backdrop = chat.usingColorSpace(.sRGB) ?? chat
        }
        let rep = try AttachmentDrawingTests.layerImage(of: view, scale: 2, over: backdrop)
        let data = try #require(rep.representation(using: .png, properties: [:]))
        try FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        let file = Self.folder.appendingPathComponent("\(name)-\(theme).png")
        try data.write(to: file)
        print("RENDER \(file.path) \(note)")
    }
}
