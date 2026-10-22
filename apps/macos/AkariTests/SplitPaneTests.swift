import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct ThreadMenuTests {
    let join = SessionModel.MembershipItem(joins: true, enabled: true)
    let leave = SessionModel.MembershipItem(joins: false, enabled: true)

    // The official full view's "…": Join or Leave Thread | Jump To Top, Open in Split View | …
    @Test
    func theFullViewsMenuOffersMembershipThenOpenInSplitView() {
        let menu = ThreadMenu.fullView(membership: leave, splittable: true)

        #expect(menu.sections == [[.membership(leave)], [.openInSplitView]])
        #expect(menu.sections.flatMap { $0 }.map(\.title) == ["Leave Thread", "Open in Split View"])
        #expect(menu.location == .toolbarOverflow)
    }

    // The official split view's "…": Join or Leave Thread, Open in Full View | …
    @Test
    func theSplitsMenuOffersMembershipAndOpenInFullView() {
        let menu = ThreadMenu.splitView(membership: join)

        #expect(menu.sections == [[.membership(join), .openInFullView]])
        #expect(menu.sections.flatMap { $0 }.map(\.title) == ["Join Thread", "Open in Full View"])
        #expect(menu.location == .sidebarOverflow)
    }

    // A forum's posts have no channel list to open beside.
    @Test
    func aThreadOfAForumHasNoOpenInSplitView() {
        let menu = ThreadMenu.fullView(membership: leave, splittable: false)

        #expect(menu.sections == [[.membership(leave)]])
    }

    // A split thread's record can lack the thread's state; Open in Full View needs none.
    @Test
    func theSplitsMenuWithoutTheThreadsStateKeepsOpenInFullView() async {
        let account = FormattedAccount(contents: ["test"], thread: true, threadState: false)
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: account,
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(GuildId(rawValue: 1)))
        session.open(channel: FormattedStore.channelIds[0])
        await session.openBeside(thread: FormattedStore.threadId, from: FormattedStore.channelIds[0])
        #expect(session.split?.channelId == FormattedStore.threadId)

        let menu = ThreadMenu.splitView(of: session)

        #expect(menu?.sections == [[.openInFullView]])
        #expect(menu?.location == .sidebarOverflow)
    }

    @Test
    func anArchivedThreadsMembershipIsDisabledAndTheRestIsnt() {
        let archived = SessionModel.MembershipItem(joins: false, enabled: false)
        let menu = ThreadMenu.splitView(membership: archived)

        #expect(menu.sections.flatMap { $0 }.map(\.enabled) == [false, true])
    }
}

// Discord's thread sidebar, read from its web client (canary 645444, 2026-10-21).
@MainActor
struct SplitWidthTests {
    @Test
    func theSplitStartsAt480AndStaysWithin450AndThePageLess315() {
        let split = SplitWidth()

        #expect(split.shown(in: 1200) == CGFloat(480))
        split.width = 300
        #expect(split.shown(in: 1200) == CGFloat(450))
        split.width = 2000
        #expect(split.shown(in: 1200) == CGFloat(885), "the page less 315")
        #expect(!SplitWidth.floats(in: 766))
    }

    // Too narrow for both, it floats over the channel at its minimum, without a handle.
    @Test
    func aPageTooNarrowForBothFloatsTheSplit() {
        let split = SplitWidth()
        split.width = 600

        #expect(SplitWidth.floats(in: 765))
        #expect(split.shown(in: 700) == CGFloat(450))
    }

    // Dragging the gap left widens the split, from its width when the drag began.
    @Test
    func aDragMovesTheSplitsLeftEdgeWithinItsLimits() {
        #expect(SplitWidth.dragged(from: 480, by: -40, in: 1200) == CGFloat(520))
        #expect(SplitWidth.dragged(from: 480, by: 100, in: 1200) == CGFloat(450))
        #expect(SplitWidth.dragged(from: 480, by: -900, in: 1200) == CGFloat(885))
    }

}

