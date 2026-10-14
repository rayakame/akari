import AppKit
import SwiftUI

@main
struct AkariApp: App {
    @State private var launch = Launch.start()
    @NSApplicationDelegateAdaptor private var delegate: AppDelegate

    init() {
        NSApplication.shared.appearance = ThemeChoice.stored(in: .standard).appearance
    }

    var body: some Scene {
        Window("Akari", id: "main") {
            RootView(launch: launch)
                .onAppear {
                    if let app = launch.app {
                        delegate.quit = { limit in await app.prepareToQuit(limit: limit) }
                    }
                }
        }
        .defaultSize(width: 1280, height: 800)
        .windowResizability(.contentMinSize)
        // The session centers its own title (TitleBar); SwiftUI would show the window's title
        // again whenever a navigation title changes.
        .windowToolbarStyle(.unified(showsTitle: false))
        .commands {
            AccountCommands(app: launch.app)
            ThemeCommands()
        }
    }
}
