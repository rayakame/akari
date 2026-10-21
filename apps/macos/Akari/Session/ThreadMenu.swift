import AkariKit
import SwiftUI

/// A thread's "…" in the full view's header or the split view's: the items Akari has, in the
/// official order (D119), and where Discord is told the user chose them (D114).
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

    static func fullView(membership: SessionModel.MembershipItem) -> Self {
        Self(sections: [[.membership(membership)], [.openInSplitView]], location: .toolbarOverflow)
    }

    static func splitView(membership: SessionModel.MembershipItem) -> Self {
        Self(sections: [[.membership(membership), .openInFullView]], location: .sidebarOverflow)
    }
}

/// The "…" button that opens a `ThreadMenu`, in a header's 32-point button box.
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
        .help("More")
        .accessibilityLabel("More")
    }
}
