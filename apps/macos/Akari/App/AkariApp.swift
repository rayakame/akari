import AppKit
import SwiftUI

@main
struct AkariApp: App {
    @State private var launch = Launch.start()

    init() {
        NSApplication.shared.appearance = ThemeChoice.stored(in: .standard).appearance
    }

    var body: some Scene {
        Window("Akari", id: "main") {
            RootView(launch: launch)
        }
        .defaultSize(width: 1280, height: 800)
        .windowResizability(.contentMinSize)
        .commands {
            AccountCommands(app: launch.app)
            ThemeCommands()
        }
    }
}