@MainActor
struct ResizeCursorTests {
    // A gap that goes away under the pointer gets no hover's end: it pops on its way out.
    @Test
    func theCursorIsPushedOnceAndPoppedOnce() {
        var pushes = 0
        var pops = 0
        let cursor = ResizeCursor(push: { pushes += 1 }, pop: { pops += 1 })

        cursor.hover(true)
        cursor.hover(true)
        #expect((pushes, pops) == (1, 0))
        cursor.release()
        cursor.hover(false)
        cursor.release()
        #expect((pushes, pops) == (1, 1))
        cursor.hover(true)
        cursor.hover(false)
        #expect((pushes, pops) == (2, 2))
    }

    // Past the width's limits the pointer leaves the gap mid-drag; the cursor stays until the
    // drag ends.
    @Test
    func theCursorStaysForTheWholeDrag() {
        var pushes = 0
        var pops = 0
        let cursor = ResizeCursor(push: { pushes += 1 }, pop: { pops += 1 })

        cursor.hover(true)
        cursor.drag(true)
        cursor.hover(false)
        #expect((pushes, pops) == (1, 0))
        cursor.hover(true)
        cursor.drag(false)
        #expect((pushes, pops) == (1, 0), "still over the gap")
        cursor.drag(true)
        cursor.hover(false)
        cursor.drag(false)
        #expect((pushes, pops) == (1, 1))
        cursor.drag(true)
        cursor.release()
        #expect((pushes, pops) == (2, 2), "a gap gone mid-drag pops it")
    }

    // A press, a move and a release as the window gets them; the release waits in the queue
    // when `release` is false.
    func drag(in window: NSWindow, from start: NSPoint, by distance: CGFloat, release: Bool = true)
        throws
    {
        let now = ProcessInfo.processInfo.systemUptime
        let event = { (type: NSEvent.EventType, point: NSPoint, step: Double) in
            NSEvent.mouseEvent(
                with: type, location: point, modifierFlags: [], timestamp: now + step / 100,
                windowNumber: window.windowNumber, context: nil, eventNumber: 0, clickCount: 1,
                pressure: 1)
        }
        let end = NSPoint(x: start.x + distance, y: start.y)
        let down = try #require(event(.leftMouseDown, start, 0))
        for (step, point) in [NSPoint(x: start.x + distance / 2, y: start.y), end].enumerated() {
            let moved = try #require(event(.leftMouseDragged, point, Double(step + 1)))
            NSApp.postEvent(moved, atStart: false)
        }
        if release {
            NSApp.postEvent(try #require(event(.leftMouseUp, end, 3)), atStart: false)
        }
        window.sendEvent(down)
    }

    @Test(.enabled(if: firstClicks))
    func aDragThroughTheGapHoldsTheCursorUntilItsRelease() async throws {
        var calls: [String] = []
        let cursor = ResizeCursor(push: { calls.append("push") }, pop: { calls.append("pop") })
        let split = SplitWidth()
        let host = NSHostingView(
            rootView: SplitGap(split: split, page: 1200, cursor: cursor)
                .frame(width: SplitWidth.gap, height: 100)
                .frame(width: 100, height: 100, alignment: .leading)
                .takingFirstClicks())
        let window = MessageTableTests.KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 100, height: 100), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        host.layoutSubtreeIfNeeded()

        try drag(in: window, from: NSPoint(x: 4, y: 50), by: -40, release: false)
        let deadline = ContinuousClock.now + .seconds(5)
        while split.width != 520, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(split.width == CGFloat(520))
        #expect(cursor.pushed, "held with the pointer off the gap")

        let up = try #require(
            NSEvent.mouseEvent(
                with: .leftMouseUp, location: NSPoint(x: -36, y: 50), modifierFlags: [],
                timestamp: ProcessInfo.processInfo.systemUptime, windowNumber: window.windowNumber,
                context: nil, eventNumber: 0, clickCount: 1, pressure: 1))
        NSApp.postEvent(up, atStart: false)
        while cursor.pushed, ContinuousClock.now < deadline {
            try await Task.sleep(for: .milliseconds(20))
        }
        #expect(calls == ["push", "pop"])
    }

    // Escape, the X, a deleted thread or a narrower page take the gap away under the pointer.
    @Test
    func aGapThatGoesAwayPopsItsCursor() async throws {
        var pops = 0
        let cursor = ResizeCursor(push: {}, pop: { pops += 1 })
        let host = NSHostingView(
            rootView: AnyView(
                SplitGap(split: SplitWidth(), page: 1200, cursor: cursor)
                    .frame(width: SplitWidth.gap, height: 100)))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 100, height: 100), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        host.layoutSubtreeIfNeeded()
        cursor.hover(true)

        host.rootView = AnyView(EmptyView())
        let deadline = ContinuousClock.now + .seconds(5)
        while pops == 0, ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            try await Task.sleep(for: .milliseconds(20))
        }

        #expect(pops == 1)
        #expect(!cursor.pushed)
    }
}

