import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

// The flash's upper layer (the official client's stylesheet, canary 671549): the same color
// again with a bar on its left, blinking twice in its first 0.6 s.
@MainActor
struct FlashOverlayTests {
    final class Clock {
        var now = berlinTime(2026, 10, 10, 15, 0)
    }

    let clock = Clock()
    let list: MessageTableTests

    init() {
        list = MessageTableTests(onScreen: MessageTableController.isOnScreen, now: { [clock] in clock.now })
        list.window.appearance = NSAppearance(named: .darkAqua)
        list.controller.reducesMotion = { false }
    }

    func jumped(to raw: UInt64) throws -> MessageCell {
        list.show(MessageTableState(rows: list.page))
        list.show(
            MessageTableState(
                rows: list.page,
                messageJump: MessageListModel.MessageJump(messageId: MessageId(rawValue: raw), number: 1)))
        return try #require(try list.cell(of: raw) as? MessageCell)
    }

    @Test
    func theTimesAreTheSheets() {
        #expect(MessageCell.flashFadeIn == 0.2)
        #expect(MessageCell.flashHold == 2)
        #expect(MessageCell.flashFadeOut == 0.6)
        #expect(MessageCell.flashBlink == 0.6)
    }

    // Invariants of the two animations, not their frames: they start together, last as long,
    // the blink is over before the hold ends, and both end at nothing.
    @Test
    func theUpperLayerBlinksTwiceThenHoldsAndFadesWithTheLower() throws {
        let cell = try jumped(to: 120)
        let base = try #require(cell.flashAnimation as? CAKeyframeAnimation)
        let blink = try #require(cell.flashOverlayAnimation as? CAKeyframeAnimation)

        #expect(blink.keyPath == "opacity")
        #expect(blink.duration == base.duration && blink.duration == 2.6)
        #expect(abs(blink.beginTime - base.beginTime) < 0.05)
        let values = try #require(blink.values as? [Double])
        let times = try #require(blink.keyTimes?.map { $0.doubleValue * blink.duration })
        #expect(values.count == times.count)
        #expect(times == times.sorted() && Set(times).count == times.count)
        #expect(times.first == 0 && abs((times.last ?? 0) - 2.6) < 1e-9)
        #expect(values.first == 1 && values.last == 0)
        // Out twice, at a fifth and three fifths of 0.6 s, back in between and after.
        let outs = zip(times, values).filter { $0.1 == 0 }.map(\.0).dropLast()
        #expect(outs.count == 2)
        #expect(abs((outs.first ?? 0) - 0.12) < 1e-9 && abs((outs.last ?? 0) - 0.36) < 1e-9)
        let backIn = zip(times, values).filter { $0.1 == 1 }.map(\.0)
        #expect(backIn.contains { abs($0 - 0.24) < 1e-9 } && backIn.contains { abs($0 - 0.48) < 1e-9 })
        #expect(outs.allSatisfy { $0 < MessageCell.flashBlink })
        #expect(backIn.contains { abs($0 - MessageCell.flashHold) < 1e-9 }, "full until the fade out")
        #expect(blink.timingFunctions?.count == values.count - 1)
        #expect(base.timingFunctions?.count == (base.values?.count ?? 0) - 1)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theUpperLayerIsTheHighlightAgainWithABrandBarOfTwo(appearance: NSAppearance.Name) throws {
        list.window.appearance = NSAppearance(named: appearance)
        let cell = try jumped(to: 120)

        var highlight: CGColor?
        var bar: CGColor?
        cell.effectiveAppearance.performAsCurrentDrawingAppearance {
            highlight = Palette.messageHighlight.cgColor
            bar = Palette.textBrand.cgColor
        }
        #expect(cell.flashOverlay.backgroundColor == highlight)
        #expect(cell.flashLayer.backgroundColor == highlight)
        #expect(cell.flashBar.backgroundColor == bar)
        #expect(cell.flashBar.frame == NSRect(x: 0, y: 0, width: 2, height: cell.box.height))
        #expect(cell.flashBar.superlayer === cell.flashOverlay)
        #expect(cell.flashOverlay.frame == cell.flashLayer.frame)
        #expect(cell.flashFrame == cell.box)
        #expect(cell.flashOverlay.bounds.size == cell.box.size)
        #expect(cell.flashOverlay.cornerRadius == 4)
        #expect(cell.flashOverlay.maskedCorners == [.layerMaxXMinYCorner, .layerMaxXMaxYCorner])
        // At rest neither shows.
        #expect(cell.flashOverlay.opacity == 0 && cell.flashLayer.opacity == 0)
    }

