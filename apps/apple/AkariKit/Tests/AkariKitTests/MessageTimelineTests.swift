import Foundation
import Testing

@testable import AkariKit

struct MessageTimelineTests {
    let noon = berlinTime(2026, 10, 10, 12, 0)

    func timeline(_ rows: MessageListModel.Row...) -> MessageTimeline {
        MessageTimeline(rows: rows, calendar: berlin)
    }

    @Test
    func oneAuthorWithinSevenMinutesSharesAGroup() {
        let shown = timeline(
            row(1, at: noon), row(2, at: noon + 60), row(3, at: noon + 60 + 419))

        #expect(shown.shape == ["day", "+1", "2", "3"])
    }

    @Test
    func sevenMinutesStartAGroup() {
        let shown = timeline(row(1, at: noon), row(2, at: noon + 420))

        #expect(shown.shape == ["day", "+1", "+2"])
        #expect(MessageTimeline.groupInterval == 420)
    }

    @Test
    func anotherAuthorOrWebhookNameStartsAGroup() {
        let hook = { (name: String) in user(9, name: name) }

        let shown = timeline(
            row(1, by: user(1), at: noon), row(2, by: user(2), at: noon + 1),
            row(3, by: hook("Alerts"), at: noon + 2, webhook: true),
            row(4, by: hook("Alerts"), at: noon + 3, webhook: true),
            row(5, by: hook("Builds"), at: noon + 4, webhook: true))

        #expect(shown.shape == ["day", "+1", "+2", "+3", "4", "+5"])
    }

    @Test
    func repliesCommandsAndNoticesStartGroupsAndNoticesEndThem() {
        let shown = timeline(
            row(1, at: noon), row(2, at: noon + 1, kind: .reply),
            row(3, at: noon + 2, kind: .chatInputCommand),
            row(4, at: noon + 3, kind: .contextMenuCommand), row(5, at: noon + 4),
            row(6, at: noon + 5, kind: .userJoin), row(7, at: noon + 6))

        #expect(shown.shape == ["day", "+1", "+2", "+3", "+4", "5", "+6", "+7"])
    }

    @Test
    func aNewDayGetsADividerAndStartsAGroup() {
        let shown = timeline(
            row(1, at: berlinTime(2026, 10, 9, 23, 59)), row(2, at: berlinTime(2026, 10, 10, 0, 1)))

        #expect(shown.shape == ["day", "+1", "day", "+2"])
        #expect(
            shown.items.map(\.id).first == .day(berlinTime(2026, 10, 9, 0, 0)))
    }

    @Test
    func daysFollowTheLocalCalendarAcrossDaylightSaving() {
        let spring = timeline(
            row(1, at: berlinTime(2026, 3, 28, 23, 30)), row(2, at: berlinTime(2026, 3, 29, 1, 59)),
            row(3, at: berlinTime(2026, 3, 29, 3, 1)), row(4, at: berlinTime(2026, 3, 30, 0, 10)))
        let autumnFirst = berlinTime(2026, 10, 25, 2, 30)
        let autumn = timeline(
            row(5, at: berlinTime(2026, 10, 24, 23, 50)), row(6, at: autumnFirst),
            row(7, at: autumnFirst + 3600), row(8, at: berlinTime(2026, 10, 26, 0, 0)))

        // 01:59 to 03:01 across the spring jump is two minutes, so 3 continues 2's group.
        #expect(spring.shape == ["day", "+1", "day", "+2", "3", "day", "+4"])
        #expect(autumn.shape == ["day", "+5", "day", "+6", "+7", "day", "+8"])
        #expect(
            spring.items.map(\.id).filter { if case .day = $0 { true } else { false } }
                == [28, 29, 30].map { .day(berlinTime(2026, 3, $0, 0, 0)) })
    }

    @Test
    func aMessageOutOfTimeOrderGetsNoSecondDivider() {
        let tomorrow = noon + 86_400

        let shown = timeline(row(1, at: noon), row(2, at: tomorrow), row(3, at: noon + 60))

        #expect(shown.shape == ["day", "+1", "day", "+2", "3"])
        #expect(Set(shown.items.map(\.id)).count == shown.items.count)
    }

    @Test
    func dividersKeepTheirKeyWhenTheDaysFirstMessageLeaves() {
        let both = timeline(row(1, at: noon), row(2, at: noon + 3600))
        let second = timeline(row(2, at: noon + 3600))

        #expect(both.items.first?.id == second.items.first?.id)
        #expect(second.shape == ["day", "+2"])
    }
}
