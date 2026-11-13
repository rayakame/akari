import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The band over the composer's text while a reply is pending: the official client's measures
// (its stylesheet, canary 671549), from the band's top left.
@MainActor
struct ReplyBarTests {
    final class Calls {
        var jumps = 0
        var toggles = 0
        var cancels = 0
    }

    let calls = Calls()

    func pending(mention: Bool = true, toggle: Bool = true, name: String = "Ray") -> PendingReply {
        PendingReply(
            messageId: MessageId(rawValue: 5), authorName: name, mention: mention, showsToggle: toggle)
    }

    func bar(
        _ reply: PendingReply, width: CGFloat = 500, scale: CGFloat = 1,
        appearance: NSAppearance.Name = .darkAqua
    ) -> ReplyBarView {
        let window = ScaledWindow.make(scale: scale)
        window.appearance = NSAppearance(named: appearance)
        let bar = ReplyBarView()
        bar.onJump = { [calls] in calls.jumps += 1 }
        bar.onToggle = { [calls] in calls.toggles += 1 }
        bar.onCancel = { [calls] in calls.cancels += 1 }
        bar.frame = NSRect(
            x: 0, y: 0, width: width, height: ReplyBarView.height(showsToggle: reply.showsToggle))
        window.contentView?.addSubview(bar)
        bar.show(reply)
        bar.layoutSubtreeIfNeeded()
        return bar
    }

