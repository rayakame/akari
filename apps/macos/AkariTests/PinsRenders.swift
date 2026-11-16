import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// Pictures to look at, not assertions: they run only with TEST_RUNNER_AKARI_RENDERS=1.
@MainActor @Suite(.enabled(if: ProcessInfo.processInfo.environment["AKARI_RENDERS"] != nil))
struct PinsRenders {
    static let folder = FileManager.default.temporaryDirectory.appendingPathComponent("pr-g-renders")
    static let margin: CGFloat = 20

    func save(_ rep: NSBitmapImageRep, _ name: String, theme: String, notes: [String] = []) throws {
        let data = try #require(rep.representation(using: .png, properties: [:]))
        try FileManager.default.createDirectory(at: Self.folder, withIntermediateDirectories: true)
        let file = Self.folder.appendingPathComponent("\(name)-\(theme).png")
        try data.write(to: file)
        print("RENDER \(file.path) \(notes.joined(separator: "; "))")
    }

    // SwiftUI drawn by its window at 2x, on the chat's color.
    func drawn(_ view: some View, size: NSSize, theme: NSAppearance.Name) throws -> NSBitmapImageRep {
        let window = ScaledWindow.make(scale: 2)
        window.appearance = NSAppearance(named: theme)
        let host = NSHostingView(
            rootView: view.padding(Self.margin).background(Color(nsColor: Palette.chat)).ignoresSafeArea())
        window.setContentSize(NSSize(width: size.width + 2 * Self.margin, height: size.height + 2 * Self.margin))
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        window.displayIfNeeded()
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        return rep
    }

    func session(_ configure: (FakeBrowserStore) -> Void) async throws -> PinsModel {
        let fake = FakeBrowserStore()
        fake.lines = true
        configure(fake)
        let session = fake.session()
        session.open(channel: FakeBrowserStore.parent)
        session.togglePins()
        let pins = try #require(session.pins)
        await pins.open()
        return pins
    }

    func panel(_ pins: PinsModel, hovered: MessageId? = nil, window: CGFloat = 813) -> (some View, NSSize) {
        let height = PinsLayout.height(
            content: PinsLayout.contentHeight(pins, upTo: window), window: window)
        let size = NSSize(width: PinsLayout.width, height: height)
        return (PinsPanel(pins: pins, size: size, hovered: hovered), size)
    }

    // Where a column's color changes going down, in points from the image's top.
    func edges(_ rep: NSBitmapImageRep, x: CGFloat, from: CGFloat, to: CGFloat, by: CGFloat = 0.06) -> [CGFloat] {
        var found: [CGFloat] = []
        var previous: NSColor?
        for row in Int(from * 2)..<Int(to * 2) {
            let color = rep.colorAt(x: Int(x * 2), y: row)?.usingColorSpace(.sRGB)
            if let color, let previous, abs(color.brightnessComponent - previous.brightnessComponent) > by {
                found.append(CGFloat(row) / 2)
            }
            previous = color
        }
        return found
    }

    func columns(_ rep: NSBitmapImageRep, y: CGFloat, from: CGFloat, to: CGFloat, by: CGFloat = 0.06) -> [CGFloat] {
        var found: [CGFloat] = []
        var previous: NSColor?
        for column in Int(from * 2)..<Int(to * 2) {
            let color = rep.colorAt(x: column, y: Int(y * 2))?.usingColorSpace(.sRGB)
            if let color, let previous, abs(color.brightnessComponent - previous.brightnessComponent) > by {
                found.append(CGFloat(column) / 2)
            }
            previous = color
        }
        return found
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func popout(_ name: NSAppearance.Name) async throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let m = Self.margin

        let list = try await session { $0.pins = 3 }
        let (view, size) = panel(list, hovered: list.rows.first?.id)
        let rep = try drawn(view, size: size, theme: name)
        let first = PinsLayout.cardHeight(list.rows[0])
        try save(
            rep, "1-popout-pins", theme: theme,
            notes: [
                "panel \(size.width) x \(size.height)",
                "header line at \(PinsLayout.headerHeight - 1)",
                "cards from y \(PinsLayout.headerHeight + PinsLayout.listTop), x \(PinsLayout.listLeading) w \(PinsLayout.cardWidth)",
                "card heights \(list.rows.map(PinsLayout.cardHeight))",
                "preview \(PinsLayout.previewWidth) wide at x \(PinsLayout.listLeading + 1 - PinsLayout.messageShift)",
            ])
        let cardTop = m + PinsLayout.headerHeight + PinsLayout.listTop
        print(
            "MEASURE \(theme) popout: down the card's left side",
            edges(rep, x: m + PinsLayout.listLeading + 3, from: m, to: cardTop + first + 12).map { $0 - m })
        print(
            "MEASURE \(theme) popout: across the first card's header line",
            columns(rep, y: cardTop + 1 + 16 + 8, from: m, to: m + size.width).map { $0 - m })
        print(
            "MEASURE \(theme) popout: down through the avatar",
            edges(rep, x: m + PinsLayout.listLeading + 1 + 12 + 20, from: cardTop, to: cardTop + first).map { $0 - cardTop })
        print(
            "MEASURE \(theme) popout: across the buttons' middle",
            columns(rep, y: cardTop + 1 + 8 + 16, from: m + 200, to: m + size.width).map { $0 - m })
        print(
            "MEASURE \(theme) popout: down the buttons",
            edges(rep, x: m + size.width - PinsLayout.cardTrailing - 1 - 8 - 16, from: cardTop, to: cardTop + 60).map { $0 - cardTop })

        let more = try await session {
            $0.pins = 3
            $0.pinsMore = true
        }
        let (moreView, moreSize) = panel(more)
        try save(try drawn(moreView, size: moreSize, theme: name), "2-popout-load-more", theme: theme)

        let empty = try await session { $0.pins = 0 }
        let (emptyView, emptySize) = panel(empty)
        let emptyRep = try drawn(emptyView, size: emptySize, theme: name)
        try save(
            emptyRep, "3-popout-empty", theme: theme,
            notes: ["panel \(emptySize.width) x \(emptySize.height)", "footer \(PinsLayout.footerHeight(empty)) high"])
        print(
            "MEASURE \(theme) empty: down the middle",
            edges(rep: emptyRep, x: m + emptySize.width / 2, height: emptySize.height))

        let failed = try await session {
            $0.pins = 0
            $0.pinsState = .failed
        }
        let (failedView, failedSize) = panel(failed)
        try save(try drawn(failedView, size: failedSize, theme: name), "4-popout-failed", theme: theme)
    }

