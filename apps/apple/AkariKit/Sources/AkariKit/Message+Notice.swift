extension Message {
    /// A piece of a notice: the parts Discord draws as names and links, and the text between.
    public enum NoticePart: Equatable, Sendable {
        case text(String)
        /// Who did it.
        case actor(String)
        /// A new thread's name, which opens it; `nil` when the message doesn't say which.
        case thread(name: String, id: ChannelId?)
        /// "threads" in "See all threads.", for the channel's threads.
        case threads(String)
        /// "a message" in a pin's line, which jumps to the pinned message.
        case message(String, id: MessageId)
        /// "pinned messages" in "See all pinned messages.", for the channel's pins.
        case pins(String)

        var text: String {
            switch self {
            case .text(let text), .actor(let text), .thread(let text, _), .threads(let text),
                .message(let text, _), .pins(let text):
                text
            }
        }
    }

    /// What a system message says in parts, e.g. "Mira" and " joined the server."; `nil` for
    /// messages people write (default, reply, commands).
    public var noticeParts: [NoticePart]? {
        // Discord draws the actor of each line observed as a name (TB3.4).
        func named(_ rest: String) -> [NoticePart] {
            [.actor(author.displayName), .text(rest)]
        }
        switch kind {
        case .default, .reply, .chatInputCommand, .contextMenuCommand, .threadStarterMessage,
            .unknown:
            return nil
        case .userJoin: return named(" joined the server.")
        // Discord's wording (TB3.4); without the reference "a message" isn't a link.
        case .channelPinnedMessage:
            let pinned: [NoticePart] =
                if let id = reference?.messageId {
                    [
                        .text(" pinned "), .message("a message", id: id),
                        .text(" to this channel. See all "),
                    ]
                } else {
                    [.text(" pinned a message to this channel. See all ")]
                }
            return [.actor(author.displayName)] + pinned + [.pins("pinned messages"), .text(".")]
        case .premiumGuildSubscription: return named(" boosted the server.")
        case .premiumGuildSubscriptionTier1: return named(Self.boosted(level: 1))
        case .premiumGuildSubscriptionTier2: return named(Self.boosted(level: 2))
        case .premiumGuildSubscriptionTier3: return named(Self.boosted(level: 3))
        case .recipientAdd: return named(" added someone to the group.")
        case .recipientRemove: return named(" left the group or removed someone.")
        case .call: return named(" started a call.")
        case .channelNameChange: return named(" changed the channel name: \(content)")
        case .channelIconChange: return named(" changed the channel icon.")
        // Discord's wording (Q38).
        case .threadCreated:
            return [
                .actor(author.displayName), .text(" started a thread: "),
                .thread(name: content, id: reference?.channelId), .text(". See all "),
                .threads("threads"), .text("."),
            ]
        case .channelFollowAdd: return named(" followed this channel's announcements.")
        case .autoModerationAction: return [.text("AutoMod blocked or flagged a message.")]
        case .stageStart: return named(" started the stage.")
        case .stageEnd: return named(" ended the stage.")
        case .pollResult: return [.text("A poll has ended.")]
        default:
            return [.text(content.isEmpty ? "System message" : "System message: \(content)")]
        }
    }

    /// What a system message says, e.g. "Mira joined the server."; `nil` for messages people
    /// write.
    public var notice: String? {
        noticeParts?.map(\.text).joined()
    }

    private static func boosted(level: Int) -> String {
        " boosted the server. It reached level \(level)."
    }
}
