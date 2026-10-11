import Foundation
import OSLog

/// `-AkariLog load`: one line per history page and per channel open, with where the time
/// went. Channel IDs, counts and durations only, never content or names.
@MainActor
public enum LoadLog {
    public static var enabled = false
    private static let logger = Logger(subsystem: "app.akari", category: "load")

    public static var write: (String) -> Void = { text in
        logger.info("\(text, privacy: .public)")
    }

    public static func isAsked(by filter: String) -> Bool {
        filter.split(separator: ",").contains { directive in
            directive.split(separator: "=").first?.trimmingCharacters(in: .whitespaces)
                == "load"
        }
    }

    public static func log(_ message: @autoclosure () -> String) {
        guard enabled else {
            return
        }
        write(message())
    }

    /// `render` is the table update that inserted `rows` of the page, `nil` when the load
    /// changed no rows; `held` is how long a live scroll held that update back.
    public static func page(
        _ trace: LoadTrace, render: Duration?, rows: Int, held: Duration?, shown: Duration
    ) -> String {
        guard let report = trace.report else {
            return "\(name(trace)): nothing to load"
        }
        let limits =
            report.remaining.map { remaining in
                "remaining \(remaining), reset "
                    + (report.resetAfter.map { String(format: "%.1f s", $0) } ?? "none")
            } ?? "no limit headers"
        return "\(name(trace)): queued \(ms(report.queued)), limited \(ms(report.limited)), "
            + "headers \(ms(report.headers)), "
            + (report.newConnection ? "new connection" : "connection reused")
            + ", body \(ms(report.body)), \(report.bytes) bytes, decode \(ms(report.decode)), "
            + "store \(ms(report.store)), read \(ms(trace.read)), "
            + "render \(render.map { "\(ms($0)) for \(rows) rows" } ?? "none (no rows changed)")"
            + (held.map { ", held \(ms($0)) for live scroll" } ?? "")
            + ", shown after \(ms(shown)); "
            + "\(report.received) messages; \(limits)"
    }

    public static func failure(_ trace: LoadTrace) -> String {
        "\(name(trace)) failed after \(ms(trace.duration)): "
            + (trace.failure.map(kind) ?? "unknown")
    }

    public static func opened(
        _ channel: ChannelId, from origin: MessageListModel.Origin, after: Duration
    ) -> String {
        let source =
            switch origin {
            case .window: "the window"
            case .disk: "disk"
            case .network: "the network"
            }
        return "channel \(channel.rawValue) opened: rows from \(source) after \(ms(after))"
    }

    private static func name(_ trace: LoadTrace) -> String {
        let kind =
            switch trace.kind {
            case .latest: "latest"
            case .older: "older"
            case .newer: "newer"
            }
        return "channel \(trace.channelId.rawValue) \(kind) page \(trace.sequence)"
    }

    // Discord's message can quote what was sent; only the kind and code are logged.
    private static func kind(_ error: RequestError) -> String {
        switch error {
        case .Unauthorized: "unauthorized"
        case .RateLimited: "rate limited"
        case .CaptchaRequired: "captcha required"
        case .Discord(_, let code, _): "Discord error \(code)"
        case .ServerError(let status): "server error \(status)"
        case .Network: "network"
        case .UnexpectedResponse: "unexpected response"
        case .TooLong: "too long"
        case .InvalidRequest: "invalid request"
        case .Closed: "closed"
        }
    }

    private static func ms(_ seconds: TimeInterval) -> String {
        let milliseconds = seconds * 1000
        return milliseconds < 10
            ? String(format: "%.1f ms", milliseconds) : "\(Int(milliseconds.rounded())) ms"
    }

    private static func ms(_ duration: Duration) -> String {
        let (seconds, attoseconds) = duration.components
        return ms(TimeInterval(seconds) + TimeInterval(attoseconds) / 1e18)
    }
}
