import AkariKit
import SwiftUI

struct RootView: View {
    let launch: Launch.State

    var body: some View {
        Group {
            switch launch {
            case .testing:
                Color(nsColor: Palette.frame)
            case .failed(let message):
                Text(message)
                    .foregroundStyle(Color(nsColor: Palette.textMuted))
                    .frame(maxWidth: .infinity, maxHeight: .infinity)
            case .running(let app):
                AppScreen(app: app)
            }
        }
        .frame(minWidth: 940, minHeight: 500)
        .background(Color(nsColor: Palette.frame))
        .background(WindowChrome())
        // SwiftUI resets the title bar's transparency when it shows the window.
        .toolbarBackground(.hidden, for: .windowToolbar)
    }
}

private struct AppScreen: View {
    let app: AppModel

    var body: some View {
        ZStack {
            switch app.screen {
            case .starting:
                Color(nsColor: Palette.frame)
            case .login(let login):
                LoginView(model: login)
                    .transition(.opacity)
            case .session(let session):
                SessionView(session: session) { messages, placeholder in
                    MessageArea(messages: messages, placeholder: placeholder)
                }
                .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: app.screen.key)
        .task { await app.start() }
    }
}

extension AppModel.Screen {
    // Identifies the screen for transitions: a new login or session counts as a change.
    var key: ObjectIdentifier? {
        switch self {
        case .starting: nil
        case .login(let login): ObjectIdentifier(login)
        case .session(let session): ObjectIdentifier(session)
        }
    }
}