@MainActor
struct SplitHeaderTests {
    let thread = Channel(
        id: ChannelId(rawValue: 31), kind: .publicThread, guildId: GuildId(rawValue: 1),
        parentId: ChannelId(rawValue: 11), name: "t1", position: 0, topic: nil, nsfw: false,
        rateLimitPerUser: 0, recipientIds: [], thread: nil)
    let menu = ThreadMenu.splitView(
        membership: SessionModel.MembershipItem(joins: true, enabled: true))

    func rendered(name: String = "t1", width: CGFloat = 480) throws -> Rendered {
        var named = thread
        named.name = name
        return try Rendered(
            SplitHeader(thread: named, menu: menu)
                .frame(width: width)
                .background(Color(nsColor: Palette.chat)).ignoresSafeArea(),
            size: NSSize(width: width, height: ChannelHeader.height + 1))
    }

    // The columns where something other than the background is drawn, in points.
    func inkColumns(_ image: Rendered, rows: ClosedRange<CGFloat> = 8...40) -> [CGFloat] {
        let background = image.color(1, 24)
        let columns = 0..<image.rep.pixelsWide
        return columns.filter { x in
            stride(from: rows.lowerBound, through: rows.upperBound, by: 0.5).contains { y in
                !same(image.pixel(x, Int(y * image.scale)), background)
            }
        }.map { CGFloat($0) / image.scale }
    }

    // The ink of everything drawn between `xs`, as one range.
    func ink(in xs: ClosedRange<CGFloat>, _ image: Rendered) -> ClosedRange<CGFloat>? {
        let columns = inkColumns(image).filter { xs.contains($0) }
        guard let first = columns.first, let last = columns.last else {
            return nil
        }
        return first...last
    }

    // 16 padding, the 20 icon at -1 and 9 before the name; 32 buttons 8 apart, 8 from the edge:
    // the X's box is 440...472, the "…"'s 400...432.
    @Test
    func theHeaderMatchesDiscordsMeasures() throws {
        let image = try rendered()

        let icon = try #require(ink(in: 0...40, image))
        #expect(icon.lowerBound >= 15 && icon.upperBound <= 35, "icon \(icon)")
        #expect(abs(icon.lowerBound + icon.upperBound - 50) <= 2, "centered in 15...35: \(icon)")
        let name = try #require(ink(in: 40...399, image))
        #expect(name.lowerBound >= 44 && name.lowerBound <= 46, "name \(name)")
        let more = try #require(ink(in: 400...432, image))
        #expect(abs((more.lowerBound + more.upperBound) / 2 - 416) <= 1, "… \(more)")
        let close = try #require(ink(in: 440...472, image))
        #expect(abs((close.lowerBound + close.upperBound) / 2 - 456) <= 1, "X \(close)")
        #expect(ink(in: 472.5...480, image) == nil, "8 from the edge")
    }

