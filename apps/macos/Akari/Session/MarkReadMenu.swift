import AkariKit
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

    // A thread row's menu: Mark As Read, then Join Thread or Leave Thread (D119).
    func threadMenu(
        markReadEnabled: Bool, markRead: @escaping () -> Void,
        membership: SessionModel.MembershipItem?, changeMembership: @escaping () -> Void
    ) -> some View {
        contextMenu {
            Button("Mark As Read", action: markRead).disabled(!markReadEnabled)
            if let membership {
                Divider()
                Button(membership.title, action: changeMembership).disabled(!membership.enabled)
            }
        }
    }
}
