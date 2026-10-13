extension Channel {
    /// Text and announcement channels, DMs, group DMs and threads. Voice, stage, forum, media
    /// and directory channels don't show a message list in Akari yet.
    public var opensMessageList: Bool {
        kind.opensMessageList
    }
}

extension ChannelType {
    /// As `Channel.opensMessageList`.
    public var opensMessageList: Bool {
        switch self {
        case .guildText, .guildNews, .dm, .groupDm, .newsThread, .publicThread, .privateThread:
            true
        default:
            false
        }
    }
}
