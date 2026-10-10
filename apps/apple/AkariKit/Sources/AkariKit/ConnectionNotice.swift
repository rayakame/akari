import Foundation

/// What the connection bar says while the session isn't online.
public enum ConnectionNotice: Equatable, Sendable {
    /// Connecting for the first time.
    case connecting
    /// Connecting again after being online.
    case reconnecting
    /// Disconnected on purpose, e.g. while the Mac sleeps.
    case offline
    /// Closed for good; `AuthenticationFailed` returns to the login screen instead.
    case closed(GatewayError?)

    public var text: String {
        switch self {
        case .connecting: "Connecting to Discord…"
        case .reconnecting: "Connection lost. Reconnecting…"
        case .offline: "Offline"
        case .closed(nil): "Akari lost its connection to Discord."
        case .closed(let error?):
            "Akari lost its connection to Discord (\(error.localizedDescription))."
        }
    }

    /// How long the state lasts before the bar shows it, so a quick resume doesn't flash it.
    public var delay: TimeInterval {
        switch self {
        case .connecting, .reconnecting: 1
        case .offline, .closed: 0
        }
    }

    public var offersReconnect: Bool {
        if case .closed = self { true } else { false }
    }
}
