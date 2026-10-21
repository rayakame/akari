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
        let menu = ThreadMenu.fullView(membership: leave)

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

    // Kept until Akari quits; each launch starts at the default and nothing is stored.
    @Test
    func theWidthIsTheAppsAndNeverStored() {
        let before = UserDefaults.standard.dictionaryRepresentation().keys.filter {
            $0.lowercased().contains("split")
        }

        SplitWidth.shared.width = 500
        defer { SplitWidth.shared.width = SplitWidth.standard }

        #expect(SplitWidth().width == CGFloat(480))
        let after = UserDefaults.standard.dictionaryRepresentation().keys.filter {
            $0.lowercased().contains("split")
        }
        #expect(after == before)
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

    // With nothing to mark or load, Escape closes the split, as the official client's does.
    @Test
    func escapeWithNothingToDoClosesTheSplit() async throws {
        let account = FormattedAccount(contents: ["test"], thread: true)
        let session = await splitSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { views(ComposerTextView.self, in: host).count == 2 }
        let composer = try #require(views(ComposerTextView.self, in: host).first)

        let escape = try #require(escapeKey())
        composer.currentEvent = { escape }
        composer.doCommand(by: #selector(NSResponder.cancelOperation(_:)))

        try await waitUntil(host) { tables(in: host).count == 1 }
        #expect(session.split == nil)
        #expect(account.markedRead.withLock { $0 }.isEmpty)
    }
}