    @Test
    func theLayersLieUnderTheMessage() throws {
        let cell = try jumped(to: 120)
        let lower = try #require(cell.subviews.firstIndex { $0.layer === cell.flashLayer })
        let upper = try #require(cell.subviews.firstIndex { $0.layer === cell.flashOverlay })

        #expect(lower == 0 && upper == 1)
        #expect(cell.subviews[lower].hitTest(NSPoint(x: 100, y: 10)) == nil)
        #expect(cell.subviews[upper].hitTest(NSPoint(x: 100, y: 10)) == nil)
    }

    @Test
    func withReducedMotionNeitherLayerFadesOrBlinks() throws {
        list.controller.reducesMotion = { true }
        let cell = try jumped(to: 120)

        for animation in [cell.flashAnimation, cell.flashOverlayAnimation] {
            let hold = try #require(animation as? CABasicAnimation)
            #expect(hold.fromValue as? Double == 1 && hold.toValue as? Double == 1)
            #expect(hold.duration == 2)
        }
    }

    @Test
    func aCellShownAgainCarriesBothLayersOn() throws {
        let cell = try jumped(to: 120)
        clock.now += 1
        list.table.reloadData(forRowIndexes: [list.index(of: 120)], columnIndexes: [0])
        let shown = try #require(try list.cell(of: 120) as? MessageCell)

        let base = try #require(shown.flashAnimation)
        let overlay = try #require(shown.flashOverlayAnimation)
        let now = shown.flashOverlay.convertTime(CACurrentMediaTime(), from: nil)
        #expect(abs(now - overlay.beginTime - 1) < 0.1)
        #expect(abs(overlay.beginTime - base.beginTime) < 0.05)

        cell.flash(elapsed: nil, reducesMotion: false)
        shown.flash(elapsed: nil, reducesMotion: false)
        #expect(shown.flashAnimation == nil && shown.flashOverlayAnimation == nil)
    }

    @Test
    func aSecondJumpEndsBothLayersOfTheFirst() throws {
        let first = try jumped(to: 121)

        list.show(
            MessageTableState(
                rows: list.page,
                messageJump: MessageListModel.MessageJump(messageId: MessageId(rawValue: 120), number: 2)))

        #expect(first.flashAnimation == nil && first.flashOverlayAnimation == nil)
        #expect(try #require(try list.cell(of: 120) as? MessageCell).flashOverlayAnimation != nil)
    }
}

// A click on a reply's line, as the table sees the model's jump land.
@MainActor
struct ReplyJumpScrollTests {
    let list = MessageTableTests()
    let actions = MessageTableTests.RecordingActions()

    init() {
        list.controller.actions = actions
        list.controller.reducesMotion = { false }
    }

    func jump(to raw: UInt64, _ number: Int) -> MessageListModel.MessageJump {
        MessageListModel.MessageJump(messageId: MessageId(rawValue: raw), number: number)
    }

    func offCenter(_ raw: UInt64) -> CGFloat {
        let bounds = list.clip.bounds
        let bottom = list.controller.scrollView.contentInsets.bottom
        return list.table.rect(ofRow: list.index(of: raw)).midY - (bounds.minY + (bounds.height - bottom) / 2)
    }

    func cell(_ raw: UInt64) throws -> MessageCell {
        try #require(try list.cell(of: raw) as? MessageCell)
    }

    var rows: [MessageListModel.Row] {
        list.page + [replyRow(300, by: author(3, "Kim"), at: list.noon + 300 * 120, to: 120, target())]
    }

