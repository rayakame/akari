import SwiftUI

extension View {
    // The read item leads the menu; with nothing unread it's disabled, not hidden (D98).
    func markReadMenu(
        _ title: String = "Mark As Read", enabled: Bool, action: @escaping () -> Void
    ) -> some View {
        contextMenu {
            Button(title, action: action).disabled(!enabled)
        }
    }
}
