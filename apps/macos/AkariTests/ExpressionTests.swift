import AkariKit
import AppKit
import Testing

@testable import Akari

// Hovering a custom emoji, a Unicode emoji or a sticker in a message shows the pointing hand
// (the list's one cursor authority) and, after 300 ms, a hover card; a click opens its popout.
@MainActor @Suite(.serialized, .timeLimit(.minutes(1)))
final class ExpressionTests {
    let list = MessageTableTests()
    let actions = MessageTableTests.RecordingActions()
    let times = TimestampText(
        locale: Locale(identifier: "en_US"), timeZone: TimeZone(identifier: "Europe/Berlin")!,
        calendar: Calendar(identifier: .gregorian))

    init() {
        list.controller.actions = actions
        list.controller.hoverCard.delay = .milliseconds(20)
    }

    func show(_ content: String, stickers: [String] = []) {
        list.show(
            MessageTableState(
                rows: [row(1, at: list.noon, content: content, stickers: stickers)], reachedOldest: true,
                images: ImageCacheRef(ImageCache(source: FakeImageSource.stalling())), focused: true))
    }

    func text() throws -> MessageTextView {
        try #require(list.messageTexts(in: try list.cell(of: 1)).first { !$0.string.isEmpty })
    }

    // In window coordinates: the middle of the character at `index` of the message's text.
    func point(at index: Int, in view: MessageTextView) throws -> NSPoint {
        let layout = try #require(view.layoutManager)
        let glyphs = layout.glyphRange(forCharacterRange: NSRange(location: index, length: 1), actualCharacterRange: nil)
        let rect = layout.boundingRect(forGlyphRange: glyphs, in: try #require(view.textContainer))
            .offsetBy(dx: view.textContainerOrigin.x, dy: view.textContainerOrigin.y)
        return view.convert(NSPoint(x: rect.midX, y: rect.midY), to: nil)
    }

    func index(of shown: String, in view: MessageTextView) -> Int {
        (view.string as NSString).range(of: shown).location
    }

    func standalone(_ content: String) -> MessageTextView {
        let view = MessageTextView()
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 600, height: 200), styleMask: [.titled],
            backing: .buffered, defer: false)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 600, height: 200))
        window.contentView = container
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.widthAnchor.constraint(equalToConstant: 400),
        ])
        let message = row(1, at: list.noon, content: content).message
        view.detectsExpressions = true
        view.show(MessageText.build(message, revealed: [], now: list.noon, times: times).text)
        container.layoutSubtreeIfNeeded()
        retained.append(window)
        return view
    }

    var retained: [NSWindow] = []

    // A click on an emoji opens it at either backing scale (CI's displays are 1x).
    @Test(arguments: [1, 2] as [CGFloat])
    func aClickOnAnEmojiInTextOpensIt(_ scale: CGFloat) throws {
        let window = ScaledWindow(scale: scale)
        retained.append(window)
        let container = NSView(frame: NSRect(x: 0, y: 0, width: 300, height: 200))
        window.contentView = container
        let view = MessageTextView()
        view.detectsExpressions = true
        container.addSubview(view)
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: container.leadingAnchor),
            view.topAnchor.constraint(equalTo: container.topAnchor),
            view.widthAnchor.constraint(equalToConstant: 280),
        ])
        view.show(MessageText.build(row(1, at: list.noon, content: "hi <:wave:5> there").message, revealed: [], now: list.noon, times: times).text)
        container.layoutSubtreeIfNeeded()
        var opened: [ExpressionTarget] = []
        view.onExpression = { target, _ in opened.append(target) }
        // What CI leaves behind: another suite's drag and release nobody consumed, older than the press.
        let stale = try point(at: 3, in: view)
        for type in [NSEvent.EventType.leftMouseDragged, .leftMouseUp] {
            NSApp.postEvent(
                try #require(
                    NSEvent.mouseEvent(
                        with: type, location: NSPoint(x: stale.x + 60, y: stale.y), modifierFlags: [],
                        timestamp: 0, windowNumber: window.windowNumber, context: nil, eventNumber: 0,
                        clickCount: 1, pressure: 1)), atStart: false)
        }

        try click(at: try point(at: 3, in: view), in: view, draining: false)

        #expect(opened == [.customEmoji(id: EmojiId(rawValue: 5), name: "wave", animated: false)], "\(scale)x: selection \(view.selectedRange())")
    }

    @Test
    func theTextKnowsItsEmoji() throws {
        // Discord's masked links can't hold emoji, so none is ever inside a link.
        let view = standalone("hi <:wave:5> and 😀 `🎉` ||🍕|| ™")
        let local = { (index: Int) in view.convert(try self.point(at: index, in: view), from: nil) }

        let custom = try #require(view.expression(at: try local(3)))
        #expect(custom.target == .customEmoji(id: EmojiId(rawValue: 5), name: "wave", animated: false))
        #expect(custom.rect.width >= 22)
        let grinning = try #require(view.expression(at: try local(index(of: "😀", in: view))))
        #expect(grinning.target == .unicodeEmoji("😀", name: "grinning"))
        #expect(view.cursor(at: try local(3)) == .pointingHand)
        #expect(view.cursor(at: try local(index(of: "😀", in: view))) == .pointingHand)

        #expect(view.expression(at: try local(0)) == nil, "text")
        #expect(view.expression(at: try local(index(of: "🎉", in: view))) == nil, "code")
        #expect(view.expression(at: try local(index(of: "🍕", in: view))) == nil, "a hidden spoiler")
        #expect(view.expression(at: try local(index(of: "™", in: view))) == nil, "™ stays text")
    }

    // Only a message's own text offers emoji: a channel's start, a notice or a file's line
    // doesn't open popouts, nor does the New Thread form's preview.
    @Test
    func onlyAMessagesTextKnowsItsEmoji() throws {
        let plain = MessageTextView()
        #expect(!plain.detectsExpressions)
        let view = standalone("hi 😀")
        view.detectsExpressions = false
        #expect(view.expression(at: view.convert(try point(at: 3, in: view), from: nil)) == nil)
        #expect(view.cursor(at: view.convert(try point(at: 3, in: view), from: nil)) != .pointingHand)

        show("hi 😀")
        #expect(try text().detectsExpressions)
        let cell = MessageCell()
        cell.expressionsEnabled = false
        #expect(!(MessageMenuAreaTests.views(MessageTextView.self, in: cell).contains { $0.detectsExpressions }))
    }

    @Test
    func keycapsAreEmoji() throws {
        let view = standalone("1️⃣ and #️⃣ and 1 #")
        let local = { (index: Int) in view.convert(try self.point(at: index, in: view), from: nil) }

        #expect(view.expression(at: try local(0))?.target == .unicodeEmoji("1️⃣", name: "one"))
        #expect(view.expression(at: try local(index(of: "#️⃣", in: view)))?.target == .unicodeEmoji("#️⃣", name: "hash"))
        #expect(view.expression(at: try local((view.string as NSString).length - 1)) == nil, "a plain #")
    }

    // The card's animated emoji plays while the window is key, as in the message.
    @Test
    func theCardPlaysAnAnimatedEmoji() async throws {
        list.show(
            MessageTableState(
                rows: [row(1, at: list.noon, content: "hi <a:dance:6> there")], reachedOldest: true,
                images: ImageCacheRef(ImageCache(source: FakeImageSource.stalling())), focused: true))
        let view = try text()
        list.pointer.location = try point(at: 3, in: view)
        list.controller.pointerMoved()
        try await eventually { self.list.controller.hoverCard.shown != nil }

        let panel = try #require(list.controller.hoverCard.panel)
        let content = try #require(panel.contentView)
        let layer = try #require(MessageMenuAreaTests.views(ImageLayerView.self, in: content).first)
        #expect(layer.animates)

        list.pointer.location = try point(at: 0, in: view)
        list.controller.pointerMoved()
        #expect(panel.contentView == nil, "a hidden card lets go of its image")
    }

    @Test
    func theListShowsTheHandOverEmojiAndStickers() throws {
        show("hi <:wave:5> and 😀", stickers: ["wave"])
        let view = try text()

        #expect(list.controller.cursor(at: try point(at: 3, in: view)) == .pointingHand)
        #expect(list.controller.cursor(at: try point(at: index(of: "😀", in: view), in: view)) == .pointingHand)
        #expect(list.controller.cursor(at: try point(at: 0, in: view)) == .iBeam)
        let sticker = try #require(MessageMenuAreaTests.views(StickerView.self, in: try list.cell(of: 1)).first)
        #expect(list.controller.cursor(at: sticker.convert(NSPoint(x: 80, y: 80), to: nil)) == .pointingHand)
    }

    // After the delay the card shows above the item; moving off hides it at once.
    @Test
    func hoveringShowsTheCardAfterTheDelay() async throws {
        show("hi <:wave:5> and 😀", stickers: ["imposter"])
        let view = try text()

        list.pointer.location = try point(at: 3, in: view)
        list.controller.pointerMoved()
        #expect(list.controller.hoverCard.shown == nil, "not before the delay")
        try await eventually { self.list.controller.hoverCard.shown != nil }
        #expect(list.controller.hoverCard.shown?.title == ":wave:")
        #expect(list.controller.hoverCard.shown?.prompt == "Click to learn more")
        let panel = try #require(list.controller.hoverCard.panel)
        #expect(panel.isVisible)
        #expect(panel.ignoresMouseEvents)
        let emoji = try #require(view.expression(at: view.convert(try point(at: 3, in: view), from: nil)))
        let box = view.window!.convertToScreen(view.convert(emoji.rect, to: nil))
        #expect(panel.frame.minY >= box.maxY, "above the emoji, not over it")

        list.pointer.location = try point(at: 0, in: view)
        list.controller.pointerMoved()
        #expect(list.controller.hoverCard.shown == nil)
        #expect(!panel.isVisible)

        let sticker = try #require(MessageMenuAreaTests.views(StickerView.self, in: try list.cell(of: 1)).first)
        list.pointer.location = sticker.convert(NSPoint(x: 80, y: 80), to: nil)
        list.controller.pointerMoved()
        try await eventually { self.list.controller.hoverCard.shown != nil }
        #expect(list.controller.hoverCard.shown?.title == "imposter")
        #expect(list.controller.hoverCard.shown?.picture == .stickerSymbol)

        list.pointer.location = try point(at: index(of: "😀", in: view), in: view)
        list.controller.pointerMoved()
        try await eventually { self.list.controller.hoverCard.shown?.title == ":grinning:" }
    }

    // A click opens the popout beside the item and hides the card; the server row of a server
    // the user is in opens it; Escape closes the popout.
    @Test
    func aClickOpensThePopout() async throws {
        show("hi <:wave:5> there")
        let view = try text()
        list.pointer.location = try point(at: 3, in: view)
        list.controller.pointerMoved()

        try click(at: try point(at: 3, in: view), in: view)

        #expect(actions.popouts == [.customEmoji(id: EmojiId(rawValue: 5), name: "wave", animated: false)])
        #expect(list.controller.hoverCard.shown == nil)
        let presenter = list.controller.expressionPopout
        // The test window isn't on screen, so neither is its child panel.
        let panel = try #require(presenter.windows.panel)
        try await eventually { presenter.model?.content.loading == false }
        #expect(presenter.model?.content.server?.name == "Emoji Hub")
        let anchor = view.window!.convertToScreen(NSRect(origin: try point(at: 3, in: view), size: .zero))
        #expect(panel.frame.minX > anchor.minX, "to the right of the emoji")
        #expect(panel.frame.width == ExpressionPopoutView.width)

        presenter.openServer(GuildId(rawValue: 50))
        #expect(actions.calls.contains("guild 50"))
        #expect(presenter.windows.panel == nil, "opening the server closes the popout")

        try click(at: try point(at: 3, in: view), in: view)
        let reopened = try #require(presenter.windows.panel)
        let escape = try #require(
            NSEvent.keyEvent(
                with: .keyDown, location: .zero, modifierFlags: [], timestamp: 0,
                windowNumber: reopened.windowNumber, context: nil, characters: "\u{1b}",
                charactersIgnoringModifiers: "\u{1b}", isARepeat: false, keyCode: 53))
        reopened.sendEvent(escape)
        #expect(presenter.windows.panel == nil)
    }

    // "Show More" grows the popout around the item, inside the window.
    @Test
    func showMoreGrowsThePopoutAroundTheItem() async throws {
        let window = list.window
        let presenter = ExpressionPopoutPresenter()
        presenter.windows.focus = WindowFocus(makeKey: { _ in })
        final class Hub: ExpressionLookups {
            func emojiSource(emojiId: EmojiId) async throws -> EmojiSource {
                .guild(guild: SourceGuild(
                    id: GuildId(rawValue: 60), name: "Hub", icon: nil, discoverable: true, verified: false,
                    partnered: false, presenceCount: 10,
                    emojis: (0..<20).map { SourceEmoji(id: EmojiId(rawValue: UInt64(100 + $0)), name: "e", animated: false, key: .emoji(emojiId: EmojiId(rawValue: UInt64(100 + $0)), animated: false)) },
                    stickers: []))
            }
            func sticker(stickerId: StickerId) async throws -> StickerDetails { throw RequestError.UnexpectedResponse }
            func stickerGuild(stickerId: StickerId) async throws -> SourceGuild { throw RequestError.UnexpectedResponse }
            func stickerPack(packId: StickerPackId) async throws -> StickerPack { throw RequestError.UnexpectedResponse }
        }
        struct Nowhere: ExpressionStore {
            func emojiGuild(emojiId: EmojiId) -> GuildId? { nil }
            func guild(id: GuildId) -> Guild? { nil }
            func nitro() -> Nitro { .none }
            func unavailableGuildIds() -> [GuildId] { [] }
        }
        let model = ExpressionPopoutModel(
            target: .customEmoji(id: EmojiId(rawValue: 5), name: "wave", animated: false), channelGuild: nil,
            store: Nowhere(), lookups: Hub())
        let anchor = NSRect(x: 100, y: 200, width: 22, height: 22)
        presenter.show(model, anchor: anchor, in: window, images: nil)
        try await eventually { model.content.loading == false && presenter.windows.panel != nil }
        try await Task.sleep(for: .milliseconds(30))
        let before = try #require(presenter.windows.panel).frame

        presenter.toggleMore()

        let after = try #require(presenter.windows.panel).frame
        #expect(after.height > before.height + 30)
        let item = window.convertToScreen(anchor)
        #expect(abs(after.midY - item.midY) < 1 || after.minY >= window.frame.minY + 8)
        #expect(after.minY >= window.frame.minY + 8 - 1 && after.maxY <= window.frame.maxY - 8 + 1)
        presenter.close()
    }

    // The card sits above the item, flips below at the top of the screen, stays on the item's
    // screen, and its arrow points at the item when the card is pushed in from an edge.
    @Test
    func theCardsPlacement() {
        let screen = NSRect(x: 0, y: 0, width: 1000, height: 800)
        let size = NSSize(width: 200, height: 70)

        let middle = ExpressionHoverCard.placement(
            anchor: NSRect(x: 400, y: 300, width: 22, height: 22), size: size, screen: screen)
        #expect(!middle.below)
        #expect(middle.frame.minY == 322 + ExpressionHoverCard.gap)
        #expect(middle.frame.midX == 411)
        #expect(middle.arrowX == 100)

        let top = ExpressionHoverCard.placement(
            anchor: NSRect(x: 400, y: 760, width: 22, height: 22), size: size, screen: screen)
        #expect(top.below)
        #expect(top.frame.maxY == 760 - ExpressionHoverCard.gap)

        let edge = ExpressionHoverCard.placement(
            anchor: NSRect(x: 10, y: 300, width: 22, height: 22), size: size, screen: screen)
        #expect(edge.frame.minX == 4)
        #expect(abs(edge.arrowX - 17) < 0.001, "\(edge)")
    }

    // A sticker is a button VoiceOver names; it isn't a Tab stop among the messages.
    @Test
    func aStickerIsNamedForVoiceOver() throws {
        show("hi", stickers: ["wave"])
        let sticker = try #require(MessageMenuAreaTests.views(StickerView.self, in: try list.cell(of: 1)).first)

        #expect(sticker.accessibilityLabel() == "Sticker, wave")
        #expect(sticker.refusesFirstResponder)
    }

    @Test
    func aClickOnAStickerOpensItsPopout() throws {
        show("hi", stickers: ["wave"])
        let sticker = try #require(MessageMenuAreaTests.views(StickerView.self, in: try list.cell(of: 1)).first)

        sticker.performClick(nil)

        guard case .sticker(let clicked) = actions.popouts.first else {
            Issue.record("a sticker popout")
            return
        }
        #expect(clicked.name == "wave")
        #expect(list.controller.expressionPopout.windows.panel != nil)
    }

    // A drag that starts on an emoji selects; only a click opens the popout.
    @Test
    func aDragFromAnEmojiSelects() throws {
        show("hi <:wave:5> there and more")
        let view = try text()
        view.setSelectedRange(NSRange(location: 0, length: 0))
        let start = try point(at: 3, in: view)

        try click(at: start, in: view, dragTo: NSPoint(x: start.x + 60, y: start.y))

        #expect(actions.popouts.isEmpty)
        #expect(view.selectedRange().length > 0)
    }

    func click(at point: NSPoint, in view: NSView, dragTo end: NSPoint? = nil, draining: Bool = true) throws {
        let window = try #require(view.window)
        // Events other suites posted and nothing consumed (CI's test host isn't the active app)
        // would join this click's tracking loop.
        while draining, NSApp.nextEvent(
            matching: [.leftMouseDown, .leftMouseDragged, .leftMouseUp],
            until: Date(timeIntervalSinceNow: 0.05), inMode: .default, dequeue: true) != nil
        {}
        let start = ProcessInfo.processInfo.systemUptime
        let event = { (type: NSEvent.EventType, at: NSPoint, after: TimeInterval) in
            NSEvent.mouseEvent(
                with: type, location: at, modifierFlags: [], timestamp: start + after,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1)
        }
        if let end, let dragged = event(.leftMouseDragged, end, 0.01) {
            NSApp.postEvent(dragged, atStart: false)
        }
        if let up = event(.leftMouseUp, end ?? point, 0.02) {
            NSApp.postEvent(up, atStart: false)
        }
        // As the window gives it to the text view; the release waits for its tracking loop.
        view.mouseDown(with: try #require(event(.leftMouseDown, point, 0)))
        window.layoutIfNeeded()
    }
}
