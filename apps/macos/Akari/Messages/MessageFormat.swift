import Foundation

/// Message times as docs/ui/message-list.md shows them, in the user's locale and time zone.
enum MessageFormat {
    static func groupTime(_ date: Date, now: Date, calendar: Calendar, locale: Locale) -> String {
        let time = shortTime(date, calendar: calendar, locale: locale)
        if calendar.isDate(date, inSameDayAs: now) {
            return "Today at \(time)"
        }
        if let yesterday = calendar.date(byAdding: .day, value: -1, to: now),
            calendar.isDate(date, inSameDayAs: yesterday)
        {
            return "Yesterday at \(time)"
        }
        return date.formatted(
            style(calendar, locale).month(.twoDigits).day(.twoDigits).year().hour().minute())
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
