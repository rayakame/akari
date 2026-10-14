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

    // Colors down the middle of a rendered view, as runs: (kind, length in pixels).
    func runs(
        _ rendered: Rendered, at x: CGFloat, kind: (NSColor) -> Character
    ) -> [(Character, Int)] {
        var runs: [(Character, Int)] = []
        let column = Int(x * rendered.scale)
        for y in 0..<rendered.rep.pixelsHigh {
            guard let color = rendered.pixel(column, y) else {
                continue
            }
            let next = kind(color)
            if let last = runs.last, last.0 == next {
                runs[runs.count - 1].1 += 1
            } else {
                runs.append((next, 1))
            }
        }
        return runs
    }

    @Test
    func anExpandedFoldersColumnPadsTopAndBottomAlike() throws {
        let column = FolderColumn(color: 0xff0000, expanded: true) {
            Color.blue.frame(width: 40, height: 40)
            Color.blue.frame(width: 40, height: 40)
        }
        .frame(width: SidebarWidth.rail, height: 160)
        .background(Color.black)
        let rendered = try Rendered(column, size: NSSize(width: SidebarWidth.rail, height: 160))

        let colors = runs(rendered, at: SidebarWidth.rail / 2) { color in
            if color.blueComponent > 0.5 { return "b" }
            if color.redComponent > 0.15 { return "r" }
            return "k"
        }

        #expect(colors.map(\.0) == ["k", "r", "b", "r", "b", "r", "k"], "\(colors)")
        guard colors.count == 7 else {
            return
        }
        #expect(colors[1].1 == colors[5].1, "top \(colors[1].1) px, bottom \(colors[5].1) px")
        #expect(abs(CGFloat(colors[3].1) - 8 * rendered.scale) <= 1, "the gap is \(colors[3].1) px")
    }

    @Test
    func anExpandedFoldersHeaderIsItsSymbolAlone() throws {
        func render(_ color: UInt32?) throws -> Rendered {
            let folder = item(["Alpha", "Beta"], color: color)
            return try Rendered(
                FolderTile(item: folder, expanded: true)
                    .frame(width: 60, height: 60)
                    .background(Color(nsColor: Palette.frame)),
                size: NSSize(width: 60, height: 60))
        }
        let plain = try render(nil)
        let red = try render(0xff0000)
        let pixels = { (rendered: Rendered) in
            (0..<rendered.rep.pixelsHigh).flatMap { y in
                (0..<rendered.rep.pixelsWide).compactMap { rendered.pixel($0, y) }
            }
        }

        // Inside a tile's left edge, beside the symbol, the background shows.
        #expect(same(plain.color(14, 30), plain.color(2, 30)), "no tile behind the symbol")
        #expect(
            pixels(plain).contains { $0.blueComponent > 0.8 && $0.redComponent < 0.5 },
            "the symbol is brand without a color")
        #expect(
            pixels(red).contains { $0.redComponent > 0.8 && $0.greenComponent < 0.3 },
            "the symbol takes the folder's color")
    }

    @Test
    func aCollapsedFolderShowsTheOpenServerInside() {
        let folder = item(["Alpha", "Beta"])
        let beta = SessionModel.Place.guild(GuildId(rawValue: 2))

        #expect(FolderTile.selected(folder, expanded: false, place: beta))
        #expect(!FolderTile.selected(folder, expanded: true, place: beta), "its server shows it")
        #expect(!FolderTile.selected(folder, expanded: false, place: .home))
        #expect(
            !FolderTile.selected(
                folder, expanded: false, place: .guild(GuildId(rawValue: 9))))
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
