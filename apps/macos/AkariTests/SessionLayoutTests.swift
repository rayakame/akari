import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct SessionLayoutTests {
    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func bothHeadersEndOnOneContinuousLine(appearance: NSAppearance.Name) throws {
        let suite = "app.akari.tests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer { defaults.removePersistentDomain(forName: suite) }
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: GuildAccount(),
            memory: AccountMemory(defaults: defaults)
        ) { _ in }
        session.open(.guild(GuildId(rawValue: 1)))
        #expect(session.messages?.channelId == ChannelId(rawValue: 11))
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

        // Up from inside the selected channel row and the red message area to where they start.
        let listContent = try #require(image.firstChange(at: listX, above: 32 + 64)) + 1
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
}
