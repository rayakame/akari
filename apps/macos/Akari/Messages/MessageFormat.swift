import Foundation

enum MessageFormat {
    static func groupTime(_ date: Date, now: Date, calendar: Calendar, locale: Locale) -> String {
        let time = shortTime(date, calendar: calendar, locale: locale)
        if calendar.isDate(date, inSameDayAs: now) {
            return time
        }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
            calendar.isDate(date, inSameDayAs: yesterday)
        {
            return "Yesterday at \(time)"
        }
        return shortDate(calendar, locale).string(from: date)
    }

    // The locale's short date and time, "17.10.26, 16:02" (D123); a template can't give the short
    // style's digits. Formatters are costly to make, and this runs for every row.
    private static var formatters: [String: DateFormatter] = [:]

    static var keptFormats: Int { formatters.count }

    // A changed 24-hour or date format setting keeps the locale's identifier.
    static func forgetFormats() {
        formatters.removeAll()
    }

    private static func shortDate(_ calendar: Calendar, _ locale: Locale) -> DateFormatter {
        let key = "\(locale.identifier)|\(calendar.identifier)|\(calendar.timeZone.identifier)"
        if let formatter = formatters[key] {
            return formatter
        }
        let formatter = DateFormatter()
        formatter.locale = locale
        formatter.calendar = calendar
        formatter.timeZone = calendar.timeZone
        formatter.dateStyle = .short
        formatter.timeStyle = .short
        formatters[key] = formatter
        return formatter
    }

    static func shortTime(_ date: Date, calendar: Calendar, locale: Locale) -> String {
        date.formatted(style(calendar, locale).hour().minute())
    }

    static func dayDivider(_ day: Date, calendar: Calendar, locale: Locale) -> String {
        day.formatted(style(calendar, locale).month(.wide).day().year())
    }

    static func fullDate(_ date: Date, calendar: Calendar, locale: Locale) -> String {
        date.formatted(
            style(calendar, locale).weekday(.wide).month(.wide).day().year().hour().minute())
    }

    private static func style(_ calendar: Calendar, _ locale: Locale) -> Date.FormatStyle {
        Date.FormatStyle(locale: locale, calendar: calendar, timeZone: calendar.timeZone)
    }
}