    // The target is among the loaded rows: the list scrolls to it and flashes it.
    @Test
    func aClickOnTheLineGoesToALoadedTargetAndFlashesIt() throws {
        list.show(MessageTableState(rows: rows))
        #expect(list.controller.isPinnedToBottom)

        try cell(300).replyLine.content.performClick(nil)
        #expect(actions.calls.last == "jump to target of 300")
        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 1)))

        #expect(abs(offCenter(120)) <= 0.5, "\(offCenter(120))")
        #expect(try cell(120).flashAnimation != nil && cell(120).flashOverlayAnimation != nil)
        #expect(try cell(121).flashAnimation == nil)
        #expect(actions.shownJumps == [1])
    }

    // The target wasn't loaded: the page around it replaces the rows with the jump.
    @Test
    func aClickOnTheLineOfAnOlderTargetLandsWithThePageAroundIt() throws {
        let reply = replyRow(300, by: author(3, "Kim"), at: list.noon + 300 * 120, to: 46, .notLoaded)
        list.show(MessageTableState(rows: list.page + [reply]))

        try cell(300).spine.performClick(nil)
        #expect(actions.calls.last == "jump to target of 300")
        list.show(
            MessageTableState(
                rows: list.messages(50, from: 21), atPresent: false, messageJump: jump(to: 46, 1)))

        #expect(abs(offCenter(46)) <= 0.5, "\(offCenter(46))")
        #expect(try cell(46).flashAnimation != nil)
        #expect(!list.controller.isPinnedToBottom)
    }

    // The around load found the message gone: the list lands beside its place, nothing flashes.
    @Test
    func aTargetGoneFromThePageAroundItFlashesNothing() throws {
        list.show(MessageTableState(rows: list.page))

        let page = list.messages(50, from: 21).filter { $0.id.rawValue != 46 }
        list.show(MessageTableState(rows: page, atPresent: false, messageJump: jump(to: 46, 1)))

        #expect(abs(offCenter(47)) <= 0.5)
        #expect(try cell(47).flashAnimation == nil && cell(47).flashOverlayAnimation == nil)
    }

    // "Jump To Original Message": back at the reply, which flashes.
    @Test
    func theWayBackFlashesTheReply() throws {
        list.show(MessageTableState(rows: rows))
        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 1)))

        list.show(MessageTableState(rows: rows, messageJump: jump(to: 300, 2)))

        #expect(try cell(300).flashAnimation != nil)
        #expect(try cell(120).flashAnimation == nil, "one flash at a time")
        #expect(list.visibleRows.contains(list.index(of: 300)))
        #expect(actions.shownJumps == [1, 2])
    }

    // A channel opened for a jump has jumped before its table exists: the table's first rows
    // scroll to it, once, whichever table of the list comes first.
    @Test
    func aTableMadeAfterTheJumpScrollsToItOnce() throws {
        actions.lastShown = 0
        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 1)))

        #expect(abs(offCenter(120)) <= 0.5, "\(offCenter(120))")
        #expect(try cell(120).flashAnimation != nil)
        #expect(actions.shownJumps == [1])
    }

    @Test
    func aTableMadeForAListWhoseJumpWasShownStaysAtThePresent() throws {
        actions.lastShown = 1
        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 1)))

        #expect(list.controller.isPinnedToBottom)
        #expect(try cell(120).flashAnimation == nil)
        #expect(actions.shownJumps.isEmpty)

        // The next jump is this table's.
        list.show(MessageTableState(rows: rows, messageJump: jump(to: 120, 2)))
        #expect(abs(offCenter(120)) <= 0.5)
        #expect(actions.shownJumps == [2])
    }

    @Test
    func theTintFollowsTheJumpedRowsToo() throws {
        list.show(MessageTableState(rows: rows, replyingTo: MessageId(rawValue: 120)))
        list.show(
            MessageTableState(rows: rows, messageJump: jump(to: 120, 1), replyingTo: MessageId(rawValue: 120)))

        #expect(try cell(120).isReplyTarget && cell(120).flashAnimation != nil)
        list.scroll(toRowOf: 180)
        list.scroll(toRowOf: 120)
        #expect(try cell(120).isReplyTarget, "a cell made again is tinted again")
        #expect(try !cell(121).isReplyTarget)
    }
}

