import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct SessionScreenTests {
    func session(_ account: GuildAccount, defaults: UserDefaults) -> SessionModel {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: account,
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.open(.guild(GuildId(rawValue: 1)))
        return session
    }

    func waitForLoad(_ account: GuildAccount) async throws {
        for _ in 0..<40 where account.loads.withLock({ $0 }) == 0 {
            try await Task.sleep(for: .milliseconds(50))
        }
    }

    // A reconnect replaces the session with one for the same channel.
    @Test
    func aNewSessionForTheSameChannelLoadsIt() async throws {
        let defaults = MemoryDefaults()
        let first = GuildAccount()
        let second = GuildAccount()
        let host = NSHostingView(
            rootView: AnyView(SessionScreen(session: session(first, defaults: defaults))))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        try await waitForLoad(first)
        #expect(first.loads.withLock { $0 } == 1)

        host.rootView = AnyView(SessionScreen(session: session(second, defaults: defaults)))
        try await waitForLoad(second)

        #expect(second.loads.withLock { $0 } == 1)
    }

    // SwiftUI applies a change over a few run loop turns; at most 5 s.
    func waitUntil(_ host: NSView, _ condition: () throws -> Bool) async throws {
        let deadline = ContinuousClock.now + .seconds(5)
        while ContinuousClock.now < deadline {
            host.layoutSubtreeIfNeeded()
            if try condition() {
                return
            }
            try await Task.sleep(for: .milliseconds(20))
        }
        Issue.record("not within 5 s")
    }

    // SwiftUI applies a channel switch over a few run loop turns.
    func settle(_ host: NSView) async {
        for _ in 0..<6 {
            try? await Task.sleep(for: .milliseconds(100))
            host.layoutSubtreeIfNeeded()
        }
    }

    func formattedSession(_ account: FormattedAccount) -> SessionModel {
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: account,
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(.guild(GuildId(rawValue: 1)))
        session.open(channel: FormattedStore.channelIds[0])
        return session
    }

    func shownWindow(_ host: NSView) -> NSWindow {
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        return window
    }

    // How far the newest row reaches below what the list shows above its bottom inset.
    func newestRowHidden(in host: NSView) throws -> CGFloat {
        let table = try #require(views(MessageTableView.self, in: host).first)
        let scroll = try #require(table.enclosingScrollView)
        let newest = table.rect(ofRow: table.numberOfRows - 1)
        return newest.maxY - (scroll.contentView.bounds.maxY - scroll.contentInsets.bottom)
    }

    @Test(arguments: [false, true])
    func returningToAChannelLeftAtTheBottomShowsItsNewestMessage(arrivals: Bool) async throws {
        let account = FormattedAccount(contents: FormattedSamples.contents)
        let session = formattedSession(account)
        let host = NSHostingView(rootView: SessionScreen(session: session))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        await settle(host)
        #expect(try newestRowHidden(in: host) <= 0.5)

        session.open(channel: FormattedStore.channelIds[1])
        await settle(host)
        if arrivals {
            account.formattedStore.add(3, to: FormattedStore.channelIds[0])
        }
        session.open(channel: FormattedStore.channelIds[0])
        await settle(host)

        #expect(try newestRowHidden(in: host) <= 0.5)
    }

    // The composer's box is its field's scroll view, in window coordinates.
    func composerBox(in host: NSView) throws -> NSRect {
        let field = try #require(views(ComposerTextView.self, in: host).first?.enclosingScrollView)
        return field.convert(field.bounds, to: nil)
    }

    // As in the official client: messages scroll on under the strip above the composer, and its
    // texts stay readable on a backdrop.
    @Test
    func theListRunsBehindTheStatusStripDownToTheComposer() async throws {
        let account = FormattedAccount(
            contents: ["test"], formattedEvery: 1000,
            slowmode: Slowmode(interval: 30, exempt: false, until: nil), mentioned: true)
        let host = NSHostingView(rootView: SessionScreen(session: formattedSession(account)))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        await settle(host)
        let table = try #require(views(MessageTableView.self, in: host).first)
        let scroll = try #require(table.enclosingScrollView)
        let box = try composerBox(in: host)
        let list = scroll.convert(scroll.bounds, to: nil)
        let stripTop = box.maxY + ComposerStatus.height

        #expect(abs(list.minY - box.maxY) <= 0.5, "list \(list), composer \(box)")
        let newest = table.convert(table.rect(ofRow: table.numberOfRows - 1), to: nil)
        #expect(newest.minY >= stripTop - 0.5, "newest \(newest), strip top \(stripTop)")

        // The newest row, tinted for its mention, moves down behind the strip.
        let clip = scroll.contentView
        clip.scroll(to: NSPoint(x: 0, y: clip.bounds.minY - 32))
        scroll.reflectScrolledClipView(clip)
        host.layoutSubtreeIfNeeded()
        let rep = try #require(host.bitmapImageRepForCachingDisplay(in: host.bounds))
        host.cacheDisplay(in: host.bounds, to: rep)
        let scale = CGFloat(rep.pixelsWide) / host.bounds.width
        let color = { (x: CGFloat, y: CGFloat) in
            rep.colorAt(x: Int(x * scale), y: Int((host.bounds.height - y) * scale))?
                .usingColorSpace(.sRGB)
        }
        let stripMiddle = box.maxY + ComposerStatus.height / 2
        let inStrip = color(list.midX, stripMiddle)
        let inRow = color(list.midX, stripTop + 4)
        let chat = color(list.minX + 3, box.midY)

        #expect(same(inStrip, inRow), "\(String(describing: inStrip)) \(String(describing: inRow))")
        #expect(!same(inStrip, chat))
        // Inside the slowmode text's backdrop, right of its last glyph.
        #expect(same(color(list.maxX - 14, stripMiddle), chat))
    }

    @Test
    func anEndedSessionEndsItsLinkConfirmation() async throws {
        let recorder = LinkTests.Recorder()
        let follower = recorder.follower(
            TrustedLinks(account: UserId(rawValue: 1), defaults: MemoryDefaults()))
        let account = FormattedAccount(contents: ["test"])
        let host = NSHostingView(
            rootView: AnyView(SessionScreen(session: formattedSession(account), links: follower)))
        let window = shownWindow(host)
        defer { window.orderOut(nil) }
        try await waitUntil(host) { !views(MessageTableView.self, in: host).isEmpty }
        follower.follow(try #require(URL(string: "https://example.com/a")), masked: true)

        host.rootView = AnyView(Color.clear)
        try await waitUntil(host) { recorder.dismissed > 0 }

        #expect(recorder.dismissed == 1)
    }

    func views<View: NSView>(_ type: View.Type, in view: NSView) -> [View] {
        view.subviews.flatMap { subview in
            [subview as? View].compactMap { $0 } + views(type, in: subview)
        }
    }

    @Test
    func anUnloadedChannelShowsPlaceholdersAndNoSpinner() async throws {
        let defaults = MemoryDefaults()
        let account = GuildAccount(holdsLoads: true)
        let host = NSHostingView(
            rootView: SessionScreen(session: session(account, defaults: defaults)))
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600), styleMask: [.titled],
            backing: .buffered, defer: false)
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        try await waitForLoad(account)
        try await Task.sleep(for: .milliseconds(100))
        host.layoutSubtreeIfNeeded()

        #expect(!views(PlaceholderCell.self, in: host).isEmpty)
        #expect(views(NSProgressIndicator.self, in: host).isEmpty)
    }
}

@MainActor
struct ComposerAlignmentTests {
    // Bottom and top edges in pixels, found by scanning a column for color changes.
    func edges(_ image: Rendered, x: CGFloat) throws -> (top: Int, bottom: Int) {
        let height = CGFloat(image.rep.pixelsHigh) / image.scale
        let below = try #require(image.firstChange(at: x, above: height - 1))
        let top = try #require(image.firstChange(at: x, above: CGFloat(below - 3) / image.scale))
        return (top, below)
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theComposerLinesUpWithTheUserPanel(appearance: NSAppearance.Name) throws {
        let defaults = MemoryDefaults()
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: GuildAccount(),
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.open(.guild(GuildId(rawValue: 1)))
        let image = try Rendered(SessionScreen(session: session), appearance: appearance)

        let panel = try edges(image, x: 40)
        let composer = try edges(image, x: image.width - 60)

        #expect(abs(panel.bottom - composer.bottom) <= 1, "panel \(panel), composer \(composer)")
        #expect(
            abs((panel.bottom - panel.top) - (composer.bottom - composer.top)) <= 2,
            "panel \(panel), composer \(composer)")
    }
}
