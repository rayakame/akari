import Foundation

/// The box under the message a thread started from, or under the line about a thread started
/// without one, as Discord's client draws it.
public struct ThreadBox: Equatable, Sendable {
    /// Its second line.
    public enum Bottom: Equatable, Sendable {
        /// The thread's newest message, by `authorName`.
        case latest(Message, authorName: String)
        /// "There are no recent messages in this thread."
        case noRecentMessages
        /// "There are no messages in this thread yet."
        case noMessages
    }

    public struct RelativeTime: Equatable, Sendable {
        public let text: String
        /// When `text` changes; `nil` once it's a date.
        public let nextChange: Date?
    }

    public static let noRecentMessages = "There are no recent messages in this thread."
    public static let noMessages = "There are no messages in this thread yet."

    public let threadId: ChannelId
    public let name: String
    /// "16 Messages ›", "See Thread ›"
    public let link: String
    public let bottom: Bottom
    /// When the thread last had a message, shown after the second line; `nil` shows none.
    public let time: Date?

    public init(threadId: ChannelId, name: String, link: String, bottom: Bottom, time: Date?) {
        self.threadId = threadId
        self.name = name
        self.link = link
        self.bottom = bottom
        self.time = time
    }

    // Discord counted only up to 50 before 2022-07-01.
    private static let cappedBefore: UInt64 = 992_549_565_104_128_000

    /// `nil` when the message has no thread or the store doesn't have it yet.
    static func of(_ message: Message, in store: Store, name: (User) -> String) -> ThreadBox? {
        guard message.hasThread,
            let thread = store.channel(id: ChannelId(rawValue: message.id.rawValue)),
            let info = thread.thread
        else {
            return nil
        }
        let bottom: Bottom
        if info.archived {
            bottom = .noRecentMessages
        } else if (info.messageCount ?? 0) == 0 {
            bottom = .noMessages
        } else if let latest = store.latestMessage(threadId: thread.id) {
            bottom = .latest(
                latest, authorName: latest.fromWebhook ? latest.author.displayName : name(latest.author))
        } else {
            bottom = .noRecentMessages
        }
        let lastActivity =
            info.lastMessageId.map { Self.time(of: $0.rawValue) } ?? info.createdAt
            ?? Self.time(of: thread.id.rawValue)
        return ThreadBox(
            threadId: thread.id, name: thread.name ?? "",
            link: link(count: info.messageCount, threadId: thread.id), bottom: bottom,
            time: info.archived || bottom == .noMessages ? nil : lastActivity)
    }

    public static func link(count: UInt32?, threadId: ChannelId) -> String {
        guard let count, count > 0 else {
            return "See Thread ›"
        }
        let number =
            if threadId.rawValue <= cappedBefore && count >= 50 {
                "50+"
            } else if count >= 100_000 {
                "100k+"
            } else {
                String(count)
            }
        return number == "1" ? "1 Message ›" : "\(number) Messages ›"
    }

    /// What the second line says for a newest message without content; `nil` when it has some.
    public static func placeholder(for message: Message) -> String? {
        if message.kind == .channelPinnedMessage {
            return "Message pinned."
        }
        guard message.content.isEmpty else {
            return nil
        }
        if !message.stickerNames.isEmpty {
            return "Click to see sticker"
        }
        if message.kind == .chatInputCommand || message.kind == .contextMenuCommand {
            return "Click to see command"
        }
        if message.componentsV2 {
            return "Click to see message"
        }
        return "Click to see attachment"
    }

    /// "1m ago" under a minute too, up to "29d ago", then ">30d ago", and the date after 360 days.
    public static func relativeTime(
        since date: Date, now: Date, locale: Locale, timeZone: TimeZone = .current
    ) -> RelativeTime {
        let seconds = max(0, Int(now.timeIntervalSince(date)))
        let (minute, hour, day) = (60, 3600, 86_400)
        func next(_ at: Int) -> Date {
            date + TimeInterval(at)
        }
        switch seconds {
        case ..<(2 * minute):
            return RelativeTime(text: "1m ago", nextChange: next(2 * minute))
        case ..<hour:
            let minutes = seconds / minute
            return RelativeTime(text: "\(minutes)m ago", nextChange: next((minutes + 1) * minute))
        case ..<day:
            let hours = seconds / hour
            return RelativeTime(text: "\(hours)h ago", nextChange: next((hours + 1) * hour))
        case ..<(30 * day):
            let days = seconds / day
            return RelativeTime(text: "\(days)d ago", nextChange: next((days + 1) * day))
        case ..<(360 * day):
            return RelativeTime(text: ">30d ago", nextChange: next(360 * day))
        default:
            let formatter = DateFormatter()
            formatter.locale = locale
            formatter.timeZone = timeZone
            formatter.dateStyle = .long
            formatter.timeStyle = .none
            return RelativeTime(text: formatter.string(from: date), nextChange: nil)
        }
    }

    // Discord's IDs carry their time: milliseconds since 2015, above the low 22 bits.
    static func time(of raw: UInt64) -> Date {
        Date(timeIntervalSince1970: Double((raw >> 22) + 1_420_070_400_000) / 1000)
    }
}
