extension Message {
    /// What a system message says, in Akari's words, e.g. "Mira pinned a message to this
    /// channel."; `nil` for messages people write (default, reply, commands).
    public var notice: String? {
        let name = author.displayName
        switch kind {
        case .default, .reply, .chatInputCommand, .contextMenuCommand, .threadStarterMessage,
            .unknown:
            return nil
        case .userJoin: return "\(name) joined the server."
        case .channelPinnedMessage: return "\(name) pinned a message to this channel."
        case .premiumGuildSubscription: return "\(name) boosted the server."
        case .premiumGuildSubscriptionTier1: return Self.boost(name, level: 1)
        case .premiumGuildSubscriptionTier2: return Self.boost(name, level: 2)
        case .premiumGuildSubscriptionTier3: return Self.boost(name, level: 3)
        case .recipientAdd: return "\(name) added someone to the group."
        case .recipientRemove: return "\(name) left the group or removed someone."
        case .call: return "\(name) started a call."
        case .channelNameChange: return "\(name) changed the channel name: \(content)"
        case .channelIconChange: return "\(name) changed the channel icon."
        case .threadCreated: return "\(name) started a thread: \(content)"
        case .channelFollowAdd: return "\(name) followed this channel's announcements."
        case .autoModerationAction: return "AutoMod blocked or flagged a message."
        case .stageStart: return "\(name) started the stage."
        case .stageEnd: return "\(name) ended the stage."
        case .pollResult: return "A poll has ended."
        default: return content.isEmpty ? "System message" : "System message: \(content)"
        }
    }

    private static func boost(_ name: String, level: Int) -> String {
        "\(name) boosted the server. It reached level \(level)."
    }
}
