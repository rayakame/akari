import AkariKit
import SwiftUI

/// Where the user is, as the title bar names it.
struct WindowTitle: Equatable {
    enum Icon: Equatable {
        case server(String)
        case avatar(name: String, id: UInt64)
        case home
    }

    let text: String
    let icon: Icon

    static func at(
        _ place: SessionModel.Place, server: String?,
        conversation: (name: String, avatarId: UInt64)?
    ) -> WindowTitle {
        switch place {
        case .guild:
            let name = server ?? "Akari"
            return WindowTitle(text: name, icon: .server(name))
        case .home:
            // What the official client shows for an open DM isn't recorded yet (docs/ui/layout.md).
            guard let conversation else {
                return WindowTitle(text: "Direct Messages", icon: .home)
            }
            return WindowTitle(
                text: conversation.name,
                icon: .avatar(name: conversation.name, id: conversation.avatarId))
        }
    }
}

// The window's own title stays hidden: this one is centered on the whole window, as the
// official client's is, and the native title bar under it keeps dragging and zooming.
struct TitleBar: View {
    static let height: CGFloat = 32
    // Measured in the official client.
    static let iconSize: CGFloat = 16
    static let iconRadius: CGFloat = 4
    static let gap: CGFloat = 8
    // Clear of the traffic lights on both sides, so the title stays centered.
    private static let margin: CGFloat = 80

    let title: WindowTitle?

    var body: some View {
        ZStack {
            Color(nsColor: Palette.frame)
            if let title {
                HStack(spacing: Self.gap) {
                    icon(title.icon)
                    Text(title.text)
                        .font(TypeScale.titleBar.font)
                        .foregroundStyle(Color(nsColor: Palette.textDefault))
                        .lineLimit(1)
                }
                .padding(.horizontal, Self.margin)
                // NSWindow.title says the same to VoiceOver.
                .accessibilityHidden(true)
            }
        }
        .frame(height: Self.height)
        // The strip lies in the title bar, outside the safe area.
        .ignoresSafeArea(edges: .top)
    }

    @ViewBuilder private func icon(_ icon: WindowTitle.Icon) -> some View {
        switch icon {
        case .avatar(let name, let id):
            InitialsAvatar(name: name, id: id, size: Self.iconSize)
        case .server(let name):
            let initials = Initials.of(name)
            tile {
                Text(initials)
                    .font(
                        .system(
                            size: GuildIcon.fontSize(initials.count) * Self.iconSize / 40,
                            weight: .medium)
                    )
                    .lineLimit(1)
                    .minimumScaleFactor(0.5)
            }
        case .home:
            tile { AkariLogo() }
        }
    }

    // A rail icon at the title's size.
    private func tile(@ViewBuilder _ content: () -> some View) -> some View {
        RoundedRectangle(cornerRadius: Self.iconRadius, style: .continuous)
            .fill(Color(nsColor: Palette.hoverBackground))
            .frame(width: Self.iconSize, height: Self.iconSize)
            .overlay { content().foregroundStyle(Color(nsColor: Palette.textDefault)) }
    }
}

// The top and left edges of the area right of the rail and under the title bar: a 1 pt line with
// a rounded top-left corner, as in the official client.
nonisolated struct ContentCorner: Shape {
    // Measured: the line's outer edge.
    static let radius: CGFloat = 11.5

    func path(in rect: CGRect) -> Path {
        var path = Path()
        let inset: CGFloat = 0.5
        path.move(to: CGPoint(x: rect.minX + inset, y: rect.maxY))
        path.addLine(to: CGPoint(x: rect.minX + inset, y: rect.minY + Self.radius))
        path.addArc(
            tangent1End: CGPoint(x: rect.minX + inset, y: rect.minY + inset),
            tangent2End: CGPoint(x: rect.minX + Self.radius, y: rect.minY + inset),
            radius: Self.radius - inset)
        path.addLine(to: CGPoint(x: rect.maxX, y: rect.minY + inset))
        return path
    }
}
