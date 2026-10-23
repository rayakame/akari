import AkariKit
import SwiftUI

// The items in the official order (D119), and the location Discord is told (D114).
struct ThreadMenu: Equatable {
    enum Item: Equatable {
        case membership(SessionModel.MembershipItem)
        case openInSplitView
        case openInFullView

        var title: String {
            switch self {
            case .membership(let item): item.title
            case .openInSplitView: "Open in Split View"
            case .openInFullView: "Open in Full View"
            }
        }

        var enabled: Bool {
            if case .membership(let item) = self {
                return item.enabled
            }
            return true
        }
    }

    let sections: [[Item]]
    let location: ThreadLocation

    static func fullView(membership: SessionModel.MembershipItem, splittable: Bool) -> Self {
        Self(
            sections: [[.membership(membership)]] + (splittable ? [[.openInSplitView]] : []),
            location: .toolbarOverflow)
    }

    // Without the thread's state there's nothing to join or leave, but Open in Full View stays.
    static func splitView(membership: SessionModel.MembershipItem?) -> Self {
        Self(
            sections: [membership.map { [.membership($0)] }.map { $0 + [.openInFullView] }
                ?? [.openInFullView]],
            location: .sidebarOverflow)
    }

    @MainActor
    static func splitView(of session: SessionModel) -> Self? {
        session.split.map { .splitView(membership: session.membershipItem(for: $0.channelId)) }
    }
}

struct ThreadMenuButton: View {
    let menu: ThreadMenu
    let act: (ThreadMenu.Item) -> Void
    @State private var hovered = false

    var body: some View {
        Menu {
            ForEach(Array(menu.sections.enumerated()), id: \.offset) { index, section in
                if index > 0 {
                    Divider()
                }
                ForEach(Array(section.enumerated()), id: \.offset) { _, item in
                    Button(item.title) { act(item) }
                        .disabled(!item.enabled)
                }
            }
        } label: {
            Image(systemName: "ellipsis")
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(
                    Color(nsColor: hovered ? Palette.interactiveText : Palette.textMuted))
                .frame(width: HeaderIconButton.size, height: HeaderIconButton.size)
                .contentShape(Rectangle())
        }
        // A borderless menu button tints its label in the system's color.
        .menuStyle(.button)
        .buttonStyle(.plain)
        .menuIndicator(.hidden)
        .fixedSize()
        .onHover { hovered = $0 }
        .background(CursorArea(.pointingHand))
        .help("More")
        .accessibilityLabel("More")
    }
}
