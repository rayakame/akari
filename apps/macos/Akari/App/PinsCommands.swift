import AkariKit
import SwiftUI

// Discord's ⌘P toggles the pins popout; Akari has nothing to print.
struct PinsCommands: Commands {
    let app: AppModel?

    private var session: SessionModel? {
        if case .session(let session) = app?.screen {
            return session
        }
        return nil
    }

    var body: some Commands {
        CommandGroup(replacing: .printItem) {}
        CommandGroup(before: .toolbar) {
            Button(session?.pins == nil ? "Show Pinned Messages" : "Hide Pinned Messages") {
                session?.togglePins()
            }
            .keyboardShortcut("p", modifiers: .command)
            .disabled(
                session?.pinsChannel == nil || session?.pinsDisabled == true
                    || session?.asksAboutMessage == true)
        }
    }
}
