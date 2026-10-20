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
        /// Keyed by the row it stands before.
        case newMessages(MessageId)
        case message(MessageId)
    }

    public enum Item: Identifiable, Equatable, Sendable {
        /// First (older) or last (newer); its look comes from the list's load state.
        case edge(Edge)
        /// Before a day's first message; the date is that day's start.
        case day(Date)
        /// The new messages line, right before the row with this ID and after its day's divider,
        /// or last while that row is a thread's starter being fetched. It starts no group.
        case newMessages(MessageId)
        /// `startsGroup` shows the avatar, the name and the time.
        case message(MessageListModel.Row, startsGroup: Bool)

        public var id: ItemId {
            switch self {
            case .edge(let edge): .edge(edge)
            case .day(let start): .day(start)
            case .newMessages(let id): .newMessages(id)
            case .message(let row, _): .message(row.id)
            }
        }
    }

    /// Messages this far apart or more start a new group: 7 minutes.
    public static let groupInterval: TimeInterval = 7 * 60

    public let items: [Item]

    /// `edges` wrap the rows: `.beginning` or `.older` first, `.newer` last. Without rows only
    /// `.older` stays, for messages that aren't loaded yet. The new messages line goes before the
    /// row whose ID is `newMessages`, or the next shown one; last if none is.
    public init(
        rows: [MessageListModel.Row], calendar: Calendar = .current, edges: Set<Edge> = [],
        newMessages: MessageId? = nil
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
        var previous: MessageListModel.Row?
        var day: Date?
        var line: MessageId?
        for row in rows {
            if row.id == newMessages {
                line = row.id
            }
            // A starter still being fetched has no row yet; the line goes before the next one, or
            // last.
            if row.starter == .loading {
                continue
            }
            let start = calendar.startOfDay(for: row.shown.timestamp)
            // Only forward, so a message stamped earlier than the one before (a skewed clock)
            // can't repeat a divider's key.
            let newDay = day.map { start > $0 } ?? true
            if newDay {
                items.append(.day(start))
                day = start
            }
            if let placed = line {
                items.append(.newMessages(placed))
                line = nil
            }
            let startsGroup = newDay || Self.startsGroup(row, after: previous)
            items.append(.message(row, startsGroup: startsGroup))
            previous = row
        }
        // The unread bar counts a starter still being fetched.
        if let placed = line {
            items.append(.newMessages(placed))
        }
        if edges.contains(.newer) {
            items.append(.edge(.newer))
        }
        self.items = items
    }

    private static func startsGroup(
        _ row: MessageListModel.Row, after previousRow: MessageListModel.Row?
    ) -> Bool {
        guard let previousRow else {
            return true
        }
        if row.notice != nil || previousRow.notice != nil {
            return true
        }
        // A thread's starter is a group of its own, whatever comes after it.
        if row.starter != nil || previousRow.starter != nil {
            return true
        }
        let message = row.shown
        let previous = previousRow.shown
        // A message with a thread starts a group, and its thread's box ends it (D124).
        if message.hasThread || previous.hasThread {
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