// The bar over the list's bottom while it shows what a reply answers.
@MainActor
struct ReplyReturnBarTests {
    @Test
    func awayFromThePresentAfterAReplysJumpTheBarOffersTheWayBack() {
        #expect(JumpBar.kind(atPresent: false, isStale: false, hasRows: true, fromReply: true) == .reply)
        #expect(
            JumpBar.kind(atPresent: false, isStale: false, hasRows: true, fromReply: false)
                == .present("You're reading older messages"))
        #expect(
            JumpBar.kind(atPresent: false, isStale: true, hasRows: true, fromReply: false)
                == .present("Some messages may be missing"))
        #expect(JumpBar.kind(atPresent: true, isStale: false, hasRows: true, fromReply: true) == nil)
        #expect(JumpBar.kind(atPresent: false, isStale: false, hasRows: false, fromReply: true) == nil)
    }

    @Test
    func itsWordsAreTheOfficialClients() {
        #expect(JumpBar.Kind.reply.text == "You Are Viewing a Reply.")
        #expect(JumpBar.Kind.reply.button == "Jump To Original Message")
        #expect(JumpBar.Kind.reply.symbol == "arrow.uturn.backward")
        #expect(JumpBar.Kind.present("x").text == "x")
        #expect(JumpBar.Kind.present("x").button == "Jump to present")
        #expect(JumpBar.Kind.present("x").symbol == "arrow.down")
    }

    @Test
    func theBarIsAsHighAsTheOtherAndItsButtonHasTheHand() throws {
        let host = NSHostingView(rootView: JumpBar(.reply) {}.frame(width: 500))
        let other = NSHostingView(rootView: JumpBar(.present("older")) {}.frame(width: 500))

        #expect(host.fittingSize == other.fittingSize)
        #expect(host.fittingSize.height == JumpBar.height + JumpBar.margin)
        let areas = BrowserButtonTests().handAreas(in: JumpBar(.reply) {}, size: NSSize(width: 500, height: 40))
        #expect(areas.contains { $0.contains(NSPoint(x: 400, y: 18)) }, "\(areas)")
        #expect(!areas.contains { $0.contains(NSPoint(x: 60, y: 18)) }, "\(areas)")
    }
}

// Through the app's screen: a reply in the fake's channel, its line, the click and the bar.
@MainActor
struct ReplyScreenTests {
    let screens = SessionScreenTests()
    let account = FormattedAccount(contents: ["test"], thread: true)
    let channel = FormattedStore.channelIds[0]

    init() {
        account.formattedStore.answerable.withLock { $0 = true }
        // The newest message answers the fifth, far above what the list shows.
        account.formattedStore.replies.withLock { $0 = [40: 5] }
    }

    func message(_ number: UInt64, in channel: ChannelId? = nil) -> MessageId {
        MessageId(rawValue: (channel ?? self.channel).rawValue * 1000 + number)
    }

    func cell(_ id: MessageId, in table: MessageTableView, make: Bool = false) throws -> MessageCell {
        let controller = try #require(table.delegate as? MessageTableController)
        let index = try #require(controller.timeline.items.firstIndex { $0.id == .message(id) })
        let cell = try #require(table.view(atColumn: 0, row: index, makeIfNecessary: make) as? MessageCell)
        cell.layoutSubtreeIfNeeded()
        return cell
    }

    func offCenter(_ id: MessageId, in table: MessageTableView) throws -> CGFloat {
        let controller = try #require(table.delegate as? MessageTableController)
        let row = try #require(controller.timeline.items.firstIndex { $0.id == .message(id) })
        let scroll = try #require(table.enclosingScrollView)
        let clip = scroll.contentView
        return table.rect(ofRow: row).midY
            - (clip.bounds.minY + (clip.bounds.height - scroll.contentInsets.bottom) / 2)
    }

    @Test
    func aClickOnAReplysLineCentersWhatItAnswersAndFlashesIt() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let reply = try cell(message(40), in: table)
        #expect(!reply.replyLine.isHidden)
        #expect(reply.replyLine.shownName == "User 3")
        #expect(reply.replyLine.content.shownText.string == "test 5")
        #expect(try offCenter(message(5), in: table) < -100, "the answered message is far above")

        reply.replyLine.content.performClick(nil)

