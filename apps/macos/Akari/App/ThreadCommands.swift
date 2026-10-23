import AkariKit
import SwiftUI

// Discord's "Create a thread" shortcut: the open channel's form, or the open thread's parent's.
struct ThreadCommands: Commands {
    let app: AppModel?

    private var session: SessionModel? {
        if case .session(let session) = app?.screen {
            return session
        }
        return nil
    }

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Create Thread") {
                session?.openThreadFormHere()
            }
            .keyboardShortcut("n", modifiers: [.command, .shift, .option])
            .disabled(session?.canCreateThreadHere != true)
        }
    }
}
