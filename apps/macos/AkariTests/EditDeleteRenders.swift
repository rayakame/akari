import AkariKit
import AppKit
import Testing

@testable import Akari

// Pictures to look at, not assertions, so they run only when asked (TEST_RUNNER_AKARI_RENDERS=1).
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct EditDeleteRenders {
    nonisolated final class ActionsStore: Store, @unchecked Sendable {
        init() {
            super.init(noHandle: NoHandle())
        }

        required init(unsafeFromHandle handle: UInt64) {
            fatalError("a fake has no Rust object")
        }

        override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
            let own = messageId.rawValue < 10
            return testMessageActions(createThread: true, reply: true, edit: own, delete: own)
        }

        override func channel(id: ChannelId) -> Channel? { nil }

        override func reactions(channelId: ChannelId, messageId: MessageId) -> [Reaction] { [] }

        override func displaySettings() -> DisplaySettings {
            DisplaySettings(
                renderSpoilers: .onClick, viewImageDescriptions: false, inlineAttachmentMedia: true,
                inlineEmbedMedia: true, gifAutoPlay: true, renderEmbeds: true, renderReactions: true,
                animateEmoji: true, animateStickers: .always, explicitContentFilter: .nonFriends,
                explicitContent: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
                gore: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset),
                selfHarm: .init(guilds: .unset, friendDm: .unset, nonFriendDm: .unset))
        }
    }

    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pr-f-renders")

    func write(_ view: NSView, _ name: String, theme: String, notes: [String] = []) throws {
        view.window?.layoutIfNeeded()
        view.window?.displayIfNeeded()
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
        print("RENDER \(file.path) \(notes.joined(separator: "; "))")
    }

    func box(_ view: NSView, in root: NSView) -> String {
        box(view.bounds, of: view, in: root)
    }

    func box(_ rect: NSRect, of view: NSView, in root: NSView) -> String {
        let rect = view.convert(rect, to: root)
        let top = root.isFlipped ? rect.minY : root.bounds.height - rect.maxY
        return "x \(rect.minX * 2) y \(top * 2) w \(rect.width * 2) h \(rect.height * 2)"
    }

    func list(_ name: NSAppearance.Name, height: CGFloat = 520) -> BarList {
        let list = BarList(scale: 2, height: height)
        list.window.appearance = NSAppearance(named: name)
        list.actions.model = MessageActionsModel(
            store: ActionsStore(), account: GuildAccount(), copy: { _ in }, retry: { _ in },
            discard: { _ in }, createThread: { _, _ in }, viewThread: { _, _ in }, failed: { _ in })
        return list
    }

    func changed(_ row: MessageListModel.Row, _ change: (inout Message) -> Void) -> MessageListModel.Row {
        var message = row.message
        change(&message)
        return MessageListModel.Row(id: row.id, message: message, reactions: row.reactions)
    }

    func rows(_ list: BarList) -> [MessageListModel.Row] {
        let me = author(1, "Chris")
        let ray = author(12, "Ray")
        return [
            row(11, by: ray, at: list.noon, content: "Did you see the release notes?"),
            row(1, by: me, at: list.noon + 600, content: "I did, they look good."),
            row(2, by: me, at: list.noon + 620, content: "One typo in the second paragraph though."),
            row(13, by: ray, at: list.noon + 1200, content: "Fixed, thanks!"),
        ]
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func editor(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = list(name)
        list.show(rows(list), bars: false)
        let view = try #require(list.window.contentView)
        let editor = list.controller.editor

        func edit(_ raw: UInt64, _ text: String, problem: EditProblem? = nil, opening: Int) {
            list.actions.editing = text
            list.editing = EditModel.Editing(messageId: MessageId(rawValue: raw), problem: problem, opening: opening)
            list.show(rows(list), bars: false)
        }
        func notes() throws -> [String] {
            [
                "row \(box(try list.cell(of: 2), in: view))",
                "editor \(box(editor, in: view))",
                "box \(box(editor.textView, in: view))",
                "hint \(box(editor.hint, in: view))", "cancel \(box(editor.hint.cancel, in: view))",
                "save \(box(editor.hint.save, in: view))",
                "caret line \(box(editor.caretRect, of: editor, in: view))",
            ]
        }

        edit(2, "One typo in the second paragraph though.", opening: 1)
        try write(view, "1-editor-follow-up", theme: theme, notes: try notes())

        edit(1, "I did, they look good.", opening: 2)
        try write(
            view, "2-editor-group-start", theme: theme,
            notes: ["header row \(box(try list.cell(of: 1), in: view))", "box \(box(editor.textView, in: view))"])

        edit(2, "First line\nSecond line\nThird line", opening: 3)
        try write(view, "3-editor-three-lines", theme: theme, notes: try notes())

        list.actions.left = -12
        edit(2, String(repeating: "Far too long. ", count: 8), problem: .tooLong(by: 12, limit: 2000), opening: 4)
        try write(
            view, "4-editor-counter-and-reason", theme: theme,
            notes: try notes() + [
                "counter \(box(editor.counter, in: view))", "reason \(box(editor.problem, in: view))",
            ])
        list.actions.left = nil

        edit(2, "One typo in the second paragraph though.", opening: 5)
        let save = editor.hint.save
        list.move(to: save.convert(NSPoint(x: save.bounds.midX, y: save.bounds.midY), to: list.table))
        try write(view, "5-editor-save-hovered", theme: theme)

        let room = editor.textView.frame.width - 2 * ComposerTextView.insets.width
        var line = "Typed right up to the edge"
        while NSAttributedString(string: line + " x", attributes: ComposerTextView.attributes).size().width < room - 12 {
            line += " x"
        }
        edit(2, line, opening: 6)
        list.actions.left = 150
        editor.textView.insertText("!", replacementRange: editor.textView.selectedRange())
        list.window.layoutIfNeeded()
        try write(
            view, "11-editor-counter-wraps-as-typed", theme: theme,
            notes: try notes() + ["counter \(box(editor.counter, in: view))"])
        list.actions.left = nil
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func marksAndDims(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = list(name, height: 560)
        let me = author(1, "Chris")
        let thumbs = ReactionRow(
            pills: [ReactionPill(emoji: .unicode("👍"), normal: 2, burst: 0, mine: false)], adds: true,
            toggles: true)
        let rows = [
            row(1, by: me, at: list.noon, content: "An edited message.", edited: list.noon + 60),
            row(2, by: me, at: list.noon + 10, content: "```\nlet code = \"edited\"\n```", edited: list.noon + 70),
            MessageListModel.Row(
                id: MessageId(rawValue: 3),
                message: row(3, by: me, at: list.noon + 20, content: "", stickers: ["wave"], edited: list.noon + 80).message,
                reactions: thumbs),
            changed(row(4, by: me, at: list.noon + 30, content: "A saved edit Discord hasn't taken yet.")) {
                $0.edit = .pending
            },
            changed(row(5, by: me, at: list.noon + 40, content: "A message being deleted.")) {
                $0.deleting = true
            },
            row(6, by: me, at: list.noon + 50, content: "A plain message."),
        ]
        list.show(rows, bars: false)
        let view = try #require(list.window.contentView)
        var notes: [String] = []
        for raw: UInt64 in [1, 2] {
            let cell = try #require(try list.cell(of: raw) as? MessageCell)
            let text = try #require(texts(in: cell).first { $0.string.contains("(edited)") })
            let range = (text.string as NSString).range(of: "(edited)")
            if let layout = text.layoutManager, let container = text.textContainer {
                var rect = layout.boundingRect(
                    forGlyphRange: layout.glyphRange(forCharacterRange: range, actualCharacterRange: nil),
                    in: container)
                rect.origin.x += text.textContainerOrigin.x
                rect.origin.y += text.textContainerOrigin.y
                notes.append("mark \(raw) \(box(rect, of: text, in: view))")
            }
        }
        try write(view, "6-edited-marks-and-dims", theme: theme, notes: notes)

        list.shift = true
        try list.hover(6)
        try write(view, "7-shift-bar-with-delete", theme: theme, notes: ["bar \(box(list.bar, in: view))"])
        list.shift = false
        try list.hover(6)
        try write(view, "8-bar-with-edit", theme: theme, notes: ["bar \(box(list.bar, in: view))"])
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func question(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let me = author(1, "Chris")
        let message = row(1, by: me, at: berlinTime(2026, 10, 10, 12, 0), content: "One typo in the second paragraph though.")
        for (file, tip) in [("9-delete-question-from-the-menu", true), ("10-delete-question", false)] {
            let question = MessageActionsModel.DeleteQuestion(row: message, showsTip: tip) {}
            let view = DeleteMessageView()
            let height = view.height(for: question, width: DeleteLayout.width, maxHeight: 800)
            let window = ScaledWindow.make(scale: 2)
            window.appearance = NSAppearance(named: name)
            window.setContentSize(NSSize(width: DeleteLayout.width + 40, height: height + 40))
            let backdrop = FlippedView()
            window.contentView = backdrop
            view.frame = NSRect(x: 20, y: 20, width: DeleteLayout.width, height: height)
            backdrop.addSubview(view)
            view.show(question, images: nil, maxHeight: 800)
            window.layoutIfNeeded()
            try write(
                backdrop, file, theme: theme,
                notes: [
                    "panel \(box(view, in: backdrop))", "title \(box(view.title, in: backdrop))",
                    "message \(box(view.message, in: backdrop))", "box \(box(view.box, in: backdrop))",
                    "row \(box(view.box.preview, in: backdrop))", "tip \(box(view.tip, in: backdrop))",
                    "cancel \(box(view.cancel, in: backdrop))", "delete \(box(view.delete, in: backdrop))",
                ])
        }
    }

    func texts(in view: NSView) -> [MessageTextView] {
        view.subviews.flatMap { ($0 as? MessageTextView).map { [$0] } ?? texts(in: $0) }
    }
}
