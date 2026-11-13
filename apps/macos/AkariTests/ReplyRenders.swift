import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Pictures, not assertions: with TEST_RUNNER_AKARI_RENDERS=1 replies are drawn at 2x in both
// themes into the test host's temporary folder (the RENDER lines name the files).
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct ReplyRenders {
    nonisolated final class ActionsStore: Store, @unchecked Sendable {
        init() {
            super.init(noHandle: NoHandle())
        }

        required init(unsafeFromHandle handle: UInt64) {
            fatalError("a fake has no Rust object")
        }

        override func messageActions(channelId: ChannelId, messageId: MessageId) -> MessageActions {
            testMessageActions(createThread: true, reply: true, edit: messageId.rawValue == 9)
        }

        override func channel(id: ChannelId) -> Channel? { nil }
    }

    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pr-c-renders")

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

    // A view's box in 2x pixels from the picture's top left.
    func box(_ view: NSView, in root: NSView) -> String {
        let rect = view.convert(view.bounds, to: root)
        let top = root.isFlipped ? rect.minY : root.bounds.height - rect.maxY
        return "x \(rect.minX * 2) y \(top * 2) w \(rect.width * 2) h \(rect.height * 2)"
    }

    func rows(_ list: BarList) -> [MessageListModel.Row] {
        let ray = author(2, "Ray")
        let kim = author(4, "Kim")
        let long = String(repeating: "a long original message that does not fit on one line ", count: 4)
        return [
            row(1, by: ray, at: list.noon, content: "The original message."),
            replyRow(
                2, by: kim, at: list.noon + 600, content: "A reply that pings.", to: 1,
                target(by: ray, "The original message."), mentions: true),
            replyRow(
                3, by: ray, at: list.noon + 1200, content: "A reply to a long message with a picture.",
                to: 2, target(by: kim, long, attachments: true)),
            replyRow(4, by: kim, at: list.noon + 1800, content: "Its message is gone.", to: 90, .deleted),
            replyRow(5, by: ray, at: list.noon + 2400, content: "Its message is not loaded.", to: 91, .notLoaded),
            replyRow(
                6, by: kim, at: list.noon + 3000, content: "To a picture.", to: 92,
                target(by: ray, "", attachments: true)),
            replyRow(
                7, by: ray, at: list.noon + 3600, content: "To a sticker.", to: 93,
                target(by: kim, "", sticker: true)),
            replyRow(
                8, by: kim, at: list.noon + 4200, content: "A reply that mentions me.", to: 9,
                target(by: author(1, "Chris"), "What I wrote, **bold** and `code`.", edited: true),
                mentions: true, mentionsMe: true),
            row(9, by: author(1, "Chris"), at: list.noon + 4800, content: "My own message."),
        ]
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func lines(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = BarList(scale: 2, height: 760)
        list.window.appearance = NSAppearance(named: name)
        list.actions.model = MessageActionsModel(
            store: ActionsStore(), account: GuildAccount(),
            copy: { _ in }, retry: { _ in }, discard: { _ in }, createThread: { _, _ in },
            viewThread: { _, _ in }, failed: { _ in })
        list.show(rows(list), bars: false)
        let view = try #require(list.window.contentView)
        let cell = try #require(try list.cell(of: 2) as? MessageCell)
        try write(
            view, "1-lines", theme: theme,
            notes: [
                "row 2 line \(box(cell.replyLine, in: view))", "spine \(box(cell.spine, in: view))",
                "mini avatar \(box(cell.replyLine.avatar, in: view))",
                "content \(box(cell.replyLine.content, in: view))",
            ])

        list.move(to: {
            let rect = cell.replyLine.content.convert(cell.replyLine.content.bounds, to: list.table)
            return NSPoint(x: rect.midX, y: rect.midY)
        }())
        try write(view, "2-line-hovered", theme: theme)

        try list.hover(1)
        try write(
            view, "3-hover-bar-with-reply", theme: theme, notes: ["bar \(box(list.bar, in: view))"])
        list.shift = true
        try list.hover(9)
        try write(view, "4-hover-bar-own-message-shift", theme: theme)
        list.shift = false
        list.move(to: NSPoint(x: -50, y: -50))

        list.controller.show(
            MessageTableState(rows: rows(list), reachedOldest: true, replyingTo: MessageId(rawValue: 1)))
        try write(view, "5-answered-row-tint", theme: theme)
        try list.hover(1)
        try write(view, "6-answered-row-tint-hovered", theme: theme)
        list.move(to: NSPoint(x: -50, y: -50))
        list.controller.show(MessageTableState(rows: rows(list), reachedOldest: true))

        // The flash as it holds: both layers full (a render shows the layers' own values).
        let flashed = try #require(try list.cell(of: 3) as? MessageCell)
        flashed.flash(elapsed: 1, reducesMotion: true)
        flashed.flashLayer.opacity = 1
        flashed.flashOverlay.opacity = 1
        try write(view, "7-flash-held", theme: theme)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func bars(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        for (file, mention, toggle) in [
            ("8-reply-bar-on", true, true), ("9-reply-bar-off", false, true),
            ("10-reply-bar-no-switch", true, false),
        ] {
            let account = FormattedAccount(contents: ["test"], thread: true)
            account.formattedStore.answerable.withLock { $0 = toggle }
            let session = SessionScreenTests().formattedSession(account)
            let composer = try #require(session.messages).composer
            if toggle {
                composer.startReply(to: try #require(session.messages?.rows.last), shift: !mention)
            }
            let window = ScaledWindow.make(scale: 2)
            window.appearance = NSAppearance(named: name)
            window.setContentSize(NSSize(width: 600, height: 120))
            let content =
                toggle
                ? AnyView(ComposerView(composer: composer, placeholder: "Message #general", areaHeight: 400))
                : AnyView(
                    // A DM's band alone: no switch.
                    ReplyBar(
                        reply: PendingReply(
                            messageId: MessageId(rawValue: 1), authorName: "Ray", mention: true,
                            showsToggle: false),
                        jump: {}, toggle: {}, cancel: {}
                    )
                    .frame(height: ReplyBarView.height(showsToggle: false))
                    .padding(.horizontal, 8))
            let host = NSHostingView(
                rootView: content.frame(maxHeight: .infinity, alignment: .top)
                    .background(Color(nsColor: Palette.chat)))
            window.contentView = host
            host.layoutSubtreeIfNeeded()
            let bar = try #require(SessionScreenTests().views(ReplyBarView.self, in: host).first)
            bar.layoutSubtreeIfNeeded()
            try write(
                host, file, theme: theme,
                notes: [
                    "band \(box(bar, in: host))", "switch \(box(bar.toggle, in: host))",
                    "separator \(box(bar.separator, in: host))", "close \(box(bar.close, in: host))",
                ])
        }
    }
}
