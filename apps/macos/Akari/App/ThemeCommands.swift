import SwiftUI

struct ThemeCommands: Commands {
    @AppStorage(ThemeChoice.key) private var theme = ThemeChoice.dark

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Picker("Theme", selection: choice) {
                ForEach(ThemeChoice.allCases) { choice in
                    Text(choice.title).tag(choice)
                }
            }
        }
    }

    private var choice: Binding<ThemeChoice> {
        Binding {
            theme
        } set: { choice in
            theme = choice
            NSApplication.shared.appearance = choice.appearance
        }
    }
}
