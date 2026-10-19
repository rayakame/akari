import Foundation
import Testing

@testable import Akari

struct MessageFormatTests {
    let now = berlinTime(2026, 10, 10, 15, 0)
    let locale = Locale(identifier: "en_US")
    let german = Locale(identifier: "de_DE")

    func groupTime(_ date: Date, _ locale: Locale? = nil) -> String {
        MessageFormat.groupTime(date, now: now, calendar: berlin, locale: locale ?? self.locale)
    }

    func hourCycle(_ identifier: String, _ cycle: Locale.HourCycle) -> Locale {
        var components = Locale.Components(identifier: identifier)
        components.hourCycle = cycle
        return Locale(components: components)
    }

    // The official client shows only the time for today's messages.
    @Test
    func timestampsSayTheTimeTodayYesterdayOrTheDate() {
        #expect(groupTime(berlinTime(2026, 10, 10, 14, 5)) == "2:05\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 10, 0, 0)) == "12:00\u{202F}AM")
        #expect(groupTime(berlinTime(2026, 10, 9, 23, 59)) == "Yesterday at 11:59\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 3, 14, 5)) == "10/3/26, 2:05\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 11, 10, 0)) == "10/11/26, 10:00\u{202F}AM")
        #expect(
            MessageFormat.shortTime(
                berlinTime(2026, 10, 10, 14, 5), calendar: berlin, locale: locale)
                == "2:05\u{202F}PM")
    }

    @Test
    func timesFollowTheLocale() {
        #expect(groupTime(berlinTime(2026, 10, 10, 14, 5), german) == "14:05")
        #expect(groupTime(berlinTime(2026, 10, 9, 23, 59), german) == "Yesterday at 23:59")
        #expect(groupTime(berlinTime(2026, 10, 3, 14, 5), german) == "03.10.26, 14:05")
    }

    // The system's 24-hour setting reaches Akari as the locale's hour cycle.
    @Test
    func theClockFollowsTheHourCycle() {
        let time = berlinTime(2026, 10, 3, 14, 5)
        let english = hourCycle("en_US", .zeroToTwentyThree)
        let german = hourCycle("de_DE", .oneToTwelve)

        #expect(MessageFormat.shortTime(time, calendar: berlin, locale: english) == "14:05")
        #expect(groupTime(time, english) == "10/3/26, 14:05")
        #expect(MessageFormat.shortTime(time, calendar: berlin, locale: german) == "2:05\u{202F}PM")
        #expect(groupTime(time, german) == "03.10.26, 2:05\u{202F}PM")
    }

    // D123, the user's screenshot: "17.10.26, 16:02" in German, "10/17/26, 4:02 PM" in English.
    @Test
    func fullDatesUseTheLocalesShortDateAndTime() {
        let time = berlinTime(2026, 10, 17, 16, 2)

        #expect(groupTime(time, german) == "17.10.26, 16:02")
        #expect(groupTime(time) == "10/17/26, 4:02\u{202F}PM")
        #expect(groupTime(time, Locale(identifier: "en_DE")) == "17.10.26, 16:02")
    }

    // A changed 24-hour or date format setting keeps the locale's identifier.
    @MainActor
    @Test
    func aLocaleChangeDropsTheKeptFormats() {
        _ = groupTime(berlinTime(2026, 10, 3, 14, 5))
        #expect(MessageFormat.keptFormats > 0)

        MessageTableTests().controller.refreshTimes()

        #expect(MessageFormat.keptFormats == 0)
    }

    @Test
    func todayAndYesterdayStay() {
        #expect(groupTime(berlinTime(2026, 10, 10, 9, 30)) == "9:30\u{202F}AM")
        #expect(groupTime(berlinTime(2026, 10, 9, 9, 30)) == "Yesterday at 9:30\u{202F}AM")
        #expect(groupTime(berlinTime(2026, 10, 9, 9, 30), german) == "Yesterday at 09:30")
        #expect(groupTime(berlinTime(2026, 10, 8, 9, 30), german) == "08.10.26, 09:30")
    }

    @Test
    func dayDividersShowTheLocalesLongDate() {
        let day = berlinTime(2026, 10, 3, 0, 0)
        let divider = { (identifier: String) in
            MessageFormat.dayDivider(day, calendar: berlin, locale: Locale(identifier: identifier))
        }

        #expect(divider("en_US") == "October 3, 2026")
        #expect(divider("de_DE") == "3. Oktober 2026")
        // Akari in English on a German Mac: the region's order with the app's words.
        #expect(divider("en_DE") == "3. October 2026")
    }

    @Test
    func fullDatesFollowTheLocale() {
        let time = berlinTime(2026, 10, 3, 14, 5)

        #expect(
            MessageFormat.fullDate(time, calendar: berlin, locale: locale)
                == "Saturday, October 3, 2026 at 2:05\u{202F}PM")
        #expect(
            MessageFormat.fullDate(time, calendar: berlin, locale: german)
                == "Samstag, 3. Oktober 2026 um 14:05")
    }
}
