import AkariKit
import SwiftUI

/// A server folder's header on the rail: a tile with up to four of its servers in a 2 × 2
/// grid while collapsed, only a folder symbol while expanded.
struct FolderTile: View {
    let item: ServerListModel.FolderItem
    let expanded: Bool

    var body: some View {
        if expanded {
            Image(systemName: "folder.fill")
                .font(.system(size: 18))
                .foregroundStyle(Color(nsColor: item.folder.color.map(Self.tint) ?? Palette.brand))
                .frame(width: 40, height: 40)
        } else {
            RoundedRectangle(cornerRadius: RailShape.cornerRadius, style: .continuous)
                .fill(Color(nsColor: Self.background(item.folder.color)))
                .frame(width: 40, height: 40)
                .overlay {
                    MiniGrid(guilds: Array(item.guilds.prefix(4)))
                }
        }
    }

    /// The first four servers' initials, as the tile shows them.
    static func minis(_ item: ServerListModel.FolderItem) -> [String] {
        item.guilds.prefix(4).map { Initials.of($0.name) }
    }

    /// The folder's color, or the neutral gray of a folder without one.
    static func tint(_ color: UInt32?) -> NSColor {
        guard let color else {
            return Palette.folder
        }
        return NSColor(hex: String(format: "#%06x", color))
    }

    // A colored folder's tile and column show its color faintly.
    static func background(_ color: UInt32?) -> NSColor {
        color == nil ? Palette.folder : tint(color).withAlphaComponent(0.3)
    }

    /// Whether the tile carries the selected pill: the open server is inside, out of sight.
    static func selected(
        _ item: ServerListModel.FolderItem, expanded: Bool, place: SessionModel.Place
    ) -> Bool {
        !expanded && item.guilds.contains { place == .guild($0.id) }
    }

    /// The folder's name, or its servers' names when it has none.
    static func help(_ item: ServerListModel.FolderItem) -> String {
        item.folder.name ?? item.guilds.map(\.name).joined(separator: ", ")
    }
}

// Up to four servers' minis, 16 each: their icon or their initials, as on the rail.
private struct MiniGrid: View {
    let guilds: [Guild]

    var body: some View {
        let rows = stride(from: 0, to: guilds.count, by: 2).map {
            Array(guilds[$0..<min($0 + 2, guilds.count)])
        }
        VStack(spacing: 2) {
            ForEach(rows.indices, id: \.self) { row in
                HStack(spacing: 2) {
                    ForEach(rows[row], id: \.id) { guild in
                        ServerPicture(name: guild.name, icon: guild.icon, size: 16, cornerRadius: 5)
                    }
                }
            }
        }
        .frame(width: 34, height: 34, alignment: .top)
    }
}

private let columnPadding: CGFloat = 4

/// An expanded folder: one column in the folder's color around its header and servers. A
/// background, so it never clips them.
struct FolderColumn<Content: View>: View {
    let color: UInt32?
    let expanded: Bool
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 8) {
            content
        }
        .padding(.vertical, expanded ? columnPadding : 0)
        .background {
            if expanded {
                // Concentric with the servers' corners, so none pokes out of the column.
                RoundedRectangle(
                    cornerRadius: RailShape.cornerRadius + columnPadding, style: .continuous
                )
                .fill(Color(nsColor: FolderTile.background(color)))
                .frame(width: 40 + 2 * columnPadding)
            }
        }
    }
}
