import AkariKit
import SwiftUI

// Discord's "Create a thread" shortcut: the open channel's form, or the open thread's parent's.
struct ThreadCommands: Commands {
    let app: AppModel?

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button("Create Thread") {
                if case .session(let session) = app?.screen {
                    session.openThreadFormHere()
                }
            }
            .keyboardShortcut("n", modifiers: [.command, .shift, .option])
        }
    }
}
