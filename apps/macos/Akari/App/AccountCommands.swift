import AkariKit
import SwiftUI

// Discord keeps Log Out in Settings, which Akari doesn't have yet.
struct AccountCommands: Commands {
    let app: AppModel?

    var body: some Commands {
        CommandGroup(after: .appSettings) {
            Button("Log Out") {
                Task { await app?.logOut() }
            }
            .disabled(app?.screen.isSession != true)
        }
    }
}

extension AppModel.Screen {
    var isSession: Bool {
        if case .session = self { true } else { false }
    }
}