    func click(_ type: NSEvent.EventType, at point: NSPoint, in bar: ReplyBarView) throws -> NSEvent {
        try #require(
            NSEvent.mouseEvent(
                with: type, location: bar.convert(point, to: nil), modifierFlags: [], timestamp: 0,
                windowNumber: bar.window?.windowNumber ?? 0, context: nil, eventNumber: 0,
                clickCount: 1, pressure: 1))
    }

    @Test
    func theBandIs37WithTheSwitchAnd35Without() {
        #expect(ReplyBarView.height(showsToggle: true) == 37)
        #expect(ReplyBarView.height(showsToggle: false) == 35)
        #expect(ComposerView.replyLine == 1)
    }

    @Test(arguments: [1, 2] as [CGFloat])
    func theSwitchTheLineAndTheCrossEndFourFromTheRight(scale: CGFloat) throws {
        let bar = bar(pending(), scale: scale)

        // 3 kept clear, then a row of 34: ✕ is 16 with 8 above and below and 12 beside it.
        #expect(bar.close.frame == NSRect(x: 456, y: 4, width: 40, height: 32))
        #expect(bar.close.iconFrame == NSRect(x: 12, y: 8, width: 16, height: 16))
        #expect(bar.close.labelFrame == nil)
        #expect(bar.separator.frame == NSRect(x: 455, y: 10, width: 1, height: 20))
        #expect(!bar.separator.isHidden && !bar.toggle.isHidden)

        let toggle = bar.toggle.frame
        #expect(toggle.maxX == 455 && toggle.minY == 3 && toggle.height == 34)
        #expect(bar.toggle.iconFrame == NSRect(x: 12, y: 9, width: 16, height: 16))
        let word = try #require(bar.toggle.labelFrame)
        #expect(word.minX == 32, "4 after the symbol")
        #expect(toggle.width == 32 + word.width + 12)
        #expect(abs(word.midY - 17) <= 0.5)

        let label = bar.labelFrame
        #expect(label.minX == 16 && label.height == 18)
        #expect(label.midY == 20, "the row's middle")
        #expect(label.maxX <= toggle.minX)
        for rect in [bar.close.frame, bar.separator.frame, toggle, label] {
            #expect((rect.minX * scale).rounded() == rect.minX * scale)
            #expect((rect.minY * scale).rounded() == rect.minY * scale)
            #expect((rect.width * scale).rounded() == rect.width * scale)
        }
    }

    @Test
    func withoutTheSwitchOnlyTheCrossShows() {
        let bar = bar(pending(toggle: false))

        #expect(bar.bounds.height == 35)
        #expect(bar.toggle.isHidden && bar.separator.isHidden)
        #expect(bar.close.frame == NSRect(x: 456, y: 3, width: 40, height: 32))
        #expect(bar.labelFrame.midY == 19)
        #expect(bar.hitTest(NSPoint(x: 440, y: 20)) === bar, "where the switch would be")
    }

    @Test
    func theLabelNamesTheAuthorSemibold() throws {
        let bar = bar(pending(name: "Ray"))
        let label = bar.label

        #expect(label.string == "Replying to Ray")
        let plain = try #require(label.attribute(.font, at: 0, effectiveRange: nil) as? NSFont)
        let name = try #require(label.attribute(.font, at: 12, effectiveRange: nil) as? NSFont)
        #expect(plain.pointSize == TypeScale.size(14) && name.pointSize == TypeScale.size(14))
        #expect(NSFontManager.shared.weight(of: name) > NSFontManager.shared.weight(of: plain))
        #expect(plain == NSFont.systemFont(ofSize: TypeScale.size(14)))
        #expect(name == NSFont.systemFont(ofSize: TypeScale.size(14), weight: .semibold))
        #expect(label.attribute(.foregroundColor, at: 0, effectiveRange: nil) as? NSColor === Palette.textDefault)
        #expect(label.attribute(.foregroundColor, at: 12, effectiveRange: nil) as? NSColor === Palette.textDefault)
        #expect(bar.accessibilityLabel() == "Replying to Ray")
    }

    @Test
    func aLongNameIsCutBeforeTheSwitch() {
        let bar = bar(pending(name: String(repeating: "A very long name ", count: 12)), width: 320)

        #expect(bar.labelFrame.minX == 16)
        #expect(bar.labelFrame.maxX == bar.toggle.frame.minX)
        #expect(bar.label.size().width > bar.labelFrame.width)

        let short = self.bar(pending(name: "Al"), width: 320)
        #expect(short.labelFrame.width == ceil(short.label.size().width))
    }

    @Test
    func theSwitchSaysOnOrOffInItsColorWithItsTooltip() {
        let on = bar(pending(mention: true))
        #expect(on.toggle.title == "ON")
        #expect(on.toggle.tint === Palette.textLink)
        #expect(on.toggle.toolTip == "Click to disable pinging the original author.")
        #expect(on.toggle.accessibilityLabel() == "Mention")
        #expect(on.toggle.accessibilityValue() as? String == "on")

        let off = bar(pending(mention: false))
        #expect(off.toggle.title == "OFF")
        #expect(off.toggle.tint === Palette.textMuted)
        #expect(off.toggle.toolTip == "Click to enable pinging the original author.")
        #expect(off.toggle.accessibilityValue() as? String == "off")
        #expect(off.toggle.frame.width > on.toggle.frame.width, "the longer word")
        #expect(off.toggle.frame.maxX == on.toggle.frame.maxX)

        off.show(pending(mention: true))
        off.layoutSubtreeIfNeeded()
        #expect(off.toggle.title == "ON" && off.toggle.frame == on.toggle.frame)
    }

    @Test
    func theCrossIsNamedAndBrightensUnderThePointer() {
        let bar = bar(pending())
        #expect(bar.close.accessibilityLabel() == "Cancel reply")
        #expect(bar.close.toolTip == "Cancel reply")
        #expect(bar.close.tint === Palette.interactiveText)

        bar.pointerMoved(to: NSPoint(x: bar.close.frame.midX, y: bar.close.frame.midY))
        #expect(bar.close.tint === Palette.interactiveTextActive)

        // Showing the reply again, as a toggle does, keeps it lit.
        bar.show(pending(mention: false))
        #expect(bar.close.tint === Palette.interactiveTextActive)

        bar.pointerMoved(to: NSPoint(x: bar.toggle.frame.midX, y: 20))
        #expect(bar.close.tint === Palette.interactiveText)
        #expect(bar.toggle.tint === Palette.textMuted, "the switch has no hover color")

        bar.pointerMoved(to: NSPoint(x: bar.close.frame.midX, y: 20))
        bar.pointerMoved(to: nil)
        #expect(bar.close.tint === Palette.interactiveText)
    }

    @Test
    func theSwitchAndTheCrossTakeTheirClicksAndTheBandJumps() throws {
        let bar = bar(pending())
        let toggle = NSPoint(x: bar.toggle.frame.midX, y: bar.toggle.frame.midY)
        let close = NSPoint(x: bar.close.frame.midX, y: bar.close.frame.midY)

        #expect(bar.hitTest(toggle) === bar.toggle)
        #expect(bar.hitTest(close) === bar.close)
        #expect(bar.hitTest(NSPoint(x: bar.close.frame.minX + 1, y: 6)) === bar.close, "its padding too")
        #expect(bar.hitTest(NSPoint(x: 100, y: 20)) === bar)
        #expect(bar.hitTest(NSPoint(x: 100, y: 1)) === bar, "the clear 3 belong to the band")

        bar.toggle.performClick(nil)
        #expect((calls.toggles, calls.cancels, calls.jumps) == (1, 0, 0))
        bar.close.performClick(nil)
        #expect((calls.toggles, calls.cancels, calls.jumps) == (1, 1, 0))

        bar.mouseDown(with: try click(.leftMouseDown, at: NSPoint(x: 100, y: 20), in: bar))
        #expect(calls.jumps == 0, "on the release")
        bar.mouseUp(with: try click(.leftMouseUp, at: NSPoint(x: 100, y: 20), in: bar))
        #expect((calls.toggles, calls.cancels, calls.jumps) == (1, 1, 1))

        // Pressed on the band and let go outside it: nothing.
        bar.mouseDown(with: try click(.leftMouseDown, at: NSPoint(x: 100, y: 20), in: bar))
        bar.mouseUp(with: try click(.leftMouseUp, at: NSPoint(x: 100, y: 80), in: bar))
        #expect(calls.jumps == 1)
    }

    // A token as the window draws it.
    final class Fill: NSView {
        let token: NSColor

        init(_ token: NSColor) {
            self.token = token
            super.init(frame: NSRect(x: 0, y: 60, width: 10, height: 10))
        }

        required init?(coder: NSCoder) {
            nil
        }

        override func draw(_ dirtyRect: NSRect) {
            token.setFill()
            bounds.fill()
        }
    }

    static func drawn(_ view: NSView, at point: NSPoint) throws -> NSColor {
        let rep = try #require(view.bitmapImageRepForCachingDisplay(in: view.bounds))
        view.cacheDisplay(in: view.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / view.bounds.width
        return try #require(rep.colorAt(x: Int(point.x * scale), y: Int(point.y * scale))?.usingColorSpace(.sRGB))
    }

    static func same(_ a: NSColor, _ b: NSColor) -> Bool {
        abs(a.redComponent - b.redComponent) < 0.02 && abs(a.greenComponent - b.greenComponent) < 0.02
            && abs(a.blueComponent - b.blueComponent) < 0.02
    }

    static func token(_ token: NSColor, in window: NSWindow?) throws -> NSColor {
        let fill = Fill(token)
        window?.contentView?.addSubview(fill)
        defer { fill.removeFromSuperview() }
        return try drawn(fill, at: NSPoint(x: 5, y: 5))
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theBandIsTheHigherSurfaceAndItsLineTheSubtleBorder(appearance: NSAppearance.Name) throws {
        let bar = bar(pending(), appearance: appearance)

        let band = try Self.drawn(bar, at: NSPoint(x: 300, y: 1))
        #expect(Self.same(band, try Self.token(Palette.surfaceHigher, in: bar.window)))
        if appearance == .darkAqua {
            #expect(!Self.same(band, try Self.token(Palette.composer, in: bar.window)))
        }
        var line: CGColor?
        bar.effectiveAppearance.performAsCurrentDrawingAppearance {
            line = Palette.borderSubtle.cgColor
        }
        #expect(bar.separator.layer?.backgroundColor == line)
    }

    @Test
    func anAppearanceChangeRecolorsTheSeparator() throws {
        let bar = bar(pending())
        let dark = try #require(bar.separator.layer?.backgroundColor)

        bar.window?.appearance = NSAppearance(named: .aqua)

        var line: CGColor?
        bar.effectiveAppearance.performAsCurrentDrawingAppearance {
            line = Palette.borderSubtle.cgColor
        }
        #expect(bar.separator.layer?.backgroundColor == line)
        #expect(bar.separator.layer?.backgroundColor != dark)
    }

    @Test
    func theButtonsDrawSymbolsOf16() {
        let bar = bar(pending())

        #expect(ReplyBarButton.iconSize == 16 && ReplyBarButton.iconGap == 4)
        #expect(bar.toggle.symbolName == "at")
        #expect(bar.close.symbolName == "xmark.circle.fill")
    }
}

// The band in the composer's card, and the list over it.
@MainActor
struct ReplyBarScreenTests {
    let screens = SessionScreenTests()
    // The fake lets the user send only with its thread.
    let account = FormattedAccount(contents: ["test"], thread: true)
    let channel = FormattedStore.channelIds[0]

    init() {
        account.formattedStore.answerable.withLock { $0 = true }
    }

    func message(_ number: UInt64) -> MessageId {
        MessageId(rawValue: channel.rawValue * 1000 + number)
    }

    func row(_ number: UInt64, in list: MessageListModel) throws -> MessageListModel.Row {
        try #require(list.rows.first { $0.message.id == message(number) })
    }

    func bar(in host: NSView) -> ReplyBarView? {
        screens.views(ReplyBarView.self, in: host).first
    }

    // A view's frame from the window's top left.
    func place(of view: NSView) -> NSRect {
        let rect = view.convert(view.bounds, to: nil)
        let height = view.window?.contentView?.bounds.height ?? 0
        return NSRect(x: rect.minX, y: height - rect.maxY, width: rect.width, height: rect.height)
    }

    func cell(_ number: UInt64, in table: MessageTableView) throws -> MessageCell {
        let controller = try #require(table.delegate as? MessageTableController)
        let index = try #require(controller.timeline.items.firstIndex { $0.id == .message(message(number)) })
        return try #require(table.view(atColumn: 0, row: index, makeIfNecessary: false) as? MessageCell)
    }

    @Test
    func theBandIsTheCardsTopPartAndTheCardGrowsByIt() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let list = try #require(session.messages)
        let field = try #require(screens.views(ComposerTextView.self, in: host).first?.enclosingScrollView)
        let before = place(of: field)
        let tableBefore = place(of: try #require(table.enclosingScrollView))

        list.composer.startReply(to: try row(38, in: list), shift: false)
        try await screens.waitUntil(host) { bar(in: host) != nil }
        host.layoutSubtreeIfNeeded()

        let band = place(of: try #require(bar(in: host)))
        let after = place(of: field)
        #expect(band.height == 37)
        #expect(band.minX == after.minX && band.width == after.width, "as wide as the card")
        #expect(after.minY == band.maxY + 1, "a line of 1 between them")
        #expect(after.maxY == before.maxY && after.height == before.height, "the text stays where it was")
        let tableAfter = place(of: try #require(table.enclosingScrollView))
        #expect(tableAfter.minY == tableBefore.minY)
        #expect(tableAfter.maxY == tableBefore.maxY - 38, "the list gives the band its room")
        #expect(band.minY == tableAfter.maxY)

        // Without the switch the band is 2 lower.
        account.formattedStore.answerable.withLock { $0 = false }
        list.composer.cancelReply()
        try await screens.waitUntil(host) { bar(in: host) == nil }
        #expect(place(of: try #require(table.enclosingScrollView)) == tableBefore)
    }

    @Test
    func theCardKeepsItsRoundCornersOverTheBand() async throws {
        let session = screens.formattedSession(account)
        let list = try #require(session.messages)
        list.composer.startReply(to: try row(38, in: list), shift: false)
        let view = ComposerView(composer: list.composer, placeholder: "Message", areaHeight: 400)
            .frame(width: 400)
            .background(Color(nsColor: Palette.chat))
        let host = NSHostingView(rootView: view)
        let window = TestWindow.make(size: NSSize(width: 400, height: 200))
        window.appearance = NSAppearance(named: .darkAqua)
        host.frame = NSRect(origin: .zero, size: host.fittingSize)
        window.contentView?.addSubview(host)
        host.layoutSubtreeIfNeeded()
        #expect(host.frame.size == NSSize(width: 400, height: 37 + 1 + 56 + 8))
        let band = try ReplyBarTests.token(Palette.surfaceHigher, in: window)
        let chat = try ReplyBarTests.token(Palette.chat, in: window)
        let pixel = { (x: CGFloat, y: CGFloat) in try ReplyBarTests.drawn(host, at: NSPoint(x: x, y: y)) }

        // The card starts 8 in; its corner of radius 8 leaves the first pixels to the chat.
        #expect(ReplyBarTests.same(try pixel(8.5, 0.5), chat))
        #expect(ReplyBarTests.same(try pixel(391.5, 0.5), chat))
        #expect(ReplyBarTests.same(try pixel(12, 4), band))
        #expect(ReplyBarTests.same(try pixel(388, 4), band))
        #expect(ReplyBarTests.same(try pixel(200, 1), band))
        #expect(ReplyBarTests.same(try pixel(4, 20), chat), "8 beside the card")
        #expect(ReplyBarTests.same(try pixel(8.5, 93.5), chat), "the bottom corners too")
        #expect(!ReplyBarTests.same(try pixel(200, 37.5), band), "the line under the band")
    }

    @Test
    func theAnsweredRowIsTintedWhileTheBandIsUp() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let list = try #require(session.messages)

        list.composer.startReply(to: try row(38, in: list), shift: false)
        try await screens.waitUntil(host) { (try? cell(38, in: table).isReplyTarget) == true }
        #expect(!(try cell(37, in: table)).isReplyTarget)
        #expect(bar(in: host)?.label.string == "Replying to User 2")

        list.composer.startReply(to: try row(37, in: list), shift: false)
        try await screens.waitUntil(host) { (try? cell(37, in: table).isReplyTarget) == true }
        #expect(!(try cell(38, in: table)).isReplyTarget)
        #expect(screens.views(ReplyBarView.self, in: host).count == 1)

        try #require(bar(in: host)).close.performClick(nil)
        try await screens.waitUntil(host) { bar(in: host) == nil }
        #expect(list.composer.reply == nil)
        #expect(!(try cell(37, in: table)).isReplyTarget)
    }

    @Test
    func theSwitchTurnsThePingOffAndOn() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        _ = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let list = try #require(session.messages)
        list.composer.startReply(to: try row(38, in: list), shift: false)
        try await screens.waitUntil(host) { bar(in: host)?.toggle.title == "ON" }

        try #require(bar(in: host)).toggle.performClick(nil)
        try await screens.waitUntil(host) { bar(in: host)?.toggle.title == "OFF" }
        #expect(list.composer.reply?.mention == false)

        try #require(bar(in: host)).toggle.performClick(nil)
        try await screens.waitUntil(host) { bar(in: host)?.toggle.title == "ON" }
        #expect(list.composer.reply?.mention == true)
    }

    // The band takes its room from the list's bottom: what a reader scrolled up sees stays put.
    @Test
    func theBandMovesNothingAReaderScrolledUpSees() async throws {
        account.formattedStore.add(80, to: channel)
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let controller = try #require(table.delegate as? MessageTableController)
        let index = try #require(controller.timeline.items.firstIndex { $0.id == .message(message(60)) })
        table.scrollRowToVisible(index)
        let clip = try #require(table.enclosingScrollView).contentView
        clip.scroll(to: NSPoint(x: 0, y: table.rect(ofRow: index).minY - 40))
        table.enclosingScrollView?.reflectScrolledClipView(clip)
        host.layoutSubtreeIfNeeded()
        try #require(!controller.isPinnedToBottom)
        let origin = clip.bounds.origin
        let before = MessageTableTests.visibleViews(in: controller).mapValues { view in
            view.view.map { place(of: $0).minY }
        }
        try #require(before.count > 3)
        let list = try #require(session.messages)

        list.composer.startReply(to: try row(60, in: list), shift: false)
        try await screens.waitUntil(host) { bar(in: host) != nil }
        host.layoutSubtreeIfNeeded()

        #expect(clip.bounds.origin == origin)
        let after = MessageTableTests.visibleViews(in: controller).mapValues { view in
            view.view.map { place(of: $0).minY }
        }
        for (raw, y) in after {
            #expect(before[raw] == y, "\(raw)")
        }
        #expect(after.count >= before.count - 2 && !after.isEmpty)

        list.composer.cancelReply()
        try await screens.waitUntil(host) { bar(in: host) == nil }
        host.layoutSubtreeIfNeeded()
        #expect(clip.bounds.origin == origin)
    }

    // At the present the newest message stays above the band.
    @Test
    func aListAtThePresentStaysAtItsNewestMessage() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        _ = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let hidden = try screens.newestRowHidden(in: host)
        let list = try #require(session.messages)

        list.composer.startReply(to: try row(38, in: list), shift: false)
        try await screens.waitUntil(host) { bar(in: host) != nil }
        try await screens.waitUntil(host) { try screens.newestRowHidden(in: host) == hidden }

        list.composer.cancelReply()
        try await screens.waitUntil(host) { bar(in: host) == nil }
        try await screens.waitUntil(host) { try screens.newestRowHidden(in: host) == hidden }
    }

    @Test
    func startingAReplyFocusesTheComposer() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let composer = try #require(screens.views(ComposerTextView.self, in: host).first)
        try #require(composer.isEditable)
        window.makeFirstResponder(table)
        try #require(window.firstResponder === table)
        let list = try #require(session.messages)

        list.composer.startReply(to: try row(38, in: list), shift: false)

        try await screens.waitUntil(host) { window.firstResponder === composer }
    }

    // Escape's order: the pending reply first; only the next one marks read.
    @Test
    func escapeCancelsTheReplyBeforeItMarksRead() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        _ = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let composer = try #require(screens.views(ComposerTextView.self, in: host).first)
        let list = try #require(session.messages)
        list.composer.draft = "kept"
        list.composer.startReply(to: try row(38, in: list), shift: false)
        try await screens.waitUntil(host) { bar(in: host) != nil }

        composer.onEscape()
        try await screens.waitUntil(host) { bar(in: host) == nil }
        try await Task.sleep(for: .milliseconds(50))
        #expect(account.markedRead.withLock { $0 }.isEmpty)
        #expect(list.composer.draft == "kept")

        composer.onEscape()
        try await screens.waitUntil(host) { account.markedRead.withLock { $0 } == [channel] }
    }
}

