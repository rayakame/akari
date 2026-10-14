import AkariKit
import SwiftUI

/// A server folder's tile on the rail: up to four of its servers in a 2 × 2 grid while
/// collapsed, a folder symbol while expanded.
struct FolderTile: View {
    let item: ServerListModel.FolderItem
    let expanded: Bool

    var body: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(Color(nsColor: Self.background(item.folder.color)))
            .frame(width: 40, height: 40)
            .overlay {
                if expanded {
                    Image(systemName: "folder.fill")
                        .font(.system(size: 18))
                        .foregroundStyle(
                            Color(
                                nsColor: item.folder.color == nil
                                    ? Palette.interactiveText : Self.tint(item.folder.color)))
                } else {
                    MiniGrid(initials: Self.minis(item))
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

private struct MiniGrid: View {
    let initials: [String]

    var body: some View {
        let rows = stride(from: 0, to: initials.count, by: 2).map {
            Array(initials[$0..<min($0 + 2, initials.count)])
        }
        VStack(spacing: 2) {
            ForEach(rows.indices, id: \.self) { row in
                HStack(spacing: 2) {
                    ForEach(rows[row].indices, id: \.self) { column in
                        RoundedRectangle(cornerRadius: 5, style: .continuous)
                            .fill(Color(nsColor: Palette.hoverBackground))
                            .frame(width: 16, height: 16)
                            .overlay {
                                Text(rows[row][column])
                                    .font(.system(size: 7, weight: .medium))
                                    .lineLimit(1)
                                    .minimumScaleFactor(0.5)
                                    .foregroundStyle(Color(nsColor: Palette.textDefault))
                            }
                    }
                }
            }
        }
        .frame(width: 34, height: 34, alignment: .top)
    }
}

/// An expanded folder's servers, on a column in the folder's color under its tile.
struct FolderColumn<Content: View>: View {
    let color: UInt32?
    let expanded: Bool
    @ViewBuilder let content: Content

    var body: some View {
        VStack(spacing: 8) {
            content
        }
        .background {
            if expanded {
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(Color(nsColor: FolderTile.background(color)))
                    .frame(width: 48)
            }
        }
    }
}
