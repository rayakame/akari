import AkariKit
import AppKit
import SwiftUI
import Testing

@testable import Akari

@MainActor
struct RailFolderTests {
    func item(_ names: [String], name: String? = nil, color: UInt32? = nil)
        -> ServerListModel.FolderItem
    {
        let guilds = names.enumerated().map { index, name in
            Guild(id: GuildId(rawValue: UInt64(index + 1)), name: name)
        }
        return ServerListModel.FolderItem(
            folder: Folder(id: 7, name: name, color: color, guildIds: guilds.map(\.id)),
            guilds: guilds)
    }

    @Test
    func aCollapsedFolderShowsUpToFourInitials() {
        let five = item(["Alpha", "Beta Lab", "Gamma", "Delta", "Epsilon"])

        #expect(FolderTile.minis(five) == ["A", "BL", "G", "D"])
        #expect(FolderTile.minis(item(["Alpha", "Beta"])) == ["A", "B"])
    }

    @Test
    func aFolderWithoutAColorIsNeutralGray() {
        #expect(FolderTile.tint(nil) == Palette.folder)
        #expect(
            same(
                FolderTile.tint(0x5865f2).usingColorSpace(.sRGB),
                NSColor(hex: "#5865f2").usingColorSpace(.sRGB)))
    }

    @Test
    func aFoldersTooltipIsItsNameOrItsServers() {
        #expect(FolderTile.help(item(["Alpha", "Beta"], name: "Games")) == "Games")
        #expect(FolderTile.help(item(["Alpha", "Beta"])) == "Alpha, Beta")
    }

    @Test
    func anExpandedFolderListsItsServersOnItsColor() throws {
        let column = FolderColumn(color: 0xff0000, expanded: true) {
            Color.clear.frame(width: SidebarWidth.rail, height: 96)
        }
        let collapsed = FolderColumn(color: 0xff0000, expanded: false) {
            Color.clear.frame(width: SidebarWidth.rail, height: 96)
        }

        let open = try Rendered(column, size: NSSize(width: SidebarWidth.rail, height: 96))
        let closed = try Rendered(collapsed, size: NSSize(width: SidebarWidth.rail, height: 96))

        let middle = try #require(open.color(SidebarWidth.rail / 2, 48))
        #expect(middle.redComponent > middle.greenComponent + 0.1, "\(middle)")
        let plain = try #require(closed.color(SidebarWidth.rail / 2, 48))
        #expect(abs(plain.redComponent - plain.greenComponent) < 0.05, "\(plain)")
    }
}

@MainActor
struct MutedRowTests {
    // The brightest pixel: a row's name and icon, drawn on the dark theme's frame.
    func brightest<Row: View>(_ row: Row) throws -> CGFloat {
        let rendered = try Rendered(
            row.frame(width: 240, height: 40).background(Color(nsColor: Palette.frame)),
            size: NSSize(width: 240, height: 40))
        var top: CGFloat = 0
        for y in 0..<rendered.rep.pixelsHigh {
            for x in 0..<rendered.rep.pixelsWide {
                if let color = rendered.pixel(x, y) {
                    top = max(top, color.redComponent + color.greenComponent + color.blueComponent)
                }
            }
        }
        return top
    }

    let general = Channel(
        id: ChannelId(rawValue: 11), kind: .guildText, guildId: GuildId(rawValue: 1),
        parentId: nil, name: "general", position: 0, topic: nil, nsfw: false,
        rateLimitPerUser: 0, recipientIds: [])

    @Test
    func aMutedChannelRowIsDimmed() throws {
        let normal = try brightest(ChannelRow(channel: general, selected: false, muted: false) {})
        let muted = try brightest(ChannelRow(channel: general, selected: false, muted: true) {})

        #expect(muted < normal * 0.8, "\(muted) against \(normal)")
    }

    @Test
    func aMutedCategoryIsDimmed() throws {
        let normal = try brightest(CategoryRow(name: "Text", collapsed: false, muted: false) {})
        let muted = try brightest(CategoryRow(name: "Text", collapsed: false, muted: true) {})

        #expect(muted < normal * 0.8, "\(muted) against \(normal)")
    }
}