    // A token as the renderer draws it: tokens come out shifted by the bitmap's color space.
    func drawn(_ token: NSColor) throws -> NSColor? {
        try Rendered(Color(nsColor: token).ignoresSafeArea(), size: NSSize(width: 10, height: 10))
            .color(5, 5)
    }

    @Test
    func theHeaderDrawsDiscordsColors() throws {
        let image = try rendered()
        let ink = try [0...40, 40...399, 400...432, 440...472].map {
            try #require(self.ink(in: $0, image))
        }
        func brightest(in range: ClosedRange<CGFloat>) -> NSColor? {
            var best: NSColor?
            for x in stride(from: range.lowerBound, through: range.upperBound, by: 0.5) {
                for y in stride(from: CGFloat(8), through: 40, by: 0.5) {
                    if let color = image.color(x, y),
                        color.brightnessComponent > (best?.brightnessComponent ?? -1)
                    {
                        best = color
                    }
                }
            }
            return best
        }

        #expect(same(brightest(in: ink[0]), try drawn(Palette.channelDefault)))
        #expect(same(brightest(in: ink[1]), try drawn(Palette.textStrong)))
        let muted = try drawn(Palette.textMuted)
        #expect(same(brightest(in: ink[2]), muted), "\(String(describing: brightest(in: ink[2]))) \(String(describing: muted))")
        #expect(same(brightest(in: ink[3]), try drawn(Palette.textMuted)))
        // The line under the header, over its background.
        let line = image.color(240, ChannelHeader.height + 0.25)
        let above = image.color(240, ChannelHeader.height - 2)
        #expect(!same(line, above), "\(String(describing: line))")
    }

    // No ellipsis: a long name is cut, fading out over its last 8.
    @Test
    func aLongNameFadesInsteadOfAnEllipsis() throws {
        let long = String(repeating: "thread name ", count: 12)
        let image = try rendered(name: long)
        let name = try #require(ink(in: 40...399, image))

        // The name's area ends 8 + 8 before the "…" button's box (480 − 8 − 32 − 8 − 32 − 16),
        // its last 8 faded out.
        let areaEnd: CGFloat = 384
        #expect(name.upperBound <= areaEnd + 0.5, "\(name)")
        #expect(name.upperBound >= areaEnd - 8, "cut at the area's end, not before: \(name)")
        func brightness(_ xs: ClosedRange<CGFloat>) -> CGFloat {
            var best: CGFloat = 0
            for x in stride(from: xs.lowerBound, through: xs.upperBound, by: 0.5) {
                for y in stride(from: CGFloat(14), through: 34, by: 0.5) {
                    best = max(best, image.color(x, y)?.brightnessComponent ?? 0)
                }
            }
            return best
        }
        #expect(brightness(areaEnd - 2...areaEnd - 0.5) < brightness(60...100) * 0.5)
    }
}

extension SessionScreenTests {
    // Channel 1001 open with thread 1003 beside it.
    func splitSession(_ account: FormattedAccount) async -> SessionModel {
        let session = formattedSession(account)
        await session.openBeside(thread: FormattedStore.threadId, from: FormattedStore.channelIds[0])
        return session
    }

    func tables(in host: NSView) -> [MessageTableView] {
        views(MessageTableView.self, in: host).sorted {
            $0.convert($0.bounds, to: nil).minX < $1.convert($1.bounds, to: nil).minX
        }
    }

