import Foundation

@testable import AkariKit

let berlin: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "Europe/Berlin")!
    calendar.locale = Locale(identifier: "en_US_POSIX")
    return calendar
}()

func berlinTime(_ year: Int, _ month: Int, _ day: Int, _ hour: Int, _ minute: Int) -> Date {
    berlin.date(
        from: DateComponents(year: year, month: month, day: day, hour: hour, minute: minute))!
}

func row(
    _ raw: UInt64, by author: User = user(1), at time: Date, kind: MessageType = .default,
    webhook: Bool = false, key: UInt64? = nil, delivery: Delivery = .sent
) -> MessageListModel.Row {
    let message = Message(
        id: id(raw), channelId: id(10), kind: kind, author: author, fromWebhook: webhook,
        content: "message \(raw)", timestamp: time, editedTimestamp: nil, pinned: false,
        mentionEveryone: false, attachments: [], embedCount: 0, stickerNames: [],
        delivery: delivery
    )
    return MessageListModel.Row(id: id(key ?? raw), message: message)
}

extension MessageTimeline {
    // "day" for a divider, "+id" for a group's first message, "id" for the rest.
    var shape: [String] {
        items.map { item in
            switch item {
            case .day: "day"
            case .message(let row, let startsGroup):
                startsGroup ? "+\(row.id.rawValue)" : "\(row.id.rawValue)"
            }
        }
    }
}
