import Foundation

/// A message list as a table shows it: day dividers and where author groups start, by the rule
/// in `docs/ui/message-list.md`.
public struct MessageTimeline: Equatable, Sendable {
    /// The ends of a list: more history beyond them, or where the channel begins.
    public enum Edge: Hashable, Sendable {
        case older
        case newer
        /// In place of `.older` once the oldest message is loaded.
        case beginning
    }

    public enum ItemId: Hashable, Sendable {
        case edge(Edge)
        case day(Date)
        case message(MessageId)
    }

    public enum Item: Identifiable, Equatable, Sendable {
        /// First (older) or last (newer); its look comes from the list's load state.
        case edge(Edge)
        /// Before a day's first message; the date is that day's start.
        case day(Date)
        /// `startsGroup` shows the avatar, the name and the time.
        case message(MessageListModel.Row, startsGroup: Bool)

        public var id: ItemId {
            switch self {
            case .edge(let edge): .edge(edge)
            case .day(let start): .day(start)
            case .message(let row, _): .message(row.id)
            }
        }
    }

    /// Messages this far apart or more start a new group: 7 minutes.
    public static let groupInterval: TimeInterval = 7 * 60

    public let items: [Item]

    /// `edges` wrap the rows: `.beginning` or `.older` first, `.newer` last. Without rows only
    /// `.older` stays, for messages that aren't loaded yet.
    public init(
        rows: [MessageListModel.Row], calendar: Calendar = .current, edges: Set<Edge> = []
    ) {
        var items: [Item] = []
        if rows.isEmpty {
            self.items =
                edges.contains(.older) && !edges.contains(.beginning) ? [.edge(.older)] : []
            return
        }
        if let top = [Edge.beginning, .older].first(where: edges.contains) {
            items.append(.edge(top))
        }
        var previous: Message?
        var day: Date?
        for row in rows {
            let start = calendar.startOfDay(for: row.message.timestamp)
            // Only forward, so a message stamped earlier than the one before (a skewed clock)
            // can't repeat a divider's key.
            let newDay = day.map { start > $0 } ?? true
            if newDay {
                items.append(.day(start))
                day = start
            }
            let startsGroup = newDay || Self.startsGroup(row.message, after: previous)
            items.append(.message(row, startsGroup: startsGroup))
            previous = row.message
        }
        if edges.contains(.newer) {
            items.append(.edge(.newer))
        }
        self.items = items
    }

    private static func startsGroup(_ message: Message, after previous: Message?) -> Bool {
        guard let previous else {
            return true
        }
        if message.notice != nil || previous.notice != nil {
            return true
        }
        if message.author.id != previous.author.id {
            return true
        }
        // One webhook can post under several names.
        if message.fromWebhook && previous.fromWebhook
            && message.author.username != previous.author.username
        {
            return true
        }
        switch message.kind {
        case .reply, .chatInputCommand, .contextMenuCommand:
            return true
        default:
            return message.timestamp.timeIntervalSince(previous.timestamp) >= groupInterval
        }
    }
}
