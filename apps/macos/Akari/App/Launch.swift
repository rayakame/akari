import AkariKit
import Foundation

enum Launch {
    enum State {
        // Hosted tests run inside the app; they must not reach the Keychain or Discord.
        case testing
        case failed(String)
        case running(AppModel)

        var app: AppModel? {
            if case .running(let app) = self { app } else { nil }
        }
    }

    // Hosted test runs, Swift Testing included, set this in the app's environment.
    static var isTesting: Bool {
        ProcessInfo.processInfo.environment["XCTestConfigurationFilePath"] != nil
    }

    static func start(_ defaults: UserDefaults = .standard) -> State {
        if isTesting {
            return .testing
        }
        LaunchLog.mark("app started")
        enableLoggingIfAsked(defaults) { filter in
            ScrollLog.enabled = ScrollLog.isAsked(by: filter)
            try enableLogging(filter: filter)
        }
        do {
            let client = try DiscordClient(host: .current, tokenStore: KeychainTokenStore())
            LaunchLog.mark("client created")
            return .running(AppModel(client: client))
        } catch {
            return .failed(error.localizedDescription)
        }
    }

    static func enableLoggingIfAsked(_ defaults: UserDefaults, enable: (String) throws -> Void) {
        guard let filter = defaults.string(forKey: "AkariLog") else {
            return
        }
        do {
            try enable(filter)
        } catch {
            FileHandle.standardError.write(Data("AkariLog: \(error.localizedDescription)\n".utf8))
        }
    }
}
