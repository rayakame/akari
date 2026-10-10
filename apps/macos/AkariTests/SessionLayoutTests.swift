import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct SessionLayoutTests {
    @Test(arguments: [false, true], [NSAppearance.Name.darkAqua, .aqua])
    func bothHeadersEndOnOneContinuousLine(atHome: Bool, appearance: NSAppearance.Name) throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.start()
        if atHome {
            session.open(channel: CrowdedStore.dmIds[0])
        } else {
            session.open(.guild(CrowdedStore.guildIds[0]))
        }
        #expect(session.messages != nil)
        let image = try Rendered(
            SessionView(session: session) { _, _ in Color.red }, appearance: appearance)
        let sidebar = SidebarWidth.stored(in: .standard)
        let listX = sidebar - 30
        let pageX = image.width - 20

        // Down from inside both headers, right of their text, to the line.
        let headerMiddle: CGFloat = 32 + 24
        let list = try #require(image.firstChange(at: listX, below: headerMiddle))
        let page = try #require(image.firstChange(at: pageX, below: headerMiddle))
        #expect(list == page, "the line is at \(list) px over the list, at \(page) over the page")

        // The list's header has a title, left of where the line was found.
        let title = (Int((SidebarWidth.rail + 17) * image.scale)..<Int(listX * image.scale))
            .filter { x in
                !same(
                    image.pixel(x, Int(headerMiddle * image.scale)),
                    image.pixel(Int(listX * image.scale), Int(headerMiddle * image.scale)))
            }
        #expect(!title.isEmpty, "the list's header shows no title")

        // Up from just under both headers to where they end.
        let listContent = try #require(image.firstChange(at: listX, above: 32 + 52)) + 1
        let pageContent = try #require(image.firstChange(at: pageX, above: 300)) + 1
        #expect(
            listContent == pageContent,
            "the server header ends at \(listContent) px, the channel header at \(pageContent)")

        let lineStart = Int((SidebarWidth.rail + 2) * image.scale)
        let gaps = (lineStart..<image.rep.pixelsWide - 1).filter { x in
            same(image.pixel(x, list), image.pixel(x, list - Int(2 * image.scale)))
        }
        #expect(gaps.isEmpty, "the line is missing at \(gaps.count) px, from \(gaps.first ?? 0)")
    }

    func scrollViews(in view: NSView) -> [NSScrollView] {
        view.subviews.flatMap { subview in
            [subview as? NSScrollView].compactMap { $0 } + scrollViews(in: subview)
        }
    }

    // Rows show only through a scroll view's clip view, so where the clip view shows its
    // document is where rows can be seen.
    @Test(arguments: [false, true])
    func nothingScrollsUnderTheTitleStrip(atHome: Bool) async throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(),
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.start()
        if atHome {
            session.open(channel: CrowdedStore.dmIds[0])
        } else {
            session.open(.guild(CrowdedStore.guildIds[0]))
        }
        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 1100, height: 600),
            styleMask: [.titled, .fullSizeContentView], backing: .buffered, defer: false)
        window.titlebarAppearsTransparent = true
        let host = NSHostingView(rootView: SessionScreen(session: session))
        window.contentView = host
        window.orderFrontRegardless()
        defer { window.orderOut(nil) }
        for _ in 0..<40 where scrollViews(in: host).count < 4 {
            try await Task.sleep(for: .milliseconds(50))
        }
        try await Task.sleep(for: .milliseconds(200))
        host.layoutSubtreeIfNeeded()
        let strip = NSRect(x: 0, y: host.bounds.height - 32, width: host.bounds.width, height: 32)

        // The rail, the channel or DM list, the message list and the composer.
        let scrolls = scrollViews(in: host)
        #expect(scrolls.count == 4)
        var scrolled = 0
        for scroll in scrolls {
            let clip = scroll.contentView
            let document = try #require(scroll.documentView)
            if document.bounds.height > clip.bounds.height {
                scrolled += 1
            }
            let end =
                document.isFlipped
                ? document.bounds.maxY - clip.bounds.height + scroll.contentInsets.bottom
                : -scroll.contentInsets.bottom
            clip.scroll(to: NSPoint(x: clip.bounds.minX, y: end))
            scroll.reflectScrolledClipView(clip)
            host.layoutSubtreeIfNeeded()

            let shown = clip.convert(clip.bounds, to: nil)
                .intersection(document.convert(document.bounds, to: nil))
            let frame = scroll.convert(scroll.bounds, to: nil)
            #expect(
                shown.maxY <= strip.minY + 0.5,
                "\(type(of: scroll)) at x \(frame.minX)–\(frame.maxX) shows rows up to \(host.bounds.height - shown.maxY) pt from the top"
            )
        }
        #expect(scrolled >= 3, "the rail and both lists have more rows than fit")
    }
}
