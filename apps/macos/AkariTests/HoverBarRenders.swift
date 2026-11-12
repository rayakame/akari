import AkariKit
import AppKit
import Testing

@testable import Akari

// Pictures, not assertions: with TEST_RUNNER_AKARI_RENDERS=1 the hover bar is drawn at 2x in both
// themes, with the model's own words and symbols, into the test host's temporary folder (the
// RENDER line names it).
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct HoverBarRenders {
    nonisolated final class ActionsStore: Store, @unchecked Sendable {
        init() {
            super.init(noHandle: NoHandle())
        }

        required init(unsafeFromHandle handle: UInt64) {
            fatalError("a fake has no Rust object")
        }

        override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
            switch messageId.rawValue {
            case 6:
                var failed = testMessageActions()
                failed.markUnread = false
                failed.copyLink = false
                failed.delete = true
                return failed
            case 3: return testMessageActions(viewThread: true)
            default: return testMessageActions(createThread: true)
            }
        }

        override func channel(id: ChannelId) -> Channel? { nil }
    }

    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("hover-bar-renders")

    func write(_ list: BarList, _ name: String, theme: String) throws {
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
        let box = list.bar.convert(list.bar.bounds, to: view)
        let top = view.isFlipped ? box.minY : view.bounds.height - box.maxY
        print("RENDER \(file.path) bar x \(box.minX * 2) y \(top * 2) w \(box.width * 2) h \(box.height * 2)")
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func bars(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = BarList(scale: 2, height: 520)
        list.window.appearance = NSAppearance(named: name)
        list.actions.model = MessageActionsModel(
            store: ActionsStore(), account: GuildAccount(),
            copy: { _ in }, retry: { _ in }, discard: { _ in }, createThread: { _, _ in },
            viewThread: { _, _ in }, failed: { _ in })
        let me = author(1, "Chris")
        list.show(
            [
                row(1, by: author(2, "Ray"), at: list.noon, content: "The first message of a group, from someone else."),
                row(2, by: author(2, "Ray"), at: list.noon + 10, content: "A follow-up in the same group."),
                row(3, by: me, at: list.noon + 600, content: "My own message, which has a thread."),
                row(4, by: me, at: list.noon + 610, content: "My own follow-up."),
                row(5, by: author(4, "Kim"), at: list.noon + 700, kind: .userJoin),
                row(6, by: me, at: list.noon + 800, content: "This one was not sent.", delivery: .failed),
            ], bars: false)

        try list.hover(2)
        try write(list, "1-others-message", theme: theme)
        try list.hover(1)
        try write(list, "2-others-group-start", theme: theme)
        try list.hover(4)
        try write(list, "3-own-message", theme: theme)
        try list.hover(3)
        try write(list, "4-own-group-start-with-thread", theme: theme)
        list.shift = true
        try list.hover(2)
        try write(list, "5-shift-held", theme: theme)
        list.shift = false
        try list.hover(2)
        let more = try #require(list.bar.button(.more))
        list.move(to: list.table.convert(NSPoint(x: 14, y: 14), from: more))
        try write(list, "6-button-hovered", theme: theme)
        more.isSelected = true
        try write(list, "7-more-selected-menu-open", theme: theme)
        more.isSelected = false
        try list.hover(5)
        try write(list, "8-system-line", theme: theme)
        try list.hover(6)
        try write(list, "9-failed-send", theme: theme)
    }
}
