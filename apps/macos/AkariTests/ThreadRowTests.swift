import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct ThreadRowTests {
    let thread = Channel(
        id: ChannelId(rawValue: 31), kind: .publicThread, guildId: GuildId(rawValue: 1),
        parentId: ChannelId(rawValue: 11), name: "release checklist", position: 0, topic: nil,
        nsfw: false, rateLimitPerUser: 0, recipientIds: [],
        thread: ThreadInfo(
            joined: true, archived: false, locked: false, private: false, messageCount: nil,
            ownerId: nil))

    // Without the title bar's safe area, so points are the row's own.
    func render<Row: View>(_ row: Row) throws -> Rendered {
        try Rendered(
            row.frame(width: 240, height: 32).background(Color(nsColor: Palette.frame))
                .ignoresSafeArea(),
            size: NSSize(width: 240, height: 32))
    }

    func row(
        last: Bool = false, muted: Bool = false, badge: Badge = Badge(unread: false, mentions: 0)
    ) throws -> Rendered {
        try render(
            ThreadRow(channel: thread, last: last, selected: false, muted: muted, badge: badge) {})
    }

    func drawn(_ image: Rendered, _ x: CGFloat, _ y: CGFloat) -> Bool {
        !same(image.color(x, y), image.color(4, 16))
    }

    // Pixels other than the background in the box a channel row's 20 symbol takes, leaving out
    // the spine's column and the branch's row.
    func symbolPixels(_ image: Rendered) -> Int {
        var count = 0
        for x in stride(from: 16.0, to: 36.0, by: 1) where abs(x - ThreadRow.spine) > 1.5 {
            for y in stride(from: 6.0, to: 26.0, by: 1) where abs(y - 16) > 1.5 {
                count += drawn(image, x, y) ? 1 : 0
            }
        }
        return count
    }

    @Test
    func aThreadRowHangsFromItsParentsSpine() throws {
        let image = try row()

        #expect(drawn(image, ThreadRow.spine, 2), "the spine runs on from above")
        #expect(drawn(image, ThreadRow.spine, 30), "and on to the next thread")
        #expect(drawn(image, ThreadRow.spine + 5, 16), "the branch")
        #expect(!drawn(image, ThreadRow.spine - 6, 16))
    }

    @Test
    func theLastThreadsSpineEndsAtItsBranch() throws {
        let image = try row(last: true)

        #expect(drawn(image, ThreadRow.spine, 2))
        #expect(!drawn(image, ThreadRow.spine, 28))
        #expect(drawn(image, ThreadRow.spine + 5, 16))
    }

    @Test
    func aThreadRowDrawsNoSymbol() throws {
        let channelRow = try render(
            ChannelRow(channel: thread, selected: false, muted: false) {})

        #expect(symbolPixels(channelRow) > 20, "a channel row draws its symbol there")
        #expect(symbolPixels(try row()) == 0)
    }

    @Test
    func anUnreadThreadRowIsBrightWithThePill() throws {
        let read = try row()
        let unread = try row(badge: Badge(unread: true, mentions: 0))

        #expect(count(read, in: 44..<200, where: isBright) == 0)
        #expect(count(unread, in: 44..<200, where: isBright) > 0)
        #expect(count(read, in: 0..<4, where: isBright) == 0)
        #expect(count(unread, in: 0..<4, where: isBright) > 0, "the pill")
    }

    @Test
    func aMentionedThreadRowShowsItsCount() throws {
        let mentioned = try row(badge: Badge(unread: true, mentions: 3))

        #expect(count(try row(), in: 180..<240, where: isRed) == 0)
        #expect(count(mentioned, in: 180..<240, where: isRed) > 0)
    }

    @Test
    func aMutedThreadRowIsDimmed() throws {
        func brightest(_ image: Rendered) -> CGFloat {
            var top: CGFloat = 0
            for x in Int(44 * image.scale)..<Int(180 * image.scale) {
                for y in 0..<image.rep.pixelsHigh {
                    top = max(top, image.pixel(x, y)?.brightnessComponent ?? 0)
                }
            }
            return top
        }

        #expect(brightest(try row(muted: true)) < brightest(try row()) - 0.1)
    }

    @Test
    func theLastThreadUnderEachParentEndsItsSpine() {
        func channel(_ raw: UInt64, _ kind: ChannelType, parent: UInt64?) -> Channel {
            Channel(
                id: ChannelId(rawValue: raw), kind: kind, guildId: GuildId(rawValue: 1),
                parentId: parent.map(ChannelId.init(rawValue:)), name: "\(raw)", position: 0,
                topic: nil, nsfw: false, rateLimitPerUser: 0, recipientIds: [], thread: nil)
        }
        let rows = [
            channel(10, .guildCategory, parent: nil), channel(11, .guildText, parent: 10),
            channel(32, .publicThread, parent: 11), channel(31, .publicThread, parent: 11),
            channel(12, .guildText, parent: 10), channel(41, .privateThread, parent: 12),
        ]
        let last = ChannelSidebar.lastThreads(in: rows)

        #expect(last == [ChannelId(rawValue: 31), ChannelId(rawValue: 41)])
    }
}