        try await screens.waitUntil(host) { abs(try offCenter(message(5), in: table)) <= 0.5 }
        let list = try #require(session.messages)
        #expect(list.messageJump?.messageId == message(5))
        #expect(list.shownJump == 1)
        #expect(try cell(message(5), in: table).flashAnimation != nil)
        // The list never left the present, so nothing offers a way back.
        #expect(list.replyReturn == nil)
    }

    @Test
    func aClickOnTheBandGoesToTheAnsweredMessageToo() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let list = try #require(session.messages)
        list.composer.startReply(
            to: try #require(list.rows.first { $0.message.id == message(5) }), shift: false)
        try await screens.waitUntil(host) { !screens.views(ReplyBarView.self, in: host).isEmpty }
        let bar = try #require(screens.views(ReplyBarView.self, in: host).first)

        bar.onJump()

        try await screens.waitUntil(host) { abs(try offCenter(message(5), in: table)) <= 0.5 }
        let target = try cell(message(5), in: table)
        #expect(target.isReplyTarget && target.flashAnimation != nil)
        #expect(list.composer.reply?.messageId == message(5), "the reply stays pending")
    }

    // The split's list is a list like the other: the same line, and its click is its own.
    @Test
    func theSplitsListShowsTheLineAndJumpsOnItsOwn() async throws {
        let session = await screens.splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        try await screens.waitUntil(host) {
            screens.tables(in: host).count == 2
                && screens.tables(in: host).allSatisfy { table in
                    (try? cell(message(40, in: (table.delegate as? MessageTableController)?.state.channelId), in: table))
                        != nil
                }
        }
        let tables = screens.tables(in: host)
        let lines = try tables.map { table -> (MessageCell, NSRect) in
            let channel = try #require((table.delegate as? MessageTableController)?.state.channelId)
            let cell = try cell(message(40, in: channel), in: table)
            return (cell, cell.replyLine.convert(cell.replyLine.bounds, to: cell))
        }
        for (cell, line) in lines {
            #expect(!cell.replyLine.isHidden && !cell.spine.isHidden)
            #expect(line.minX == 72 && line.height == 18)
            #expect(line.maxX == cell.bounds.width - 16)
            #expect(cell.spine.frame.minX == 30 && cell.spine.frame.width == 38)
        }
        #expect(lines[0].1.width > lines[1].1.width, "the split is narrower")

        lines[1].0.replyLine.content.performClick(nil)

        let split = try #require(session.split)
        try await screens.waitUntil(host) { split.messageJump != nil }
        #expect(split.messageJump?.messageId == message(5, in: FormattedStore.threadId))
        #expect(session.messages?.messageJump == nil)
    }

    // Escape in the split cancels the split's reply and leaves the channel's.
    @Test
    func escapeCancelsThePaneItWasPressedInFirst() async throws {
        let session = await screens.splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        try await screens.waitUntil(host) { screens.views(ComposerTextView.self, in: host).count == 2 }
        let main = try #require(session.messages)
        let split = try #require(session.split)
        main.composer.startReply(to: try #require(main.rows.last), shift: false)
        split.composer.startReply(to: try #require(split.rows.last), shift: false)
        try await screens.waitUntil(host) { screens.views(ReplyBarView.self, in: host).count == 2 }
        let composers = screens.views(ComposerTextView.self, in: host).sorted {
            $0.convert($0.bounds, to: nil).minX < $1.convert($1.bounds, to: nil).minX
        }

        composers[1].onEscape()
        try await screens.waitUntil(host) { split.composer.reply == nil }
        #expect(main.composer.reply != nil)
        #expect(session.split != nil, "the split stays open")

        composers[1].onEscape()
        try await screens.waitUntil(host) { main.composer.reply == nil }
        #expect(session.split != nil)
        #expect(account.markedRead.withLock { $0 }.isEmpty)
    }
}

// A list away from the present: the bar over its bottom after a reply's jump, and its button.
@MainActor
struct ReplyWayBackScreenTests {
    let screens = SessionScreenTests()
    let account = FormattedAccount(contents: ["test"], thread: true)
    let channel = FormattedStore.channelIds[0]

    init() {
        account.formattedStore.answerable.withLock { $0 = true }
        account.formattedStore.replies.withLock { $0 = [40: 5] }
        account.formattedStore.away.withLock { $0 = true }
    }

    func message(_ number: UInt64) -> MessageId {
        MessageId(rawValue: channel.rawValue * 1000 + number)
    }

    // The bar's button: the one hand area of SwiftUI's over the list.
    func button(in host: NSView, over table: MessageTableView) throws -> NSRect {
        func areas(_ view: NSView) -> [CursorArea.Area] {
            ((view as? CursorArea.Area).map { [$0] } ?? []) + view.subviews.flatMap(areas)
        }
        let scroll = try #require(table.enclosingScrollView)
        let list = scroll.convert(scroll.bounds, to: nil)
        let found = areas(host).filter { $0.cursor == .pointingHand }.map { $0.convert($0.bounds, to: nil) }
            .filter { list.contains(NSPoint(x: $0.midX, y: $0.midY)) && $0.height <= JumpBar.height }
        try #require(found.count == 1, "\(found)")
        return found[0]
    }

    func click(_ point: NSPoint, in window: NSWindow) async throws {
        try settleEvents(in: window)
        let event = { (type: NSEvent.EventType) in
            NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1)
        }
        window.sendEvent(try #require(event(.leftMouseDown)))
        try await Task.sleep(for: .milliseconds(30))
        window.sendEvent(try #require(event(.leftMouseUp)))
    }

    @Test
    func theBarOffersTheWayBackAndItsButtonGoesToTheReply() async throws {
        let session = screens.formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = screens.shownWindow(host)
        defer { window.orderOut(nil) }
        let table = try await screens.shown(account.formattedStore.newest(of: channel), in: host)
        let list = try #require(session.messages)
        try #require(!list.atPresent)
        let toPresent = try button(in: host, over: table)
        let controller = try #require(table.delegate as? MessageTableController)
        let index = try #require(controller.timeline.items.firstIndex { $0.id == .message(message(40)) })
        let reply = try #require(table.view(atColumn: 0, row: index, makeIfNecessary: true) as? MessageCell)

        reply.replyLine.content.performClick(nil)

        try await screens.waitUntil(host) { list.replyReturn == message(40) }
        #expect(list.messageJump?.messageId == message(5))
        try await screens.waitUntil(host) { try button(in: host, over: table).width > toPresent.width }
        let back = try button(in: host, over: table)
        #expect(back.maxX == toPresent.maxX && back.minY == toPresent.minY, "the longer words, in the same bar")

        try await click(NSPoint(x: back.midX, y: back.midY), in: window)

        try await screens.waitUntil(host) { list.messageJump?.messageId == message(40) }
        #expect(list.replyReturn == nil)
        #expect(list.shownJump == 2)
        try await screens.waitUntil(host) { try button(in: host, over: table).width == toPresent.width }
    }
}

// Reply in the hovered message's bar and in its menu, through the app's bridge.
@MainActor
struct ReplyActionTests {
    let list = BarList()
    let account = FormattedAccount(contents: ["test"], thread: true)
    let session: SessionModel
    let messages: MessageListModel
    let bridge: MessageListBridge

    init() throws {
        account.formattedStore.answerable.withLock { $0 = true }
        session = SessionScreenTests().formattedSession(account)
        messages = try #require(session.messages)
        bridge = MessageListBridge(
            messages: messages, link: MessageTableLink(),
            links: LinkFollower(trusted: session.trustedLinks()), go: { _ in }, markServerRead: {},
            messageActions: session.messageActions(for: messages) { _ in })
        list.controller.actions = bridge
        list.show(messages.rows, bars: false)
    }

    var target: UInt64 { messages.rows[messages.rows.count - 3].message.id.rawValue }

    @Test
    func theBarHasReplyBeforeMoreWithItsArrow() throws {
        try list.hover(target)

        let reply = try #require(list.bar.button(.reply))
        #expect(reply.item.title == "Reply")
        #expect(reply.item.symbol == "arrowshape.turn.up.left")
        let actions = list.bar.buttons.map(\.item.action)
        #expect(try #require(actions.firstIndex(of: .reply)) < (try #require(actions.firstIndex(of: .more))))
    }

    @Test
    func aClickStartsAReplyWithThePing() throws {
        try list.hover(target)

        try #require(list.bar.button(.reply)).performClick(nil)

        let reply = try #require(messages.composer.reply)
        #expect(reply.messageId.rawValue == target)
        #expect(reply.mention && reply.showsToggle)
        #expect(messages.replyingTo?.rawValue == target)
    }

    @Test
    func aShiftClickStartsItWithoutThePing() throws {
        list.shift = true
        try list.hover(target)

        try #require(list.bar.button(.reply)).performClick(nil)

        let reply = try #require(messages.composer.reply)
        #expect(!reply.mention && reply.showsToggle)
    }

    @Test
    func theMenusReplyStartsOneToo() throws {
        let cell = try list.cell(of: target)
        let menu = try #require(cell.menu(for: try list.menuEvent(on: target)))
        let index = try #require(menu.items.firstIndex { $0.title == "Reply" })

        menu.performActionForItem(at: index)

        #expect(messages.composer.reply?.messageId.rawValue == target)
    }

    @Test
    func whereTheCoreDoesntAllowItNothingOffersReply() throws {
        account.formattedStore.answerable.withLock { $0 = false }
        try list.hover(target)

        #expect(list.bar.button(.reply) == nil)
        let menu = try #require(try list.cell(of: target).menu(for: try list.menuEvent(on: target)))
        #expect(!menu.items.contains { $0.title == "Reply" })
    }
}
