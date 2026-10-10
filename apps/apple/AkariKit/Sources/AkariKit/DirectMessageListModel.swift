import Observation

/// The DM list at home.
@MainActor @Observable
public final class DirectMessageListModel {
    public struct Conversation: Identifiable, Equatable, Sendable {
        public let channel: Channel
        /// Without the current user, in the channel's order; users the store doesn't know
        /// are left out.
        public let recipients: [User]
        /// A DM's recipient; a group's name, else its recipients' names.
        public let name: String

        public var id: ChannelId { channel.id }

        /// Everyone in the conversation, the current user included, known to the store or not.
        public var memberCount: Int { channel.recipientIds.count + 1 }
    }

    /// The latest conversation first.
    public private(set) var conversations: [Conversation] = []

    @ObservationIgnored private let store: Store
    // A `nil` value is a recipient the store doesn't know, so it isn't asked again.
    @ObservationIgnored private var users: [UserId: User?] = [:]

    init(store: Store) {
        self.store = store
    }

    func reload() {
        rebuild(rereading: nil)
    }

    func apply(_ batch: EventBatch) {
        if batch.ready {
            return reload()
        }
        let renamed = batch.usersChanged.filter { users[$0] != nil }
        for id in renamed {
            users[id] = .some(store.user(id: id))
        }
        if let changed = batch.channelsChanged[nil] {
            rebuild(rereading: changed)
        } else if !renamed.isEmpty {
            conversations = conversations.map { conversation($0.channel) }
        }
    }

    // `nil` re-reads every channel and recipient, e.g. after a new session.
    private func rebuild(rereading changed: Set<ChannelId>?) {
        let ids = store.privateChannelList()
        var held = Dictionary(conversations.map { ($0.id, $0.channel) }) { first, _ in first }
        if changed == nil {
            users = [:]
        }
        let unread = ids.filter { id in
            held[id] == nil || changed?.contains(id) != false
        }
        if !unread.isEmpty {
            for channel in store.channels(ids: unread) {
                held[channel.id] = channel
            }
        }
        let channels = ids.compactMap { held[$0] }
        let recipients = Set(channels.flatMap(\.recipientIds))
        users = users.filter { recipients.contains($0.key) }
        for id in channels.flatMap(\.recipientIds) where users[id] == nil {
            users[id] = .some(store.user(id: id))
        }
        conversations = channels.map(conversation)
    }

    private func conversation(_ channel: Channel) -> Conversation {
        let recipients = channel.recipientIds.compactMap { users[$0] ?? nil }
        return Conversation(
            channel: channel, recipients: recipients, name: Self.name(channel, recipients))
    }

    private static func name(_ channel: Channel, _ recipients: [User]) -> String {
        guard channel.kind == .groupDm else {
            return recipients.first?.displayName ?? "Unknown user"
        }
        if let name = channel.name, !name.isEmpty {
            return name
        }
        if recipients.isEmpty {
            return "Unnamed group"
        }
        return recipients.map(\.displayName).joined(separator: ", ")
    }
}
