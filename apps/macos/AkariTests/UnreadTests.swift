import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

private func isRed(_ color: NSColor?) -> Bool {
    guard let color else {
        return false
    }
    return color.redComponent > 0.6 && color.greenComponent < 0.35 && color.blueComponent < 0.35
}

private func isBright(_ color: NSColor?) -> Bool {
    guard let color else {
        return false
    }
    return min(color.redComponent, color.greenComponent, color.blueComponent) > 0.85
}

private func count(_ rendered: Rendered, in xs: Range<CGFloat>, where test: (NSColor?) -> Bool)
    -> Int
{
    let columns = Int(xs.lowerBound * rendered.scale)..<Int(xs.upperBound * rendered.scale)
    return columns.reduce(0) { total, x in
        total + (0..<rendered.rep.pixelsHigh).filter { test(rendered.pixel(x, $0)) }.count
    }
}

@MainActor
struct MentionBadgeTests {
    @Test
    func theBadgeWidthFollowsTheCount() {
        #expect([1, 9, 10, 99, 100, 5_000].map(MentionBadge.width) == [16, 16, 22, 22, 30, 30])
        #expect(
            [7, 999, 1_000, 4_321, 9_999, 25_000].map(MentionBadge.text)
                == ["7", "999", "1k+", "4k+", "9k+", "9k+"])
    }
}

@MainActor
struct RailUnreadTests {
    @Test
    func anUnreadServerShowsTheShortPill() {
        #expect(RailPill.height(selected: false, hovered: false, unread: true) == 8)
        #expect(RailPill.height(selected: false, hovered: false, unread: false) == 0)
    }

    @Test
    func hoverAndSelectionWinOverTheUnreadPill() {
        #expect(RailPill.height(selected: false, hovered: true, unread: true) == 20)
        #expect(RailPill.height(selected: true, hovered: false, unread: true) == 40)
        #expect(RailPill.height(selected: true, hovered: true, unread: false) == 40)
    }

    // A server's rail item on the rail's color: the pill at the left edge, the badge at the
    // icon's bottom right.
    func render(_ badge: Badge) throws -> Rendered {
        try Rendered(
            RailItem(selected: false, help: "", badge: badge) { highlighted in
                GuildIcon(name: "Lab", highlighted: highlighted)
            } action: {}
            .frame(width: SidebarWidth.rail, height: 56)
            .background(Color(nsColor: Palette.frame)),
            size: NSSize(width: SidebarWidth.rail, height: 56))
    }

    @Test
    func aMutedServerShowsNoPillButItsBadge() throws {
        let muted = try render(Badge(unread: false, mentions: 3))
        let unread = try render(Badge(unread: true, mentions: 0))

        #expect(count(muted, in: 0..<4, where: isBright) == 0, "no pill")
        #expect(count(unread, in: 0..<4, where: isBright) > 0, "the unread pill")
        #expect(count(muted, in: 36..<SidebarWidth.rail, where: isRed) > 0, "the badge")
        #expect(count(unread, in: 36..<SidebarWidth.rail, where: isRed) == 0)
    }

    @Test
    func unreadDirectMessagesSitUnderHome() throws {
        let unread = [CrowdedStore.dmIds[3], CrowdedStore.dmIds[1]]
        let session = SessionModel(
            userId: UserId(rawValue: 1), account: CrowdedAccount(unread: unread),
            memory: AccountMemory(defaults: MemoryDefaults())
        ) { _ in }
        session.start()
        session.open(channel: CrowdedStore.dmIds[0])
        #expect(session.directMessages.unread.map(\.id) == unread)
        let rail = try Rendered(
            ServerRail(session: session)
                .frame(width: SidebarWidth.rail, height: 400)
                .background(Color(nsColor: Palette.frame)),
            size: NSSize(width: SidebarWidth.rail, height: 400))

        // Down the middle, the items are runs of pixels off the rail's color: Home (selected,
        // brand) and each unread DM's avatar are in color, the separator below them isn't.
        let middle = Int(SidebarWidth.rail / 2 * rail.scale)
        let background = rail.color(SidebarWidth.rail - 2, 1)
        var items: [(start: CGFloat, colored: Bool)] = []
        var inside = false
        for y in 0..<rail.rep.pixelsHigh {
            let pixel = rail.pixel(middle, y)
            let item = !same(pixel, background)
            if item, !inside {
                items.append((CGFloat(y) / rail.scale, false))
            }
            if item, let pixel,
                max(pixel.redComponent, pixel.greenComponent, pixel.blueComponent)
                    - min(pixel.redComponent, pixel.greenComponent, pixel.blueComponent) > 0.25
            {
                items[items.count - 1].colored = true
            }
            inside = item
        }
        try #require(items.count >= 4, "\(items)")
        #expect(items.prefix(4).map(\.colored) == [true, true, true, false], "\(items)")
        // One 40 tall item after another, 8 apart, wherever the window's safe area puts Home.
        for pair in zip(items.prefix(3), items.dropFirst().prefix(2)) {
            #expect(abs(pair.1.start - pair.0.start - 48) <= 1, "\(items)")
        }
    }
}

@MainActor
struct UnreadRowTests {
    let general = Channel(
        id: ChannelId(rawValue: 11), kind: .guildText, guildId: GuildId(rawValue: 1),
        parentId: nil, name: "general", position: 0, topic: nil, nsfw: false,
        rateLimitPerUser: 0, recipientIds: [])

    func render<Row: View>(_ row: Row) throws -> Rendered {
        try Rendered(
            row.frame(width: 240, height: 40).background(Color(nsColor: Palette.frame)),
            size: NSSize(width: 240, height: 40))
    }

    func row(_ badge: Badge) throws -> Rendered {
        try render(
            ChannelRow(channel: general, selected: false, muted: false, badge: badge) {})
    }

    @Test
    func anUnreadChannelRowIsBright() throws {
        let read = try row(Badge(unread: false, mentions: 0))
        let unread = try row(Badge(unread: true, mentions: 0))

        #expect(count(read, in: 16..<200, where: isBright) == 0, "the name is dim")
        #expect(count(unread, in: 16..<200, where: isBright) > 0, "the name is bright")
        #expect(count(read, in: 0..<4, where: isBright) == 0)
        #expect(count(unread, in: 0..<4, where: isBright) > 0, "the pill")
    }

    @Test
    func aChannelRowShowsItsMentionCount() throws {
        let plain = try row(Badge(unread: true, mentions: 0))
        let mentioned = try row(Badge(unread: true, mentions: 2))

        #expect(count(plain, in: 180..<240, where: isRed) == 0)
        #expect(count(mentioned, in: 180..<240, where: isRed) > 0)
    }
}