    // The box and the line open the thread beside the channel, which keeps the left side.
    @Test
    func theSplitShowsTheThreadBesideTheChannel() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }

        try await waitUntil(host) { tables(in: host).count == 2 }
        let shown = tables(in: host)
        try #require(shown.count == 2)
        let (left, right) = (shown[0], shown[1])
        #expect((left.delegate as? MessageTableController)?.state.channelId == FormattedStore.channelIds[0])
        #expect((right.delegate as? MessageTableController)?.state.channelId == FormattedStore.threadId)
        #expect(views(ComposerTextView.self, in: host).count == 2)

        session.closeSplit()
        try await waitUntil(host) { tables(in: host).count == 1 }
    }

    // Q37: Escape in either pane marks both read and jumps both to the bottom.
    @Test(arguments: [0, 1])
    func escapeInEitherPaneMarksBothAndJumpsBoth(pane: Int) async throws {
        let account = FormattedAccount(contents: ["test"], thread: true, unread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        try await waitUntil(host) {
            tables(in: host).count == 2 && tables(in: host).allSatisfy { $0.numberOfRows > 20 }
        }
        for table in tables(in: host) {
            let clip = try #require(table.enclosingScrollView?.contentView)
            clip.scroll(to: NSPoint(x: 0, y: 0))
            table.enclosingScrollView?.reflectScrolledClipView(clip)
        }
        let composers = views(ComposerTextView.self, in: host).sorted {
            $0.convert($0.bounds, to: nil).minX < $1.convert($1.bounds, to: nil).minX
        }
        try #require(composers.count == 2)
        let composer = composers[pane]

        let escape = try #require(escapeKey())
        composer.currentEvent = { escape }
        composer.doCommand(by: #selector(NSResponder.cancelOperation(_:)))

        try await waitUntil(host) { account.markedRead.withLock { $0 }.count == 2 }
        #expect(
            Set(account.markedRead.withLock { $0 })
                == [FormattedStore.channelIds[0], FormattedStore.threadId])
        try await waitUntil(host) {
            tables(in: host).allSatisfy { table in
                guard let scroll = table.enclosingScrollView else {
                    return false
                }
                let bottom = scroll.contentView.bounds.maxY - scroll.contentInsets.bottom
                return abs(table.bounds.maxY - bottom) <= 2
            }
        }
    }

    func wideWindow(_ host: NSView, width: CGFloat) -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: width, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        return window
    }

    func frame(of view: NSView) -> NSRect {
        view.convert(view.bounds, to: nil)
    }

    // Docked: the channel, its 1 right border, an 8 gap of the frame color, then the split,
    // 480 wide by default.
    @Test
    func aWidePageDocksTheSplitBesideTheChannel() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = wideWindow(host, width: 1440)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { tables(in: host).count == 2 }
        let shown = tables(in: host)
        try #require(shown.count == 2)

        let left = frame(of: try #require(shown[0].enclosingScrollView))
        let right = frame(of: try #require(shown[1].enclosingScrollView))
        #expect(abs(right.maxX - 1440) <= 1.5, "\(right)")
        #expect(abs(right.width - (480 - 2)) <= 0.5, "inside the split's borders: \(right)")
        // The channel's border is its card's last point, the split's its first.
        #expect(abs(right.minX - left.maxX - (8 + 1)) <= 0.5, "\(left) \(right)")
        host.layoutSubtreeIfNeeded()
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let color = { (x: CGFloat, y: CGFloat) in
            rep.colorAt(x: Int(x * scale), y: Int(y * scale))?.usingColorSpace(.sRGB)
        }
        let y = host.bounds.height - right.midY
        let frameColor = try Rendered(
            Color(nsColor: Palette.frame).ignoresSafeArea(), size: NSSize(width: 10, height: 10)
        ).color(5, 5)
        for x in stride(from: left.maxX + 0.25, through: right.minX - 1.25, by: 0.5) {
            #expect(same(color(x, y), frameColor), "the gap at \(x)")
        }
        #expect(!same(color(left.maxX - 0.25, y), frameColor), "the channel's border")
        #expect(!same(color(right.minX - 0.75, y), frameColor), "the split's border")
    }

    // Too narrow for both (the page at most 765), the split floats over the channel at 450.
    @Test
    func aNarrowPageFloatsTheSplitOverTheChannel() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = wideWindow(host, width: 1100)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { tables(in: host).count == 2 }
        let shown = tables(in: host)
        try #require(shown.count == 2)

        let left = frame(of: try #require(shown[0].enclosingScrollView))
        let right = frame(of: try #require(shown[1].enclosingScrollView))
        #expect(abs(left.maxX - 1100) <= 1, "the channel keeps its full width: \(left)")
        #expect(abs(right.width - (450 - 2)) <= 0.5, "\(right)")
        #expect(abs(right.maxX - 1100) <= 1.5, "\(right)")
    }

    // Opening from a box focuses the split's composer; a split that comes back with its
    // channel leaves the focus to the channel's.
    @Test
    func theSplitsComposerTakesTheFocusOnlyWhenTheUserOpensIt() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        // A test's window never becomes key on its own.
        let window = MessageTableTests.KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1440, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        try await waitUntil(host) { views(ComposerTextView.self, in: host).count == 1 }
        let focused = { () -> ChannelId? in
            guard let responder = window.firstResponder as? ComposerTextView else {
                return nil
            }
            let composers = views(ComposerTextView.self, in: host).sorted {
                frame(of: $0).minX < frame(of: $1).minX
            }
            return composers.firstIndex(of: responder).map {
                $0 == 0 ? FormattedStore.channelIds[0] : FormattedStore.threadId
            }
        }

        await session.openBeside(thread: FormattedStore.threadId, from: FormattedStore.channelIds[0])
        try await waitUntil(host) { focused() == FormattedStore.threadId }
        #expect(!session.focusSplit, "the request is consumed")

        session.open(channel: FormattedStore.channelIds[1])
        try await waitUntil(host) { views(ComposerTextView.self, in: host).count == 1 }
        session.go(to: FormattedStore.channelIds[0])
        try await waitUntil(host) {
            views(ComposerTextView.self, in: host).count == 2
                && focused() == FormattedStore.channelIds[0]
        }
        #expect(focused() == FormattedStore.channelIds[0])
    }

    func keyWindow(_ host: NSView, width: CGFloat) -> NSWindow {
        // A test's window never becomes key on its own.
        let window = MessageTableTests.KeyWindow(
            contentRect: NSRect(x: 0, y: 0, width: width, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.isReleasedWhenClosed = false
        window.contentView = host
        window.orderFrontRegardless()
        return window
    }

    // Kept until Akari quits: a drag through the gap stores nothing, and each launch starts at
    // the default. Its own width: the other screen tests render with the shared one meanwhile.
    @Test(.enabled(if: firstClicks))
    func aDraggedWidthIsNeverStored() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let split = SplitWidth()
        let host = NSHostingView(
            rootView: SessionScreen(session: session, splitWidth: split).takingFirstClicks())
        let window = keyWindow(host, width: 1440)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { tables(in: host).count == 2 }
        let stored = { UserDefaults.standard.dictionaryRepresentation() as NSDictionary }
        let before = stored()

        // Two widths: a stored value can't match both.
        for width in [CGFloat(520), 560] {
            let channel = frame(of: try #require(tables(in: host).first?.enclosingScrollView))
            let gap = NSPoint(x: channel.maxX + SplitWidth.gap / 2, y: channel.midY)
            try ResizeCursorTests().drag(in: window, from: gap, by: -40)
            try await waitUntil(host) { split.width == width }
            #expect(split.width == width)
            #expect(stored() == before)
        }
        #expect(SplitWidth().width == SplitWidth.standard)
    }

    func composers(in host: NSView) -> [ComposerTextView] {
        views(ComposerTextView.self, in: host).sorted { frame(of: $0).minX < frame(of: $1).minX }
    }

    // With nothing to mark or load, Escape closes the split, as the official client's does, and
    // the channel's composer takes the focus.
    @Test
    func escapeWithNothingToDoClosesTheSplit() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = keyWindow(host, width: 1440)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { composers(in: host).count == 2 }
        let shown = composers(in: host)
        try #require(shown.count == 2)
        let (channel, split) = (shown[0], shown[1])

        let escape = try #require(escapeKey())
        split.currentEvent = { escape }
        split.doCommand(by: #selector(NSResponder.cancelOperation(_:)))

        try await waitUntil(host) { tables(in: host).count == 1 }
        #expect(session.split == nil)
        #expect(account.markedRead.withLock { $0 }.isEmpty)
        try await waitUntil(host) { window.firstResponder === channel }
    }

    // The channel's list stays the same table at the same place while the split opens, closes
    // or moves between docked and floating; so does the split's.
    @Test
    func theListsKeepTheirTablesAndPlaces() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = wideWindow(host, width: 1440)
        defer { window.orderOut(nil) }
        try await waitUntil(host) {
            tables(in: host).count == 1 && tables(in: host)[0].numberOfRows > 20
        }
        let channel = try #require(tables(in: host).first)
        let clip = try #require(channel.enclosingScrollView?.contentView)
        clip.scroll(to: NSPoint(x: 0, y: 200))
        channel.enclosingScrollView?.reflectScrolledClipView(clip)
        let place = clip.bounds.origin.y

        await session.openBeside(thread: FormattedStore.threadId, from: FormattedStore.channelIds[0])
        try await waitUntil(host) { tables(in: host).count == 2 }
        #expect(tables(in: host).first === channel)
        #expect(abs(clip.bounds.origin.y - place) <= 1, "\(clip.bounds.origin.y) \(place)")
        let split = try #require(tables(in: host).last)

        window.setContentSize(NSSize(width: 1100, height: 600))
        try await waitUntil(host) { frame(of: split).width < 460 }
        #expect(tables(in: host).last === split, "floating keeps the split's table")
        #expect(tables(in: host).first === channel)

        session.closeSplit()
        try await waitUntil(host) { tables(in: host).count == 1 }
        #expect(tables(in: host).first === channel)
        #expect(abs(clip.bounds.origin.y - place) <= 1, "\(clip.bounds.origin.y) \(place)")
    }

    // The y of the line under the header at `x`, from the window's top: the first row below the
    // header's middle that isn't the header's background.
    func headerLine(at x: CGFloat, in host: NSView) throws -> (y: CGFloat, color: NSColor?) {
        host.layoutSubtreeIfNeeded()
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let color = { (y: CGFloat) in
            rep.colorAt(x: Int(x * scale), y: Int(y * scale))?.usingColorSpace(.sRGB)
        }
        let middle = TitleBar.height + ChannelHeader.height / 2
        let background = color(middle)
        let y = try #require(
            stride(from: middle, through: middle + 40, by: 0.5).first {
                !same(color($0), background)
            })
        return (y, color(y))
    }

    // One line under both headers, level, whether the split is docked or floats.
    @Test(arguments: [1440.0, 1100.0])
    func theHeadersShareOneLine(width: Double) async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = wideWindow(host, width: width)
        window.appearance = NSAppearance(named: .darkAqua)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { tables(in: host).count == 2 }
        let shown = tables(in: host)
        try #require(shown.count == 2)

        // Where both headers draw nothing above their lines: the channel's right end, and the
        // split's middle (a floating split covers the channel's right end).
        let channelX = min(frame(of: shown[0]).maxX, frame(of: shown[1]).minX) - 40
        let channel = try headerLine(at: channelX, in: host)
        let split = try headerLine(at: frame(of: shown[1]).midX, in: host)

        #expect(channel.y == split.y, "\(channel.y) \(split.y)")
        #expect(same(channel.color, split.color), "one line, not two")
    }
}
