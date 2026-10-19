extension Channel {
    /// Text and announcement channels, DMs, group DMs and threads. Voice, stage, forum, media
    /// and directory channels don't show a message list in Akari yet.
    public var opensMessageList: Bool {
        kind.opensMessageList
    }

    /// Announcement, public and private threads, listed under their parent channel.
    public var isThread: Bool {
        kind.isThread
    }
}

extension ChannelType {
    /// As `Channel.isThread`.
    public var isThread: Bool {
        switch self {
        case .newsThread, .publicThread, .privateThread: true
        default: false
        }
    }

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
