import Foundation

/// The resizable sidebar: the server rail and the channel list together (docs/ui/layout.md).
enum SidebarWidth {
    static let range: ClosedRange<CGFloat> = 264...432
    static let standard: CGFloat = 375
    static let rail: CGFloat = 72
    private static let key = "sidebarWidth"

    static func clamped(_ width: CGFloat) -> CGFloat {
        min(max(width, range.lowerBound), range.upperBound)
    }

    /// The channel list: the sidebar without the rail and the line between them.
    static func listWidth(for total: CGFloat) -> CGFloat {
        total - rail - 1
    }

    static func stored(in defaults: UserDefaults) -> CGFloat {
        let stored = defaults.double(forKey: key)
        return stored == 0 ? standard : clamped(stored)
    }

    static func store(_ width: CGFloat, in defaults: UserDefaults) {
        defaults.set(Double(clamped(width)), forKey: key)
    }
}