// Escape in a list of its own (no session behind it): the same order.
@MainActor
struct ReplyEscapeBridgeTests {
    let list = MessageTableTests()
    let account = FormattedAccount(contents: ["test"])

    @Test
    func aPendingReplyTakesEscapeAndNothingScrolls() async throws {
        account.formattedStore.answerable.withLock { $0 = true }
        account.formattedStore.add(60, to: FormattedStore.channelIds[0])
        let session = SessionScreenTests().formattedSession(account)
        let messages = try #require(session.messages)
        let bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in }, markServerRead: {})
        bridge.link.controller = list.controller
        list.controller.actions = bridge
        list.show(MessageTableState(rows: messages.rows))
        list.scroll(toRowOf: messages.rows[20].message.id.rawValue)
        let origin = list.clip.bounds.origin
        try #require(!list.controller.isPinnedToBottom)
        messages.composer.startReply(to: messages.rows[20], shift: false)

        bridge.escape()
        try await SessionScreenTests().waitUntil(list.table) { messages.composer.reply == nil }
        #expect(account.markedRead.withLock { $0 }.isEmpty)
        #expect(list.clip.bounds.origin == origin, "the list stays where it is read")

        bridge.escape()
        try await SessionScreenTests().waitUntil(list.table) {
            !account.markedRead.withLock { $0 }.isEmpty
        }
        #expect(list.controller.isPinnedToBottom)
    }
}

