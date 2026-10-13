import Foundation

/// The texts of `<t:…>` timestamps in a locale and time zone, as the official client formats
/// them there.
public struct TimestampText: Sendable {
    private let formatters: Formatters

    public init(locale: Locale, timeZone: TimeZone, calendar: Calendar) {
        formatters = Formatters(locale: locale, timeZone: timeZone, calendar: calendar)
    }

    public func text(_ seconds: Int64, style: TimestampStyle, now: Date) -> String {
        let date = Self.date(seconds)
        switch style {
        case .relative:
            return formatters.relative.localizedString(for: date, relativeTo: now)
        default:
            return formatters.fixed(style).string(from: date)
        }
    }

    /// The tooltip: weekday, date and time.
    public func fullDate(_ seconds: Int64) -> String {
        formatters.fixed(.fullDateShortTime).string(from: Self.date(seconds))
    }

    /// When a relative text next changes; nil for the other styles.
    public func nextChange(_ seconds: Int64, style: TimestampStyle, now: Date) -> Date? {
        guard style == .relative else {
            return nil
        }
        let ahead = Self.date(seconds).timeIntervalSince(now)
        let distance = abs(ahead)
        let unit: TimeInterval =
            switch distance {
            case ..<60: 1
            case ..<3600: 60
            case ..<86400: 3600
            default: 86400
            }
        // The formatter truncates: ahead, the text changes once the distance drops below its
        // whole units; behind, once it reaches the next one. A millisecond past either.
        let rest = distance.truncatingRemainder(dividingBy: unit)
        let step = ahead > 0 ? rest : unit - rest
        return now + step + 0.001
    }

    // JavaScript's range, which Discord's clients format; beyond it Foundation's dates still do.
    private static func date(_ seconds: Int64) -> Date {
        let limit: Int64 = 8_640_000_000_000
        return Date(timeIntervalSince1970: TimeInterval(min(max(seconds, -limit), limit)))
    }
}

// DateFormatter is safe to use from several threads once configured.
private final class Formatters: @unchecked Sendable {
    let relative: RelativeDateTimeFormatter
    private let styles: [TimestampStyle: DateFormatter]
    private let fallback: DateFormatter

    init(locale: Locale, timeZone: TimeZone, calendar: Calendar) {
        var calendar = calendar
        calendar.timeZone = timeZone
        calendar.locale = locale
        relative = RelativeDateTimeFormatter()
        relative.locale = locale
        relative.calendar = calendar
        relative.dateTimeStyle = .numeric
        relative.unitsStyle = .full
        func formatter(_ date: DateFormatter.Style, _ time: DateFormatter.Style) -> DateFormatter {
            let formatter = DateFormatter()
            formatter.locale = locale
            formatter.timeZone = timeZone
            formatter.calendar = calendar
            formatter.dateStyle = date
            formatter.timeStyle = time
            return formatter
        }
        let longShort = formatter(.long, .short)
        fallback = longShort
        styles = [
            .shortTime: formatter(.none, .short),
            .mediumTime: formatter(.none, .medium),
            .shortDate: formatter(.short, .none),
            .longDate: formatter(.long, .none),
            .longDateShortTime: longShort,
            .fullDateShortTime: formatter(.full, .short),
            .shortDateShortTime: formatter(.short, .short),
            .shortDateMediumTime: formatter(.short, .medium),
        ]
    }

    func fixed(_ style: TimestampStyle) -> DateFormatter {
        styles[style] ?? fallback
    }
}
