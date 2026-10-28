extension Channel {
    /// Text and announcement channels, DMs, group DMs and threads. Voice, stage, media and
    /// directory channels don't show a message list in Akari yet, and forums show their page.
    public var opensMessageList: Bool {
        kind.opensMessageList
    }

    /// Forum channels, whose page lists their posts in the message list's place. Media
    /// channels don't open yet.
    public var opensForum: Bool {
        kind.opensForum
    }

    /// A message list or a forum's page.
    public var opensPage: Bool {
        kind.opensPage
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

    /// As `Channel.opensForum`.
    public var opensForum: Bool {
        self == .guildForum
    }

    /// As `Channel.opensPage`.
    public var opensPage: Bool {
        opensMessageList || opensForum
    }
}