// The row a pending reply answers: the official client's tint, drawn by the cell from the
// list's state.
@MainActor
struct ReplyTargetTintTests {
    let highlights = MentionHighlightTests()

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theAnsweredRowHasTheBrandTintAndBar(appearance: NSAppearance.Name) throws {
        let cell = highlights.cell(mentioned: false, appearance: appearance)
        cell.isReplyTarget = true

        #expect(try highlights.matches(try highlights.pixel(cell, x: 300), [Palette.replyingBackground]))
        #expect(try highlights.matches(try highlights.pixel(cell, x: 1), [Palette.replyingBackground, Palette.brand]))
        #expect(try !highlights.matches(try highlights.pixel(cell, x: 3), [Palette.replyingBackground, Palette.brand]))
        #expect(try highlights.matches(try highlights.pixel(cell, x: 300, y: 15), []), "not over the group's spacing")
        #expect(try !highlights.matches(try highlights.pixel(cell, x: 300), []))
    }

    // The sheet's later rule: the reply's tint takes the mention's place, hovered its lighter one.
    @Test
    func itReplacesTheMentionsTintAndTheHoverTint() throws {
        let cell = highlights.cell(mentioned: true)
        cell.isReplyTarget = true
        #expect(try highlights.matches(try highlights.pixel(cell, x: 300), [Palette.replyingBackground]))
        #expect(try !highlights.matches(try highlights.pixel(cell, x: 300), [Palette.mentionedBackground]))

        cell.isHovered = true
        let hovered = try highlights.pixel(cell, x: 300)
        #expect(try highlights.matches(hovered, [Palette.replyingBackgroundHover]))
        #expect(try !highlights.matches(hovered, [Palette.replyingBackground]))

        cell.isReplyTarget = false
        #expect(
            try highlights.matches(
                try highlights.pixel(cell, x: 300), [Palette.mentionedBackground, Palette.hoverBackground]))
    }

    @Test
    func theTintsAlphasAreTheSheets() throws {
        for (token, alpha) in [(Palette.replyingBackground, 0.16), (Palette.replyingBackgroundHover, 0.10)] {
            let color = try #require(token.usingColorSpace(.sRGB))
            #expect(abs(color.alphaComponent - alpha) < 0.001)
            #expect(abs(color.redComponent * 255 - 0x58) < 0.5 && abs(color.blueComponent * 255 - 0xf2) < 0.5)
        }
    }

    @Test
    func theListTintsTheRowWithoutReloadingIt() throws {
        let list = MessageTableTests()
        let rows = list.messages(5)
        list.show(MessageTableState(rows: rows))
        list.table.forget()

        list.show(MessageTableState(rows: rows, replyingTo: MessageId(rawValue: 3)))

        #expect(list.table.reloaded.isEmpty)
        #expect((try #require(try list.cell(of: 3) as? MessageCell)).isReplyTarget)
        #expect(!(try #require(try list.cell(of: 2) as? MessageCell)).isReplyTarget)

        list.show(MessageTableState(rows: rows + list.messages(1, from: 6), replyingTo: MessageId(rawValue: 3)))
        #expect((try #require(try list.cell(of: 3) as? MessageCell)).isReplyTarget)

        list.show(MessageTableState(rows: rows))
        #expect(!(try #require(try list.cell(of: 3) as? MessageCell)).isReplyTarget)
    }
}
