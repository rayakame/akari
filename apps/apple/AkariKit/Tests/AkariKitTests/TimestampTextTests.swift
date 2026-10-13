import Foundation
import Testing

@testable import AkariKit

struct TimestampTextTests {
    static let instant: Int64 = 1_800_000_000
    static let berlin = TimeZone(identifier: "Europe/Berlin")!

    func times(_ locale: String) -> TimestampText {
        TimestampText(
            locale: Locale(identifier: locale), timeZone: Self.berlin,
            calendar: Calendar(identifier: .gregorian))
    }

    // The day of check A in the official client.
    var checkA: Date {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = Self.berlin
        return calendar.date(from: DateComponents(year: 2026, month: 10, day: 13, hour: 12))!
    }

    @Test(arguments: [
        (TimestampStyle.shortTime, "09:00"),
        (.mediumTime, "09:00:00"),
        (.shortDate, "15.01.27"),
        (.longDate, "15. Januar 2027"),
        (.longDateShortTime, "15. Januar 2027 um 09:00"),
        (.fullDateShortTime, "Freitag, 15. Januar 2027 um 09:00"),
        (.shortDateShortTime, "15.01.27, 09:00"),
        (.shortDateMediumTime, "15.01.27, 09:00:00"),
        (.relative, "in 3 Monaten"),
    ])
    func everyStyleInGerman(_ style: TimestampStyle, _ shown: String) {
        #expect(times("de_DE").text(Self.instant, style: style, now: checkA) == shown)
    }

    @Test(arguments: [
        (TimestampStyle.shortTime, "9:00\u{202F}AM"),
        (.mediumTime, "9:00:00\u{202F}AM"),
        (.shortDate, "1/15/27"),
        (.longDate, "January 15, 2027"),
        (.longDateShortTime, "January 15, 2027 at 9:00\u{202F}AM"),
        (.fullDateShortTime, "Friday, January 15, 2027 at 9:00\u{202F}AM"),
        (.shortDateShortTime, "1/15/27, 9:00\u{202F}AM"),
        (.shortDateMediumTime, "1/15/27, 9:00:00\u{202F}AM"),
        (.relative, "in 3 months"),
    ])
    func everyStyleInEnglish(_ style: TimestampStyle, _ shown: String) {
        #expect(times("en_US").text(Self.instant, style: style, now: checkA) == shown)
    }

    @Test
    func theTooltipIsTheFullDate() {
        #expect(times("de_DE").fullDate(Self.instant) == "Freitag, 15. Januar 2027 um 09:00")
    }

    @Test
    func relativeTimesBothWays() {
        let now = Date(timeIntervalSince1970: TimeInterval(Self.instant))
        let english = times("en_US")
        #expect(english.text(Self.instant, style: .relative, now: now + 300) == "5 minutes ago")
        #expect(english.text(Self.instant, style: .relative, now: now - 7200) == "in 2 hours")
    }

    @Test(arguments: [
        (30, 1), (-30, 1), (1800, 60), (-1800, 60), (5 * 3600, 3600), (-5 * 3600, 3600),
        (3 * 86400, 86400), (-3 * 86400, 86400),
    ] as [(TimeInterval, TimeInterval)])
    func relativeTimesChangeAtTheirUnit(_ ahead: TimeInterval, _ unit: TimeInterval) throws {
        let target = Date(timeIntervalSince1970: TimeInterval(Self.instant))
        let now = target - ahead + 0.25
        let next = try #require(times("en_US").nextChange(Self.instant, style: .relative, now: now))
        #expect(next > now && next <= now + unit)
    }

    @Test(arguments: [
        TimestampStyle.shortTime, .mediumTime, .shortDate, .longDate, .longDateShortTime,
        .fullDateShortTime, .shortDateShortTime, .shortDateMediumTime,
    ])
    func fixedStylesNeverChange(_ style: TimestampStyle) {
        #expect(times("en_US").nextChange(Self.instant, style: style, now: checkA) == nil)
    }

    @Test(arguments: [Int64(8_640_000_000_000), -8_640_000_000_000, .max, .min])
    func theFarFutureFormats(_ seconds: Int64) {
        let times = times("de_DE")
        #expect(!times.text(seconds, style: .fullDateShortTime, now: checkA).isEmpty)
        #expect(!times.text(seconds, style: .relative, now: checkA).isEmpty)
        #expect(!times.fullDate(seconds).isEmpty)
    }
}
