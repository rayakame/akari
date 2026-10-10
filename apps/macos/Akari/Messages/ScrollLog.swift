import AppKit
import OSLog

// `-AkariLog scroll`: the message list's clip origin, row height, elasticity and our own scrolls
// on every bounds change. Numbers only, never message content.
enum ScrollLog {
    static var enabled = false
    private static let logger = Logger(subsystem: "app.akari", category: "scroll")

    static func isAsked(by filter: String) -> Bool {
        filter.split(separator: ",").contains { directive in
            directive.split(separator: "=").first?.trimmingCharacters(in: .whitespaces)
                == "scroll"
        }
    }

    // Where lines go; the tests read them here.
    static var write: (String) -> Void = { text in
        logger.info("\(text, privacy: .public)")
    }

    static func log(_ message: @autoclosure () -> String) {
        guard enabled else {
            return
        }
        write(message())
    }

    static func name(_ elasticity: NSScrollView.Elasticity) -> String {
        switch elasticity {
        case .none: "none"
        case .allowed: "allowed"
        case .automatic: "automatic"
        @unknown default: "\(elasticity.rawValue)"
        }
    }
}
