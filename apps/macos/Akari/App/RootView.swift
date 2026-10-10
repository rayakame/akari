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
    @State private var power: PowerEvents?

    var body: some View {
        ZStack {
            switch app.screen {
            case .starting:
                Color(nsColor: Palette.frame)
            case .login(let login):
                LoginView(model: login)
                    .overlay(alignment: .top) {
                        if app.warning == .logoutNotConfirmed, let warning = app.warning {
                            WarningLine(text: warning.text) { app.dismissWarning() }
                                .padding(.top, 32)
                        }
                    }
                    .transition(.opacity)
            case .session(let session):
                SessionView(
                    session: session, warning: app.warning,
                    dismissWarning: { app.dismissWarning() }, reconnect: { app.reconnect() }
                ) { messages, placeholder in
                    MessageArea(messages: messages, placeholder: placeholder)
                }
                .transition(.opacity)
            }
        }
        .animation(.easeInOut(duration: 0.2), value: app.screen.key)
        .alert(
            "Akari couldn't log out",
            isPresented: Binding(
                get: { app.logoutError != nil }, set: { if !$0 { app.dismissLogoutError() } }),
            presenting: app.logoutError
        ) { _ in
            Button("OK") { app.dismissLogoutError() }
        } message: { error in
            Text(
                "Akari couldn't remove your login from the Keychain, so you're still logged in. "
                    + error.localizedDescription)
        }
        .task {
            power = PowerEvents(sleep: { app.suspend() }, wake: { app.resume() })
            await app.start()
        }
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
