import Foundation
import Testing

@testable import Akari

struct MessageFormatTests {
    let now = berlinTime(2026, 10, 10, 15, 0)
    let locale = Locale(identifier: "en_US")

    func groupTime(_ date: Date) -> String {
        MessageFormat.groupTime(date, now: now, calendar: berlin, locale: locale)
    }

    @Test
    func timestampsSayTodayYesterdayOrTheDate() {
        #expect(groupTime(berlinTime(2026, 10, 10, 14, 5)) == "Today at 2:05\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 9, 23, 59)) == "Yesterday at 11:59\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 3, 14, 5)) == "10/03/2026, 2:05\u{202F}PM")
        #expect(groupTime(berlinTime(2026, 10, 11, 10, 0)) == "10/11/2026, 10:00\u{202F}AM")
        #expect(
            MessageFormat.shortTime(
                berlinTime(2026, 10, 10, 14, 5), calendar: berlin, locale: locale)
                == "2:05\u{202F}PM")
    }

    @Test
    func dayDividersShowTheLongDate() {
        #expect(
            MessageFormat.dayDivider(
                berlinTime(2026, 10, 3, 0, 0), calendar: berlin, locale: locale)
                == "October 3, 2026")
    }
}
