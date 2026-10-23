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
            ownerId: nil, lastMessageId: nil, createdAt: nil, memberCount: nil, memberIdsPreview: [],
            archivedAt: Date(timeIntervalSince1970: 0)))

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
            ThreadRow(
                channel: thread, first: false, last: last, selected: false, muted: muted,
                badge: badge
            ) {})
    }

    func drawn(_ image: Rendered, _ x: CGFloat, _ y: CGFloat) -> Bool {
        !same(image.color(x, y), image.color(4, 16))
    }

    // Pixels other than the background in the box a channel row's 20 symbol takes, leaving out
    // the spine's column and the branch's row with their joint.
    func symbolPixels(_ image: Rendered) -> Int {
        var count = 0
        for x in stride(from: 16.0, to: 36.0, by: 1) where abs(x - ThreadRow.spine) > 3.5 {
            for y in stride(from: 6.0, to: 26.0, by: 1) where abs(y - 16) > 3.5 {
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

    // The parent row and its first thread, 2 apart as in the sidebar.
    func underParent(first: Bool) throws -> Rendered {
        let parent = Channel(
            id: ChannelId(rawValue: 11), kind: .guildText, guildId: GuildId(rawValue: 1),
            parentId: nil, name: "general", position: 0, topic: nil, nsfw: false,
            rateLimitPerUser: 0, recipientIds: [], thread: nil)
        return try Rendered(
            VStack(spacing: 2) {
                ChannelRow(channel: parent, selected: false, muted: false) {}
                ThreadRow(
                    channel: thread, first: first, last: true, selected: false, muted: false
                ) {}
            }
            .frame(width: 240, height: 66).background(Color(nsColor: Palette.frame))
            .ignoresSafeArea(),
            size: NSSize(width: 240, height: 66))
    }

    // As in Discord: the parent row ends at 32, the thread's at 34; only the round cap reaches up.
    @Test
    func theFirstThreadsSpineStartsAtItsRowWithARoundCap() throws {
        let image = try underParent(first: true)

        #expect(!drawn(image, ThreadRow.spine, 28), "nothing under the parent's symbol")
        #expect(!drawn(image, ThreadRow.spine, 32.25))
        #expect(drawn(image, ThreadRow.spine, 33.75), "the cap")
        #expect(drawn(image, ThreadRow.spine, 36))
        #expect(drawn(try underParent(first: false), ThreadRow.spine, 32.25), "a later thread joins")
    }

    @Test(arguments: [NSAppearance.Name.darkAqua, .aqua])
    func theSpineIsTwoPointsWideInItsOwnColor(_ appearance: NSAppearance.Name) throws {
        let image = try Rendered(
            ThreadRow(channel: thread, first: false, last: false, selected: false, muted: false) {}
                .frame(width: 240, height: 32).background(Color(nsColor: Palette.frame))
                .ignoresSafeArea(),
            appearance: appearance, size: NSSize(width: 240, height: 32))
        let reference = try Rendered(
            Color(nsColor: Palette.threadSpine).frame(width: 20, height: 20).ignoresSafeArea(),
            appearance: appearance, size: NSSize(width: 20, height: 20))
        let spine = try #require(reference.color(10, 10))

        #expect(drawn(image, ThreadRow.spine - 0.5, 8))
        #expect(drawn(image, ThreadRow.spine + 0.5, 8))
        #expect(!drawn(image, ThreadRow.spine - 1.5, 8))
        #expect(!drawn(image, ThreadRow.spine + 1.5, 8))
        let color = try #require(image.color(ThreadRow.spine - 0.5, 8))
        #expect(abs(color.redComponent - spine.redComponent) < 2 / 255, "\(color) against \(spine)")
        #expect(abs(color.blueComponent - spine.blueComponent) < 2 / 255)
        #expect(!same(color, image.color(4, 8)), "not the sidebar")
    }

    @Test
    func theBranchReachesNinePointsRightOfTheSpineAndIsTwoThick() throws {
        let image = try row()

        #expect(drawn(image, ThreadRow.spine + 8.5, 16), "the round cap")
        #expect(!drawn(image, ThreadRow.spine + 9.5, 16))
        #expect(drawn(image, ThreadRow.spine + 4.5, 15.5))
        #expect(drawn(image, ThreadRow.spine + 4.5, 16.5))
        #expect(!drawn(image, ThreadRow.spine + 4.5, 14.5))
        #expect(!drawn(image, ThreadRow.spine + 4.5, 17.5))
    }

    @Test
    func aMiddleJointHasFilletsAboveAndBelow() throws {
        let image = try row()

        #expect(drawn(image, ThreadRow.spine + 1.5, 14.5), "the fillet above")
        #expect(drawn(image, ThreadRow.spine + 1.5, 17.5), "the fillet below")
        #expect(!drawn(image, ThreadRow.spine + 2.5, 13.5), "a 2 point fillet, no more")
        #expect(!drawn(image, ThreadRow.spine + 2.5, 18.5))
    }

    @Test
    func theLastThreadsSpineBendsIntoItsBranch() throws {
        let last = try row(last: true)
        let middle = try row()

        #expect(drawn(last, ThreadRow.spine - 0.5, 9.5), "straight above the bend")
        #expect(!drawn(last, ThreadRow.spine - 0.5, 15.5), "the bend leaves the line")
        #expect(drawn(middle, ThreadRow.spine - 0.5, 15.5))
        #expect(drawn(last, ThreadRow.spine + 1.5, 15.5), "on the curve")
        #expect(!drawn(last, ThreadRow.spine, 20), "nothing below the branch")
        #expect(drawn(middle, ThreadRow.spine, 20))
    }

    @Test
    func theFirstThreadUnderEachParentStartsItsSpine() {
        func channel(_ raw: UInt64, _ kind: ChannelType, parent: UInt64?) -> Channel {
            Channel(
                id: ChannelId(rawValue: raw), kind: kind, guildId: GuildId(rawValue: 1),
                parentId: parent.map(ChannelId.init(rawValue:)), name: "\(raw)", position: 0,
                topic: nil, nsfw: false, rateLimitPerUser: 0, recipientIds: [], thread: nil)
        }
        let rows = [
            channel(11, .guildText, parent: nil), channel(32, .publicThread, parent: 11),
            channel(31, .publicThread, parent: 11), channel(12, .guildText, parent: nil),
            channel(41, .privateThread, parent: 12),
        ]
        let first = ChannelSidebar.firstThreads(in: rows)

        #expect(first == [ChannelId(rawValue: 32), ChannelId(rawValue: 41)])
    }

    @Test
    func theLastThreadsSpineEndsAtItsBranch() throws {
        let image = try row(last: true)

        #expect(drawn(image, ThreadRow.spine, 2))
        #expect(!drawn(image, ThreadRow.spine, 28))
        #expect(drawn(image, ThreadRow.spine + 5, 16))
    }

    // Discord's # is about 17 tall; every kind keeps one size.
    @Test
    func aChannelSymbolIsSeventeenPointsTall() throws {
        let general = Channel(
            id: ChannelId(rawValue: 11), kind: .guildText, guildId: GuildId(rawValue: 1),
            parentId: nil, name: "general", position: 0, topic: nil, nsfw: false,
            rateLimitPerUser: 0, recipientIds: [], thread: nil)
        let image = try Rendered(
            ChannelSymbol(channel: general).frame(width: 40, height: 40)
                .background(Color(nsColor: Palette.frame)).ignoresSafeArea(),
            size: NSSize(width: 40, height: 40))
        let rows = (0..<image.rep.pixelsHigh).filter { y in
            (0..<image.rep.pixelsWide).contains { x in
                !same(image.pixel(x, y), image.pixel(0, 0))
            }
        }
        let height = CGFloat((rows.last ?? 0) - (rows.first ?? 0) + 1) / image.scale

        #expect((16.5...18).contains(height), "\(height) points")
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
    func aThreadRowNamesItsParentAndItsMute() {
        #expect(
            ThreadRow.spokenName(name: "release checklist", parent: "general")
                == "release checklist, thread in general")
        #expect(ThreadRow.spokenName(name: "t1", parent: nil) == "t1, thread")
        #expect(
            MentionBadge.accessibilityValue(Badge(unread: false, mentions: 1), muted: true)
                == "muted, 1 mention")
    }

    @Test
    func aMutedChannelRowSaysSo() {
        #expect(
            MentionBadge.accessibilityValue(Badge(unread: true, mentions: 0), muted: true)
                == "muted, unread")
        #expect(
            MentionBadge.accessibilityValue(Badge(unread: true, mentions: 0), muted: false)
                == "unread")
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
