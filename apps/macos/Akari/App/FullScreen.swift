import AppKit
import SwiftUI

// SwiftUI's View menu has no Enter Full Screen for this window, so ⌃⌘F did nothing; the
// command acts on the session's window, also from the Threads browser's panel above it.
enum FullScreen {
    @MainActor
    static func toggle(from window: NSWindow?) {
        var root = window
        while let parent = root?.parent {
            root = parent
        }
        root?.toggleFullScreen(nil)
    }

    /// The menu's title, as AppKit's own item words it.
    @MainActor
    final class State: ObservableObject {
        @Published private(set) var title = "Enter Full Screen"
        private var observers: [NSObjectProtocol] = []

        init(center: NotificationCenter = .default) {
            for (name, title) in [
                (NSWindow.didEnterFullScreenNotification, "Exit Full Screen"),
                (NSWindow.didExitFullScreenNotification, "Enter Full Screen"),
            ] {
                observers.append(
                    center.addObserver(forName: name, object: nil, queue: nil) { [weak self] _ in
                        MainActor.assumeIsolated { self?.title = title }
                    })
            }
        }
    }
}

struct FullScreenCommands: Commands {
    @StateObject private var state = FullScreen.State()

    var body: some Commands {
        CommandGroup(after: .toolbar) {
            Button(state.title) { FullScreen.toggle(from: NSApp.keyWindow ?? NSApp.mainWindow) }
                .keyboardShortcut("f", modifiers: [.control, .command])
        }
    }
}