    func edges(rep: NSBitmapImageRep, x: CGFloat, height: CGFloat) -> [CGFloat] {
        edges(rep, x: x, from: Self.margin, to: Self.margin + height).map { $0 - Self.margin }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func questions(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let message = row(
            1, by: author(2, "Ray"), at: berlinTime(2026, 10, 10, 12, 0),
            content: "The release is on Friday. Please test the build before then!")
        for (file, pins) in [("5-pin-question", true), ("6-unpin-question", false)] {
            let question = MessageActionsModel.PinQuestion(row: message, pins: pins, channelName: "new") {}
            let view = MessageQuestionView()
            let width = MessageQuestionLayout.width
            let height = view.height(for: question, width: width, maxHeight: 800)
            let window = ScaledWindow.make(scale: 2)
            window.appearance = NSAppearance(named: name)
            window.setContentSize(NSSize(width: width + 40, height: height + 40))
            let backdrop = FlippedView()
            window.contentView = backdrop
            view.frame = NSRect(x: 20, y: 20, width: width, height: height)
            backdrop.addSubview(view)
            view.show(question, images: nil, maxHeight: 800)
            window.layoutIfNeeded()
            window.displayIfNeeded()
            let chat = Palette.chat
            var color = NSColor.black
            backdrop.effectiveAppearance.performAsCurrentDrawingAppearance {
                color = chat.usingColorSpace(.sRGB) ?? chat
            }
            let rep = try AttachmentDrawingTests.layerImage(of: backdrop, scale: 2, over: color)
            try save(
                rep, file, theme: theme,
                notes: [
                    "panel \(width) x \(height)", "title \(view.title.frame)", "message \(view.message.frame)",
                    "box \(view.box.frame)", "tip \(view.tip.isHidden ? "none" : "\(view.tip.frame)")",
                    "cancel \(view.cancel.frame)", "confirm \(view.confirm.frame)",
                ])
        }
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func button(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let states: [(String, Bool, Bool, Bool)] = [
            ("rest", false, false, false), ("unread", false, true, false), ("open", true, false, false),
            ("disabled", false, false, true),
        ]
        let row = HStack(spacing: 8) {
            ForEach(states, id: \.0) { state in
                PinsButton(selected: state.1, unread: state.2, disabled: state.3) {}
            }
        }
        .frame(width: 4 * 32 + 3 * 8, height: 48)
        let rep = try drawn(row, size: NSSize(width: 4 * 32 + 3 * 8, height: 48), theme: name)
        try save(rep, "7-header-button", theme: theme, notes: ["rest, unread, open, disabled; 32 boxes 8 apart"])
        print(
            "MEASURE \(theme) button: across the unread dot's middle",
            columns(rep, y: Self.margin + 8 + 16 + 3 + 1, from: Self.margin + 40, to: Self.margin + 72)
                .map { $0 - Self.margin - 40 })
        print(
            "MEASURE \(theme) button: down the unread dot's middle",
            edges(rep, x: Self.margin + 40 + 16 + 10 + 3 - 6 + 4, from: Self.margin, to: Self.margin + 48)
                .map { $0 - Self.margin })
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func bar(_ name: NSAppearance.Name) throws {
        let theme = name == .darkAqua ? "dark" : "light"
        let list = BarList(scale: 2, height: 200)
        list.window.appearance = NSAppearance(named: name)
        list.actions.model = MessageActionsModel(
            store: PinActionsInTheListTests.PinStore(), account: GuildAccount(), copy: { _ in },
            retry: { _ in }, discard: { _ in }, createThread: { _, _ in }, viewThread: { _, _ in },
            failed: { _ in })
        list.show(
            [
                row(1, by: author(2, "Ray"), at: list.noon, content: "Not pinned yet."),
                row(2, by: author(2, "Ray"), at: list.noon + 600, content: "Pinned already."),
            ], bars: false)
        list.shift = true
        let view = try #require(list.window.contentView)
        for (raw, file) in [(UInt64(1), "8-shift-bar-pin"), (2, "9-shift-bar-unpin")] {
            try list.hover(raw)
            list.window.layoutIfNeeded()
            list.window.displayIfNeeded()
            let chat = Palette.chat
            var color = NSColor.black
            view.effectiveAppearance.performAsCurrentDrawingAppearance {
                color = chat.usingColorSpace(.sRGB) ?? chat
            }
            let rep = try AttachmentDrawingTests.layerImage(of: view, scale: 2, over: color)
            try save(rep, file, theme: theme, notes: ["bar \(list.bar.convert(list.bar.bounds, to: view))"])
        }
    }
}
