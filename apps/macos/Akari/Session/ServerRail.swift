import AkariKit
import SwiftUI

struct ServerRail: View {
    let session: SessionModel

    var body: some View {
        ScrollView(.vertical, showsIndicators: false) {
            VStack(spacing: 8) {
                RailItem(selected: session.place == .home, help: "Direct Messages") { highlighted in
                    RailIcon(highlighted: highlighted) {
                        Image(systemName: "bubble.left.and.bubble.right.fill")
                            .font(.system(size: 18))
                    }
                } action: {
                    session.open(.home)
                }
                Rectangle()
                    .fill(Color(nsColor: Palette.frameBorder))
                    .frame(width: 32, height: 1)
                ForEach(session.guilds.guilds, id: \.id) { guild in
                    RailItem(selected: session.place == .guild(guild.id), help: guild.name) {
                        highlighted in
                        GuildIcon(name: guild.name, highlighted: highlighted)
                    } action: {
                        session.open(.guild(guild.id))
                    }
                }
                let unavailable = session.guilds.unavailableIds.count
                if unavailable > 0 {
                    RailIcon(highlighted: false) {
                        Image(systemName: "exclamationmark.triangle")
                            .font(.system(size: 18))
                    }
                    .help(
                        unavailable == 1
                            ? "1 server unavailable" : "\(unavailable) servers unavailable")
                }
            }
            .padding(.top, 4)
            .padding(.bottom, 72)
            .frame(maxWidth: .infinity)
        }
    }
}

private struct RailItem<Icon: View>: View {
    let selected: Bool
    let help: String
    @ViewBuilder let icon: (_ highlighted: Bool) -> Icon
    let action: () -> Void
    @State private var hovered = false

    var body: some View {
        Button(action: action) {
            icon(selected || hovered)
        }
        .buttonStyle(.plain)
        .onHover { hovered = $0 }
        .help(help)
        .frame(width: SidebarWidth.rail, height: 40)
        .overlay(alignment: .leading) {
            RailPill(height: selected ? 40 : hovered ? 20 : 0)
        }
    }
}

private struct RailPill: View {
    let height: CGFloat

    var body: some View {
        UnevenRoundedRectangle(bottomTrailingRadius: 4, topTrailingRadius: 4)
            .fill(Color(nsColor: Palette.textStrong))
            .frame(width: 4, height: max(height, 8))
            .opacity(height == 0 ? 0 : 1)
            .animation(.easeOut(duration: 0.2), value: height)
    }
}

struct RailIcon<Content: View>: View {
    let highlighted: Bool
    @ViewBuilder let content: Content

    var body: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(Color(nsColor: highlighted ? Palette.brand : Palette.hoverBackground))
            .frame(width: 40, height: 40)
            .overlay {
                content.foregroundStyle(
                    highlighted ? Color.white : Color(nsColor: Palette.textDefault))
            }
            .animation(.easeOut(duration: 0.2), value: highlighted)
    }
}

struct GuildIcon: View {
    let name: String
    let highlighted: Bool

    var body: some View {
        let initials = Initials.of(name)
        RailIcon(highlighted: highlighted) {
            Text(initials)
                .font(.system(size: Self.fontSize(initials.count), weight: .medium))
                .lineLimit(1)
                .minimumScaleFactor(0.5)
                .padding(.horizontal, 2)
        }
    }

    static func fontSize(_ length: Int) -> CGFloat {
        switch length {
        case ...2: 18
        case 3...4: 16
        case 5: 14
        case 6: 12
        default: 10
        }
    }
}
