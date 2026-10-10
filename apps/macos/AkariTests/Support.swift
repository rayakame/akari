import AkariKit
import Foundation

let berlin: Calendar = {
    var calendar = Calendar(identifier: .gregorian)
    calendar.timeZone = TimeZone(identifier: "Europe/Berlin")!
    calendar.locale = Locale(identifier: "en_US")
    return calendar
}()

func berlinTime(_ year: Int, _ month: Int, _ day: Int, _ hour: Int, _ minute: Int) -> Date {
    berlin.date(
        from: DateComponents(year: year, month: month, day: day, hour: hour, minute: minute))!
}

func author(_ raw: UInt64, _ name: String = "Mira", bot: Bool = false) -> User {
    User(
        id: UserId(rawValue: raw), username: name.lowercased(), globalName: name,
        displayName: name, bot: bot, system: false)
}

func row(
    _ raw: UInt64, by user: User = author(1), at time: Date, kind: MessageType = .default,
    content: String? = nil, key: UInt64? = nil
) -> MessageListModel.Row {
    let message = Message(
        id: MessageId(rawValue: raw), channelId: ChannelId(rawValue: 10), kind: kind,
        author: user, fromWebhook: false, content: content ?? "message \(raw)", timestamp: time,
        editedTimestamp: nil, pinned: false, mentionEveryone: false, attachments: [],
        embedCount: 0, stickerNames: [], delivery: .sent)
    return MessageListModel.Row(id: MessageId(rawValue: key ?? raw), message: message)
}
